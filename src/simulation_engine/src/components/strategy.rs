//! The strategy the simulated path trades, and the microstructure it reacts to.
//!
//! Parity with the live path is the entire point of this file. The entry rule, the
//! stop/target geometry and the sizing arithmetic mirror
//! `alpha_engine::logic::signal_generator::SignalGenerator` and
//! `alpha_engine::logic::position_sizing::PositionSizingEngine`: a backtest of a
//! different rule than the one that trades live is not a forecast, it is fiction.
//! Where the simulation must diverge, it is commented at the site and tested.

use crate::core::constants::{
    MIN_NOTIONAL_USDT, SIM_ORDER_TTL_MS, SIM_SIGNAL_COOLDOWN_MS, SIM_STOP_DISTANCE_PCT,
    SIM_TAKE_PROFIT_PCT,
};
use crate::core::{OrderBookSnapshot, StrategyParams, TradeEvent, TradeIntent, TradeSide};
use std::collections::{HashMap, VecDeque};

/// Order Book Imbalance, ported verbatim from
/// `alpha_engine::features::order_book_imbalance::calculate_obi`.
///
/// Formula: OBI = (Sum(Bid_Volumes) - Sum(Ask_Volumes)) / (Sum(Bid) + Sum(Ask)),
/// over the top `levels` of each side, in [-1.0, 1.0].
pub fn calculate_obi(order_book: &OrderBookSnapshot, levels: usize) -> f64 {
    let levels = levels
        .min(order_book.bids.len())
        .min(order_book.asks.len());

    if levels == 0 {
        return 0.0;
    }

    let bid_volume: f64 = order_book
        .bids
        .iter()
        .take(levels)
        .map(|level| level.quantity)
        .sum();
    let ask_volume: f64 = order_book
        .asks
        .iter()
        .take(levels)
        .map(|level| level.quantity)
        .sum();

    let total_volume = bid_volume + ask_volume;

    if total_volume == 0.0 {
        return 0.0;
    }

    (bid_volume - ask_volume) / total_volume
}

/// A position the simulated path has open and is managing.
#[derive(Debug, Clone)]
pub struct OpenPosition {
    pub symbol: String,
    pub side: TradeSide,
    pub entry_price: f64,
    pub quantity: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub entry_timestamp: u64,
}

/// Why an exit was taken. Reported so a run's win rate can be attributed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    StopLoss,
    TakeProfit,
}

/// An exit the market data triggered.
///
/// Exits cross the spread rather than queueing, so the intent carries the level
/// that *triggered* it and the engine supplies the print's price as the execution
/// price; the gap between the two is the exit slippage the report shows.
#[derive(Debug, Clone)]
pub struct TriggeredExit {
    pub intent: TradeIntent,
    pub reason: ExitReason,
}

/// Why a snapshot produced no order.
///
/// Counted rather than swallowed: "the strategy never traded" and "every order
/// failed at the exchange floor" used to look identical (and both looked like a
/// successful backtest), which is precisely the failure a simulator must not hide.
/// `Hash` so the replay can tally them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rejection {
    BelowThreshold,
    OrderInFlight,
    AlreadyOpen,
    Cooldown,
    VolatilityGate,
    /// Sized below the exchange's minimum notional, which Layer 3 would reject.
    BelowMinNotional,
    NoTopOfBook,
}

/// The outcome of evaluating one snapshot.
#[derive(Debug, Clone, PartialEq)]
pub enum EntryDecision {
    Signal(TradeIntent),
    Rejected(Rejection),
}

/// OBI threshold strategy with one position per symbol.
pub struct ObiSimStrategy {
    params: StrategyParams,
    open: HashMap<String, OpenPosition>,
    /// Symbols with an entry order working the queue: a fresh signal must not stack
    /// behind one that has not been decided yet.
    pending_entry: HashMap<String, ()>,
    last_signal_ms: HashMap<String, i64>,
    last_trade_price: HashMap<String, f64>,
    volatility_samples: HashMap<String, VecDeque<f64>>,
}

impl ObiSimStrategy {
    pub fn new(params: StrategyParams) -> Self {
        Self {
            params,
            open: HashMap::new(),
            pending_entry: HashMap::new(),
            last_signal_ms: HashMap::new(),
            last_trade_price: HashMap::new(),
            volatility_samples: HashMap::new(),
        }
    }

    pub fn params(&self) -> &StrategyParams {
        &self.params
    }

    /// Used by the robustness sweep to re-run a config with a perturbed knob.
    pub fn set_params(&mut self, params: StrategyParams) {
        self.params = params;
    }

    pub fn open_positions(&self) -> impl Iterator<Item = &OpenPosition> {
        self.open.values()
    }

    /// Entry decision for one book snapshot.
    ///
    /// `equity` is the account mark at this point in the replay, so sizing follows
    /// the compounding (and the drawdowns) the run actually produced rather than
    /// the starting capital.
    pub fn on_snapshot(
        &mut self,
        book: &OrderBookSnapshot,
        equity: f64,
        leverage: f64,
    ) -> EntryDecision {
        if self.pending_entry.contains_key(&book.symbol) {
            return EntryDecision::Rejected(Rejection::OrderInFlight);
        }
        if self.open.contains_key(&book.symbol) {
            return EntryDecision::Rejected(Rejection::AlreadyOpen);
        }

        let now = book.timestamp;
        if let Some(&last) = self.last_signal_ms.get(&book.symbol) {
            if now < last + SIM_SIGNAL_COOLDOWN_MS as i64 {
                return EntryDecision::Rejected(Rejection::Cooldown);
            }
        }

        let obi = calculate_obi(book, self.params.obi_levels);
        let side = if obi > self.params.obi_threshold {
            TradeSide::Buy
        } else if obi < -self.params.obi_threshold {
            TradeSide::Sell
        } else {
            return EntryDecision::Rejected(Rejection::BelowThreshold);
        };

        // The gate is opt-in (0 disables it) because a threshold that depends on the
        // instrument must be configured, not assumed.
        if self.params.max_volatility > 0.0 && self.volatility(&book.symbol) > self.params.max_volatility {
            return EntryDecision::Rejected(Rejection::VolatilityGate);
        }

        // Layer 2 quotes a buy at the best bid and a sell at the best ask, i.e. it
        // works as a maker; the stop and target are measured from that level.
        let entry_price = match side {
            TradeSide::Buy => book.bids.first().map(|level| level.price),
            TradeSide::Sell => book.asks.first().map(|level| level.price),
        };
        let entry_price = match entry_price {
            Some(price) if price > 0.0 => price,
            _ => return EntryDecision::Rejected(Rejection::NoTopOfBook),
        };

        let (stop_loss, take_profit) = match side {
            TradeSide::Buy => (
                entry_price * (1.0 - SIM_STOP_DISTANCE_PCT),
                entry_price * (1.0 + SIM_TAKE_PROFIT_PCT),
            ),
            // Short: the stop is above the entry and the target below it.
            TradeSide::Sell => (
                entry_price * (1.0 + SIM_STOP_DISTANCE_PCT),
                entry_price * (1.0 - SIM_TAKE_PROFIT_PCT),
            ),
        };

        let quantity = self.size_position(equity, leverage, entry_price, stop_loss);
        if quantity <= 0.0 {
            return EntryDecision::Rejected(Rejection::BelowMinNotional);
        }

        self.last_signal_ms.insert(book.symbol.clone(), now);
        self.pending_entry.insert(book.symbol.clone(), ());

        EntryDecision::Signal(TradeIntent {
            symbol: book.symbol.clone(),
            side,
            price: entry_price,
            size: quantity,
            stop_loss,
            take_profit,
            time_to_live: SIM_ORDER_TTL_MS,
            // Clamped: the intent's clock is milliseconds since the epoch, and a
            // negative sample (hand-built fixtures) would otherwise wrap on cast and
            // make every time-to-live check expire instantly.
            timestamp: now.max(0) as u64,
        })
    }

    /// Fixed-fractional sizing, mirroring Layer 2's `size_position`: risk the
    /// configured share of equity between entry and stop, then cap the notional.
    ///
    /// Divergence, deliberately: Layer 2 *inflates* a too-small position to reach
    /// the exchange minimum, while the execution layer rejects orders under it
    /// (`execution_engine::MarginLeverageGuard`, `MIN_NOTIONAL`). Sizing a fill the
    /// exchange would refuse would book a trade the live path never had, so the
    /// simulation reports the rejection instead.
    fn size_position(&self, equity: f64, leverage: f64, entry_price: f64, stop_price: f64) -> f64 {
        if entry_price <= 0.0 || equity <= 0.0 {
            return 0.0;
        }

        let stop_distance = (entry_price - stop_price).abs();
        if stop_distance <= 0.0 {
            return 0.0;
        }

        let risk_amount = equity * self.params.risk_percentage;
        let mut quantity = risk_amount / stop_distance;

        let notional_cap = self
            .params
            .max_position_value
            .min(equity * leverage.max(1.0));
        if quantity * entry_price > notional_cap {
            quantity = notional_cap / entry_price;
        }

        // Rounded to the exchange's step size, as Layer 2 does. Rounding (rather than
        // flooring) is what keeps the simulated fillable size identical to the size
        // the live path would submit; it can exceed the notional cap by at most one
        // step, which is the same trade-off the live sizer accepts.
        if self.params.size_step > 0.0 {
            quantity = (quantity / self.params.size_step).round() * self.params.size_step;
        }

        if quantity * entry_price < MIN_NOTIONAL_USDT {
            return 0.0;
        }

        quantity
    }

    /// Feed one trade print to the volatility estimator. Samples are the absolute
    /// relative move between consecutive prints for that symbol.
    pub fn on_trade_print(&mut self, trade: &TradeEvent) {
        let previous = self.last_trade_price.insert(trade.symbol.clone(), trade.price);

        let previous = match previous {
            Some(price) if price > 0.0 => price,
            _ => return,
        };

        let samples = self.volatility_samples.entry(trade.symbol.clone()).or_default();
        samples.push_back(((trade.price - previous) / previous).abs());
        while samples.len() > self.params.volatility_window.max(1) {
            samples.pop_front();
        }
    }

    /// Root-mean-square of the sampled relative moves.
    fn volatility(&self, symbol: &str) -> f64 {
        let samples = match self.volatility_samples.get(symbol) {
            Some(samples) if !samples.is_empty() => samples,
            _ => return 0.0,
        };

        let sum_squares: f64 = samples.iter().map(|sample| sample * sample).sum();
        (sum_squares / samples.len() as f64).sqrt()
    }

    /// Stop/target check for one trade print.
    pub fn check_exits(&mut self, trade: &TradeEvent) -> Option<TriggeredExit> {
        // Cloned so the map is free to mutate again below.
        let position = self.open.get(&trade.symbol).cloned()?;

        let triggered = match position.side {
            TradeSide::Buy if trade.price <= position.stop_loss => {
                Some((position.stop_loss, ExitReason::StopLoss))
            }
            TradeSide::Buy if trade.price >= position.take_profit => {
                Some((position.take_profit, ExitReason::TakeProfit))
            }
            TradeSide::Sell if trade.price >= position.stop_loss => {
                Some((position.stop_loss, ExitReason::StopLoss))
            }
            TradeSide::Sell if trade.price <= position.take_profit => {
                Some((position.take_profit, ExitReason::TakeProfit))
            }
            _ => None,
        }?;

        let (trigger_price, reason) = triggered;
        self.open.remove(&trade.symbol);

        Some(TriggeredExit {
            reason,
            intent: TradeIntent {
                symbol: position.symbol,
                // Closing a long is a sell, and vice versa.
                side: match position.side {
                    TradeSide::Buy => TradeSide::Sell,
                    TradeSide::Sell => TradeSide::Buy,
                },
                price: trigger_price,
                size: position.quantity,
                // Taken immediately, so an exit carries no protection or expiry of
                // its own; those fields describe entries.
                stop_loss: 0.0,
                take_profit: 0.0,
                time_to_live: 0,
                timestamp: trade.timestamp.max(0) as u64,
            },
        })
    }

    /// Called once the shadow queue has actually filled the entry.
    pub fn on_entry_filled(&mut self, intent: &TradeIntent, fill_price: f64) {
        self.pending_entry.remove(&intent.symbol);

        self.open.insert(
            intent.symbol.clone(),
            OpenPosition {
                symbol: intent.symbol.clone(),
                side: intent.side,
                entry_price: fill_price,
                quantity: intent.size,
                stop_loss: intent.stop_loss,
                take_profit: intent.take_profit,
                entry_timestamp: intent.timestamp,
            },
        );
    }

    /// An entry that expired unfilled must not keep blocking its symbol.
    pub fn on_entry_dropped(&mut self, symbol: &str) {
        self.pending_entry.remove(symbol);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PriceLevel;

    fn book(bids: &[(f64, f64)], asks: &[(f64, f64)], timestamp: i64) -> OrderBookSnapshot {
        let levels = |rows: &[(f64, f64)]| {
            rows.iter()
                .map(|&(price, quantity)| PriceLevel { price, quantity })
                .collect::<Vec<_>>()
        };

        OrderBookSnapshot {
            symbol: "BTCUSDT".to_string(),
            timestamp,
            bids: levels(bids),
            asks: levels(asks),
        }
    }

    fn strategy() -> ObiSimStrategy {
        ObiSimStrategy::new(StrategyParams::default())
    }

    fn trade(price: f64, quantity: f64, timestamp: i64) -> TradeEvent {
        TradeEvent {
            symbol: "BTCUSDT".to_string(),
            timestamp,
            price,
            quantity,
            side: TradeSide::Buy,
        }
    }

    #[test]
    fn obi_matches_the_layer_two_formula() {
        // (3 - 1) / (3 + 1)
        assert_eq!(calculate_obi(&book(&[(40000.0, 3.0)], &[(40001.0, 1.0)], 0), 1), 0.5);
        // One-sided books: the formula only sees levels present on both sides.
        assert_eq!(calculate_obi(&book(&[(40000.0, 2.0)], &[(40001.0, 0.0)], 0), 1), 1.0);
        assert_eq!(calculate_obi(&book(&[(40000.0, 0.0)], &[(40001.0, 2.0)], 0), 1), -1.0);
        // Empty side => no levels compared, which is 0.0 rather than a division by zero.
        assert_eq!(calculate_obi(&book(&[(40000.0, 2.0)], &[], 0), 1), 0.0);
    }

    #[test]
    fn entries_follow_the_threshold_and_the_maker_side() {
        let mut strat = strategy();
        match strat.on_snapshot(&book(&[(40000.0, 4.0)], &[(40001.0, 1.0)], 10_000), 10_000.0, 10.0) {
            EntryDecision::Signal(intent) => {
                assert_eq!(intent.side, TradeSide::Buy);
                // A buy works at the bid, and its stop/target are Layer 2's geometry.
                assert_eq!(intent.price, 40000.0);
                assert_eq!(intent.stop_loss, 40000.0 * 0.995);
                assert_eq!(intent.take_profit, 40000.0 * 1.01);
                assert_eq!(intent.time_to_live, SIM_ORDER_TTL_MS);
            }
            other => panic!("expected an entry, got {:?}", other),
        }

        // The same book needs a second, separate signal before it can act again.
        assert_eq!(
            strat.on_snapshot(&book(&[(40000.0, 4.0)], &[(40001.0, 1.0)], 11_000), 10_000.0, 10.0),
            EntryDecision::Rejected(Rejection::OrderInFlight)
        );
    }

    #[test]
    fn weak_imbalance_is_not_a_signal() {
        let mut strat = strategy();
        assert_eq!(
            strat.on_snapshot(&book(&[(40000.0, 2.6)], &[(40001.0, 2.4)], 0), 10_000.0, 10.0),
            EntryDecision::Rejected(Rejection::BelowThreshold)
        );
    }

    #[test]
    fn sizing_risks_the_configured_share_and_caps_the_notional() {
        let strat = strategy();
        // 1% of 10_000 = 100 at risk over a 0.5% (200) stop distance = 0.5 BTC,
        // which is 20_000 notional - capped at max_position_value (10_000) = 0.25.
        let sized = strat.size_position(10_000.0, 10.0, 40_000.0, 39_800.0);
        assert!((sized - 0.25).abs() < 1e-12, "expected 0.25 BTC, got {}", sized);

        // A thin account still sizes, and that is correct: 1% of 100 over a 200-point
        // stop is 0.005 BTC, 200 USDT of notional, which clears the venue's 5 USDT floor
        // with room to spare. Note the floor can never bind at this price - one step of
        // 0.001 BTC is 40 USDT - so the rejection below is deliberately probed on a cheap
        // symbol, where a step is 0.1 USDT and the minimum becomes the live constraint.
        let thin = strat.size_position(100.0, 10.0, 40_000.0, 39_800.0);
        assert!(
            (thin - 0.005).abs() < 1e-12,
            "a small account sizes to 0.005 BTC, not to zero: got {}",
            thin
        );

        // Here the floor does bind: 1% of 1 over a 0.5-point stop is 0.02 = 2 USDT, and
        // an order the exchange would reject is refused rather than booked.
        assert_eq!(strat.size_position(1.0, 10.0, 100.0, 99.5), 0.0);
        // The same symbol at a workable size passes the floor untouched.
        let cheap = strat.size_position(100.0, 10.0, 100.0, 99.5);
        assert!(
            (cheap - 2.0).abs() < 1e-12,
            "2.0 units at 100 is 200 USDT, above the floor: got {}",
            cheap
        );
    }

    #[test]
    fn stops_and_targets_trigger_from_the_right_side() {
        let mut strat = strategy();
        let intent = match strat.on_snapshot(&book(&[(40000.0, 4.0)], &[(40001.0, 1.0)], 0), 10_000.0, 10.0) {
            EntryDecision::Signal(intent) => intent,
            other => panic!("expected an entry, got {:?}", other),
        };
        strat.on_entry_filled(&intent, 40_000.0);

        // Nothing to do in between.
        assert!(strat.check_exits(&trade(39_950.0, 0.1, 100)).is_none());
        // Target at 1% is 40400.
        let exit = strat.check_exits(&trade(40_401.0, 0.1, 200)).expect("target hit");
        assert_eq!(exit.reason, ExitReason::TakeProfit);
        assert_eq!(exit.intent.side, TradeSide::Sell, "closing a long is a sell");
        assert_eq!(exit.intent.price, 40_400.0, "trigger level, not the print");

        // A fresh long is stopped out below its stop.
        let intent = match strat.on_snapshot(&book(&[(40000.0, 4.0)], &[(40001.0, 1.0)], 10_000), 10_000.0, 10.0) {
            EntryDecision::Signal(intent) => intent,
            other => panic!("expected an entry, got {:?}", other),
        };
        strat.on_entry_filled(&intent, 40_000.0);
        let exit = strat.check_exits(&trade(39_799.0, 0.1, 10_500)).expect("stop hit");
        assert_eq!(exit.reason, ExitReason::StopLoss);
        assert!(strat.open_positions().next().is_none(), "the exit closed the position");
    }

    #[test]
    fn cooldown_blocks_the_next_snapshot_only_briefly() {
        let mut strat = strategy();
        let strong = book(&[(40000.0, 4.0)], &[(40001.0, 1.0)], 5_000);
        strat.on_snapshot(&strong, 10_000.0, 10.0);
        strat.on_entry_dropped("BTCUSDT"); // order expired without filling

        assert_eq!(
            strat.on_snapshot(&book(&[(40000.0, 4.0)], &[(40001.0, 1.0)], 5_500), 10_000.0, 10.0),
            EntryDecision::Rejected(Rejection::Cooldown)
        );
        assert!(matches!(
            strat.on_snapshot(&book(&[(40000.0, 4.0)], &[(40001.0, 1.0)], 6_100), 10_000.0, 10.0),
            EntryDecision::Signal(_)
        ));
    }

    #[test]
    fn volatility_gate_suppresses_entries_when_configured() {
        let mut params = StrategyParams::default();
        params.volatility_window = 2;
        params.max_volatility = 0.001; // 10 bps RMS
        let mut strat = ObiSimStrategy::new(params);

        for (i, price) in [40_000.0, 40_800.0, 40_000.0].iter().enumerate() {
            strat.on_trade_print(&trade(*price, 1.0, i as i64));
        }

        let mut at = 100_000;
        // Stepping past the cooldown is only the setup; what the test judges is the
        // decision the loop actually stopped on, so the outcome is bound rather than
        // thrown away and re-asked of a later snapshot.
        let decision = loop {
            match strat.on_snapshot(&book(&[(40000.0, 4.0)], &[(40001.0, 1.0)], at), 10_000.0, 10.0) {
                EntryDecision::Rejected(Rejection::Cooldown) => at += SIM_SIGNAL_COOLDOWN_MS as i64 + 1,
                other => break other,
            }
        };
        assert_eq!(decision, EntryDecision::Rejected(Rejection::VolatilityGate));
    }
}
