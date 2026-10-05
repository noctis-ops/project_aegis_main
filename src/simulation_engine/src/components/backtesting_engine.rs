//! Event-Driven Backtesting Engine implementation
//!
//! This is the deterministic spine of Layer 5: replayed market events drive the same
//! decision rule the live path uses ([`crate::components::strategy`]), orders are
//! worked by the pessimistic queue
//! ([`PessimisticMatchingEngine`](crate::components::shadow_trading::PessimisticMatchingEngine)),
//! and PnL is netted lot by lot
//! ([`VirtualPnlTracker`](crate::components::shadow_trading::VirtualPnlTracker)).
//!
//! What it replaces matters as much as what it does. `run_simulation` used to sleep
//! 100 ms and return a fixed report - 15% return, Sharpe 2.5, 1250 trades - whatever
//! it was given, and the capital-adaptation and CI gates above it "passed" against
//! those constants. A simulator that cannot fail cannot inform a decision, so every
//! number below is computed from the replayed data or the run refuses to report.

use crate::components::shadow_trading::{PessimisticMatchingEngine, VirtualPnlTracker};
use crate::components::strategy::{
    EntryDecision, ExitReason, ObiSimStrategy, Rejection, TriggeredExit,
};
use crate::core::{
    BacktestReport, FundingRateEvent, MarketEvent, OrderBookSnapshot, SimulationConfig,
    SimulationError, SimulationResult, TradeAction, TradeEvent, TradeIntent, TradeLogEntry,
};
use crate::core::constants::SECONDS_PER_YEAR;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, error, info, warn};

/// Entry orders that are working the queue, awaiting a fill or their expiry.
struct PendingEntry {
    intent: TradeIntent,
}

/// One replay: books, strategy and accounting for a single run.
struct RunState {
    config: SimulationConfig,
    strategy: ObiSimStrategy,
    matching: PessimisticMatchingEngine,
    pnl: VirtualPnlTracker,
    pending: HashMap<String, PendingEntry>,
    window_start_ms: i64,
    end_time_ms: i64,
    current_time: i64,
    equity: f64,
    last_prices: HashMap<String, f64>,
    equity_curve: Vec<(i64, f64)>,
    trade_log: Vec<TradeLogEntry>,
    funding_paid: f64,
    events: usize,
    snapshots: usize,
    skipped_before_window: usize,
    submitted: usize,
    filled: usize,
    expired: usize,
    rejections: HashMap<Rejection, usize>,
    slippage_cost: f64,
    traded_notional: f64,
}

impl RunState {
    fn new(config: SimulationConfig) -> Result<Self, SimulationError> {
        let mut matching = PessimisticMatchingEngine::new();
        matching.initialize()?;

        let mut pnl = VirtualPnlTracker::new();
        // The fee schedule comes from the run config. A backtest that ignores fees
        // overstates every strategy that trades often - which is the entire class of
        // strategy this project runs.
        pnl.initialize(config.maker_fee_rate, config.taker_fee_rate)?;

        let window_start_ms = config.start_time.timestamp_millis();
        let end_time_ms = config.end_time.timestamp_millis();
        let equity = config.initial_capital;

        Ok(Self {
            strategy: ObiSimStrategy::new(config.strategy.clone()),
            config,
            matching,
            pnl,
            pending: HashMap::new(),
            window_start_ms,
            end_time_ms,
            current_time: window_start_ms,
            equity,
            last_prices: HashMap::new(),
            // A run starts on the curve, so drawdown and Sharpe see the capital the
            // strategy began with rather than only its first trade.
            equity_curve: vec![(window_start_ms, equity)],
            trade_log: Vec::new(),
            funding_paid: 0.0,
            events: 0,
            snapshots: 0,
            skipped_before_window: 0,
            submitted: 0,
            filled: 0,
            expired: 0,
            rejections: HashMap::new(),
            slippage_cost: 0.0,
            traded_notional: 0.0,
        })
    }

    /// Consume one replayed event.
    fn on_event(&mut self, event: MarketEvent) -> Result<(), SimulationError> {
        let event_time = match &event {
            MarketEvent::OrderBookSnapshot(snapshot) => snapshot.timestamp,
            MarketEvent::Trade(trade) => trade.timestamp,
            MarketEvent::FundingRate(funding) => funding.timestamp,
        };

        if event_time > self.end_time_ms {
            // Acting on data past the requested window is look-ahead bias - the one
            // thing a backtest must not smuggle in, because it flatters every metric.
            return Err(SimulationError::LookAheadBias(format!(
                "Event at {} is beyond the simulation window ending at {}",
                event_time, self.end_time_ms
            )));
        }

        if event_time < self.window_start_ms {
            // History before the window is not this run's business; counting it keeps
            // "no data" distinguishable from "data starts earlier than requested".
            self.skipped_before_window += 1;
            return Ok(());
        }

        if event_time < self.current_time {
            return Err(SimulationError::TimestampViolation(format!(
                "Event at {} precedes the simulated clock {}; the feed must be ordered",
                event_time, self.current_time
            )));
        }

        self.current_time = event_time;
        self.events += 1;

        match event {
            MarketEvent::OrderBookSnapshot(snapshot) => self.on_book(snapshot),
            MarketEvent::Trade(trade) => self.on_trade(trade),
            MarketEvent::FundingRate(funding) => self.on_funding(funding),
        }
    }

    fn on_book(&mut self, snapshot: OrderBookSnapshot) -> Result<(), SimulationError> {
        self.snapshots += 1;

        // The matching engine must hold this book before an order can be worked
        // against it, and the snapshot stays owned here for the strategy call below.
        self.matching.update_order_book(snapshot.clone())?;
        self.expire_pending()?;

        match self.strategy.on_snapshot(&snapshot, self.equity, self.config.leverage) {
            EntryDecision::Signal(intent) => {
                self.submitted += 1;
                let order_id = self.matching.submit_virtual_order(intent.clone())?;
                debug!(
                    "Shadow order {} queued for {} ({} {} @ {:.2})",
                    order_id, intent.symbol, intent.size, side_label(&intent), intent.price
                );
                self.pending.insert(order_id, PendingEntry { intent });
            }
            EntryDecision::Rejected(rejection) => {
                *self.rejections.entry(rejection).or_insert(0) += 1;
                debug!(
                    "No entry on {} at {}: {:?}",
                    snapshot.symbol, snapshot.timestamp, rejection
                );
            }
        }

        self.record_equity();
        Ok(())
    }

    fn on_trade(&mut self, trade: TradeEvent) -> Result<(), SimulationError> {
        self.last_prices
            .insert(trade.symbol.clone(), trade.price);
        self.strategy.on_trade_print(&trade);

        // Queue work first: which of our entries did this print fill?
        for order_id in self.matching.process_tick_data(&trade)? {
            let intent = match self.pending.remove(&order_id) {
                Some(entry) => entry.intent,
                // `process_tick_data` only reports orders it holds, so this would mean
                // the two books disagree - worth an error log, not a silent skip.
                None => {
                    error!("Filled shadow order {} is not in the pending book", order_id);
                    continue;
                }
            };

            // A limit order fills at its quoted price; the pessimistic engine has
            // already decided whether the queue ever reached that level.
            let fill_price = intent.price;
            let signal_pnl = self.pnl.record_trade_execution(&intent, fill_price, true)?;
            self.filled += 1;
            self.traded_notional += fill_price * intent.size;
            self.strategy.on_entry_filled(&intent, fill_price);

            let (fees, slippage) = self.last_fill_costs();
            self.slippage_cost += slippage * intent.size;
            self.trade_log.push(TradeLogEntry {
                timestamp: trade.timestamp,
                symbol: intent.symbol.clone(),
                action: TradeAction::Entry,
                price: fill_price,
                quantity: intent.size,
                slippage,
                fees,
                pnl: signal_pnl,
            });
        }

        // Then protection. Stops and targets cross the spread, so they fill on the
        // print itself instead of queueing behind it.
        if let Some(exit) = self.strategy.check_exits(&trade) {
            self.record_exit(exit, &trade)?;
        }

        self.record_equity();
        Ok(())
    }

    fn record_exit(&mut self, exit: TriggeredExit, trade: &TradeEvent) -> Result<(), SimulationError> {
        let TriggeredExit { intent, reason } = exit;
        let execution_price = trade.price;
        let signal_pnl = self.pnl.record_trade_execution(&intent, execution_price, false)?;
        self.traded_notional += execution_price * intent.size;

        let (fees, slippage) = self.last_fill_costs();
        self.slippage_cost += slippage * intent.size;
        self.trade_log.push(TradeLogEntry {
            timestamp: trade.timestamp,
            symbol: intent.symbol.clone(),
            action: match reason {
                ExitReason::StopLoss => TradeAction::StopLoss,
                ExitReason::TakeProfit => TradeAction::TakeProfit,
            },
            price: execution_price,
            quantity: intent.size,
            slippage,
            fees,
            pnl: signal_pnl,
        });

        Ok(())
    }

    /// Fees and slippage of the fill just recorded, read back from the ledger rather
    /// than recomputed here - two formulas for one fee is how reports and ledgers
    /// start disagreeing.
    fn last_fill_costs(&self) -> (f64, f64) {
        match self.pnl.get_trade_history().last() {
            Some(record) => (record.fees, record.slippage),
            None => (0.0, 0.0),
        }
    }

    fn on_funding(&mut self, funding: FundingRateEvent) -> Result<(), SimulationError> {
        let mark = match self.last_prices.get(&funding.symbol) {
            Some(price) => *price,
            // Funding without a mark price cannot be charged; that is a data gap in
            // the feed, so it is reported rather than assumed to be zero cost.
            None => {
                warn!("Funding event for {} has no mark price yet; charge skipped", funding.symbol);
                return Ok(());
            }
        };

        let quantity = self.pnl.get_exposure_by_symbol(&funding.symbol);
        if quantity == 0.0 {
            return Ok(());
        }

        // Signed: a positive rate charges longs and pays shorts.
        let cost = quantity * mark * funding.rate;
        self.funding_paid += cost;
        info!(
            "Funding for {} at {}: {:+.4} USDT on {:+.6} exposure",
            funding.symbol, funding.timestamp, cost, quantity
        );

        self.record_equity();
        Ok(())
    }

    /// Cancel entries that outlived their time-to-live, as the live path would:
    /// Layer 2 stamps a TTL on every signal, so an order that never filled has to
    /// free its symbol *and* be counted as a missed opportunity.
    fn expire_pending(&mut self) -> Result<(), SimulationError> {
        let stale: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, entry)| {
                let age_ms = self.current_time.saturating_sub(entry.intent.timestamp as i64);
                age_ms > entry.intent.time_to_live.max(1) as i64
            })
            .map(|(order_id, _)| order_id.clone())
            .collect();

        for order_id in stale {
            let entry = match self.pending.remove(&order_id) {
                Some(entry) => entry,
                None => continue,
            };

            if let Err(e) = self.matching.cancel_virtual_order(&order_id) {
                // The order is no longer ours either way; the books disagreeing is a
                // bug worth seeing, not one worth aborting a run over.
                error!("Expired shadow order {} was not in the matching book: {}", order_id, e);
            }

            self.strategy.on_entry_dropped(&entry.intent.symbol);
            self.expired += 1;
            debug!("Shadow order {} for {} expired unfilled", order_id, entry.intent.symbol);
        }

        Ok(())
    }

    /// Mark the account and append to the curve, skipping points where nothing moved
    /// so a tick-level replay does not fill the report with duplicates.
    fn record_equity(&mut self) {
        let equity = self.mark_equity();
        let moved = match self.equity_curve.last() {
            Some((_, last)) => (last - equity).abs() >= 1e-9,
            None => true,
        };

        self.equity = equity;
        if moved {
            self.equity_curve.push((self.current_time, equity));
        }
    }

    /// Equity = starting capital + net realized PnL (fees included by the tracker)
    /// + unrealized on open lots - funding paid.
    fn mark_equity(&self) -> f64 {
        let unrealized = self.pnl.calculate_unrealized_pnl(&self.last_prices);
        self.config.initial_capital + self.pnl.get_realized_pnl() + unrealized - self.funding_paid
    }

    fn finalize(mut self) -> BacktestReport {
        self.record_equity();

        let total_return = if self.config.initial_capital > 0.0 {
            self.equity / self.config.initial_capital - 1.0
        } else {
            0.0
        };

        let max_drawdown = max_drawdown(&self.equity_curve);
        let sharpe_ratio = sharpe_ratio(&self.equity_curve);
        let win_rate = win_rate(self.pnl.get_closed_leg_pnls());
        let profit_factor = profit_factor(self.pnl.get_closed_leg_pnls());
        let avg_trade_duration = mean(self.pnl.get_closed_leg_durations_ms()) / 1000.0;
        let slippage_impact = if self.traded_notional > 0.0 {
            self.slippage_cost / self.traded_notional
        } else {
            0.0
        };
        // Execution quality is the share of queued orders that the pessimistic book
        // actually filled. Zero submissions is 0.0, not 1.0: "nothing was tried" must
        // never read as "everything worked".
        let execution_quality = if self.submitted > 0 {
            self.filled as f64 / self.submitted as f64
        } else {
            0.0
        };

        let results = SimulationResult {
            total_return,
            sharpe_ratio,
            max_drawdown,
            win_rate,
            profit_factor,
            total_trades: self.pnl.get_trade_history().len(),
            avg_trade_duration,
            slippage_impact,
            execution_quality,
        };

        info!(
            "Backtest replay: {} events ({} snapshots), {} orders submitted, {} filled, {} expired, {} skipped before window",
            self.events, self.snapshots, self.submitted, self.filled, self.expired, self.skipped_before_window
        );
        if !self.rejections.is_empty() {
            info!("Entry rejections by reason: {:?}", self.rejections);
        }
        if self.submitted == 0 {
            warn!(
                "No order was ever submitted across {} snapshot(s): the strategy found nothing to trade, \
                 so these metrics measure the absence of signals, not the absence of risk",
                self.snapshots
            );
        }
        if self.filled < self.submitted {
            warn!(
                "{} of {} shadow orders never filled; execution quality is capped by the queue, not by the signal",
                self.submitted - self.filled,
                self.submitted
            );
        }

        BacktestReport {
            config: self.config,
            results,
            equity_curve: self.equity_curve,
            trade_log: self.trade_log,
            timestamp: Utc::now(),
        }
    }
}

/// Event-Driven Backtesting Engine
#[derive(Clone)]
pub struct BacktestingEngine {
    run: Arc<RwLock<Option<RunState>>>,
}

impl BacktestingEngine {
    /// Create a new Backtesting Engine
    pub fn new() -> Self {
        Self { run: Arc::new(RwLock::new(None)) }
    }

    /// Run a full replay of `events` under `config` and report what was measured.
    ///
    /// The events come from the caller (the data lake) so that a run is defined by a
    /// fixed, re-playable sequence - the same input must always produce the same
    /// report, which is the property the deterministic path exists to preserve.
    ///
    /// The sequence is *borrowed*: a robustness sweep replays one window through many
    /// configurations, and cloning millions of tick events per variant would make the
    /// sweep cost memory instead of CPU.
    pub async fn run_simulation(
        &self,
        config: &SimulationConfig,
        events: &[MarketEvent],
    ) -> Result<BacktestReport, SimulationError> {
        Self::validate(config)?;

        {
            let mut guard = self.run.write().unwrap();
            if guard.is_some() {
                // Two runs sharing one ledger would blend two equity curves into a
                // report that describes neither.
                return Err(SimulationError::ExecutionError(
                    "A simulation is already running on this engine".to_string(),
                ));
            }
            *guard = Some(RunState::new(config.clone())?);
        }

        info!(
            "Starting backtest replay: {} event(s), {} symbol(s), capital ${:.2} at {:.1}x",
            events.len(),
            config.symbols.len(),
            config.initial_capital,
            config.leverage
        );

        let replay = self.replay(events).await;

        // Torn down whichever way it ended: a failed replay must not leave state that
        // the next caller silently continues from.
        let run = self.run.write().unwrap().take();
        replay?;

        let run = match run {
            Some(run) => run,
            None => {
                return Err(SimulationError::ExecutionError(
                    "Simulation state disappeared during replay".to_string(),
                ))
            }
        };

        if run.events == 0 {
            return Err(SimulationError::DataLoadingError(format!(
                "The replay window [{}, {}] contained no market events ({} skipped as earlier data); \
                 a report here would be invention, so the run is failed instead",
                config.start_time.timestamp_millis(),
                config.end_time.timestamp_millis(),
                run.skipped_before_window
            )));
        }

        Ok(run.finalize())
    }

    async fn replay(&self, events: &[MarketEvent]) -> Result<(), SimulationError> {
        for event in events {
            // The engine's public streaming entry point is reused here, so a replayed
            // file and a live feed are judged by exactly the same code.
            self.process_market_event(event.clone()).await?;
        }
        Ok(())
    }

    /// Reject configurations that cannot produce a meaningful measurement.
    fn validate(config: &SimulationConfig) -> Result<(), SimulationError> {
        let invalid = |message: &str| SimulationError::InvalidConfig(message.to_string());

        if config.start_time >= config.end_time {
            return Err(invalid("Start time must be before end time - an empty window measures nothing"));
        }
        if config.initial_capital <= 0.0 {
            return Err(invalid("Initial capital must be positive"));
        }
        if config.symbols.is_empty() {
            return Err(invalid("At least one symbol must be specified"));
        }
        if config.leverage < 1.0 {
            return Err(invalid("Leverage below 1x cannot be placed on a futures venue"));
        }
        if config.maker_fee_rate < 0.0 || config.taker_fee_rate < 0.0 {
            return Err(invalid("Fee rates must not be negative"));
        }

        let strategy = &config.strategy;
        if strategy.obi_threshold <= 0.0 || strategy.obi_threshold >= 1.0 {
            return Err(invalid("obi_threshold must lie inside (0, 1) - OBI is bounded by [-1, 1]"));
        }
        if strategy.obi_levels == 0 {
            return Err(invalid("obi_levels must be at least 1"));
        }
        if strategy.risk_percentage <= 0.0 || strategy.risk_percentage > 1.0 {
            return Err(invalid("risk_percentage must lie in (0, 1]"));
        }
        if strategy.size_step <= 0.0 {
            return Err(invalid("size_step must be positive"));
        }
        if strategy.max_position_value <= 0.0 {
            return Err(invalid("max_position_value must be positive"));
        }
        if strategy.volatility_window == 0 {
            return Err(invalid("volatility_window must be at least 1"));
        }

        Ok(())
    }

    /// Process a market event
    pub async fn process_market_event(&self, event: MarketEvent) -> Result<(), SimulationError> {
        let mut guard = self.run.write().unwrap();

        match guard.as_mut() {
            Some(run) => run.on_event(event),
            None => {
                // A feed pushing outside a run has nothing to judge the event against.
                // It used to look exactly like data being consumed.
                debug!("Dropping market event: no simulation is running on this engine");
                Ok(())
            }
        }
    }

    /// Get current simulation time
    pub fn get_current_time(&self) -> i64 {
        let guard = self.run.read().unwrap();
        match guard.as_ref() {
            Some(run) => run.current_time,
            None => 0,
        }
    }

    /// Check if a simulation is running
    pub fn is_running(&self) -> bool {
        self.run.read().unwrap().is_some()
    }

    /// Append an externally marked equity point (used by adapters that value their own
    /// books, e.g. a live shadow run driven outside `run_simulation`).
    pub fn update_equity_curve(&self, timestamp: i64, equity: f64) {
        let mut guard = self.run.write().unwrap();
        if let Some(run) = guard.as_mut() {
            run.equity_curve.push((timestamp, equity));
            run.equity = equity;
        }
    }

    /// Log a trade execution (companion of [`Self::update_equity_curve`]).
    pub fn log_trade(&self, entry: TradeLogEntry) {
        let mut guard = self.run.write().unwrap();
        if let Some(run) = guard.as_mut() {
            run.trade_log.push(entry);
        }
    }
}

fn side_label(intent: &TradeIntent) -> &'static str {
    match intent.side {
        crate::core::TradeSide::Buy => "buy",
        crate::core::TradeSide::Sell => "sell",
    }
}

/// Peak-to-trough decline of the equity curve, as a fraction of the peak.
pub fn max_drawdown(curve: &[(i64, f64)]) -> f64 {
    let mut peak = f64::NEG_INFINITY;
    let mut worst = 0.0;

    for &(_, equity) in curve {
        if equity > peak {
            peak = equity;
            continue;
        }
        if peak > 0.0 {
            let drawdown = (peak - equity) / peak;
            if drawdown > worst {
                worst = drawdown;
            }
        }
    }

    worst
}

/// Sharpe ratio of the curve's step returns, annualized by the mean spacing of the
/// curve. Returns 0.0 when there is nothing to disperse (one point, or a flat curve),
/// because a flat equity line is "no risk taken", not "infinite risk-adjusted return".
pub fn sharpe_ratio(curve: &[(i64, f64)]) -> f64 {
    if curve.len() < 2 {
        return 0.0;
    }

    let mut returns = Vec::with_capacity(curve.len() - 1);
    let mut steps = Vec::with_capacity(curve.len() - 1);

    for window in curve.windows(2) {
        let (before, after) = (&window[0], &window[1]);
        let seconds = (after.0 - before.0) as f64 / 1000.0;
        if seconds <= 0.0 || before.1 <= 0.0 {
            continue;
        }
        returns.push(after.1 / before.1 - 1.0);
        steps.push(seconds);
    }

    if returns.is_empty() {
        return 0.0;
    }

    // Named apart from the helper: a local called `mean` would shadow `fn mean` for
    // every later statement in this block.
    let mean_return = mean(&returns);
    let variance =
        returns.iter().map(|r| (r - mean_return) * (r - mean_return)).sum::<f64>() / returns.len() as f64;
    let deviation = variance.sqrt();

    if deviation <= 0.0 {
        return 0.0;
    }

    let mean_step = mean(&steps);
    if mean_step <= 0.0 {
        return 0.0;
    }

    (mean_return / deviation) * (SECONDS_PER_YEAR / mean_step).sqrt()
}

/// Share of closed rounds that made money.
pub fn win_rate(legs: &[f64]) -> f64 {
    if legs.is_empty() {
        return 0.0;
    }
    legs.iter().filter(|&&leg| leg > 0.0).count() as f64 / legs.len() as f64
}

/// Gross wins over gross losses, capped so the report stays serializable.
pub fn profit_factor(legs: &[f64]) -> f64 {
    let wins: f64 = legs.iter().filter(|&&leg| leg > 0.0).map(|leg| *leg).sum();
    let losses: f64 = legs.iter().filter(|&&leg| leg < 0.0).map(|leg| -leg).sum();

    if losses <= 0.0 {
        return if wins > 0.0 { crate::core::constants::MAX_PROFIT_FACTOR } else { 0.0 };
    }

    (wins / losses).min(crate::core::constants::MAX_PROFIT_FACTOR)
}

pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{PriceLevel, TradeSide};
    use chrono::TimeZone;

    const T0: i64 = 1_640_995_200_000; // 2022-01-01 00:00:00 UTC

    fn utc(millis: i64) -> chrono::DateTime<Utc> {
        chrono::Utc
            .timestamp_millis_opt(millis)
            .single()
            .expect("millisecond epoch inside chrono's range")
    }

    fn config(window_start: i64, window_end: i64, capital: f64) -> SimulationConfig {
        SimulationConfig {
            data_path: "./test_data".to_string(),
            start_time: utc(window_start),
            end_time: utc(window_end),
            playback_speed: 1.0,
            initial_capital: capital,
            leverage: 10.0,
            symbols: vec!["BTCUSDT".to_string()],
            strategy: crate::core::StrategyParams::default(),
            maker_fee_rate: 0.0002,
            taker_fee_rate: 0.0004,
        }
    }

    fn snapshot(bids: &[(f64, f64)], asks: &[(f64, f64)], timestamp: i64) -> MarketEvent {
        let levels = |rows: &[(f64, f64)]| {
            rows.iter()
                .map(|&(price, quantity)| PriceLevel { price, quantity })
                .collect::<Vec<_>>()
        };

        MarketEvent::OrderBookSnapshot(OrderBookSnapshot {
            symbol: "BTCUSDT".to_string(),
            timestamp,
            bids: levels(bids),
            asks: levels(asks),
        })
    }

    fn trade(price: f64, quantity: f64, timestamp: i64) -> MarketEvent {
        MarketEvent::Trade(TradeEvent {
            symbol: "BTCUSDT".to_string(),
            timestamp,
            price,
            quantity,
            side: TradeSide::Buy,
        })
    }

    /// An intent shaped like the strategy's own: 0.5% stop, 1% target, one TTL.
    fn intent(side: TradeSide, size: f64) -> TradeIntent {
        let (stop_loss, take_profit) = match side {
            TradeSide::Buy => (39_800.0, 40_400.0),
            TradeSide::Sell => (40_200.0, 39_600.0),
        };
        TradeIntent {
            symbol: "BTCUSDT".to_string(),
            side,
            price: 40_000.0,
            size,
            stop_loss,
            take_profit,
            time_to_live: crate::core::constants::SIM_ORDER_TTL_MS,
            timestamp: T0 as u64,
        }
    }

    #[tokio::test]
    async fn a_full_round_trip_is_measured_not_assumed() {
        let engine = BacktestingEngine::new();
        let events = vec![
            // Bid-heavy book: 8.0 queued on the bids against 0.5 on the ask, so
            // OBI = (8 - 0.5) / (8 + 0.5) = 0.88 and the 0.4 threshold is cleared.
            snapshot(
                &[(40_000.0, 4.0), (39_999.0, 4.0)],
                &[(40_001.0, 0.5)],
                T0 + 1_000,
            ),
            // The print that reaches our queued bid, so the entry fills at 40000.
            trade(40_000.0, 0.5, T0 + 1_050),
            // Pushes through the 1% target (40400) for the exit.
            trade(40_401.0, 0.5, T0 + 2_000),
        ];

        let report = engine
            .run_simulation(&config(T0, T0 + 60_000, 10_000.0), &events)
            .await
            .expect("a replay with events must produce a report");

        assert_eq!(report.trade_log.len(), 2, "one entry and one exit");
        assert_eq!(report.results.total_trades, 2);
        assert!(
            matches!(report.trade_log[0].action, TradeAction::Entry),
            "first record is the entry"
        );
        assert!(
            matches!(report.trade_log[1].action, TradeAction::TakeProfit),
            "second record is the target exit"
        );

        // Sized at the notional cap: 0.25 BTC entered at 40000, exited at 40401.
        assert_eq!(report.trade_log[0].quantity, 0.25);
        let gross = (40_401.0 - 40_000.0) * 0.25;
        let fees = 40_000.0 * 0.25 * 0.0002 + 40_401.0 * 0.25 * 0.0004;
        let expected = gross - fees;
        assert!(
            (report.equity_curve.last().unwrap().1 - (10_000.0 + expected)).abs() < 1e-6,
            "equity must be capital + realized net of fees, got {:?}",
            report.equity_curve.last()
        );
        assert!(report.results.total_return > 0.005);
        assert_eq!(report.results.win_rate, 1.0);
        assert_eq!(report.results.execution_quality, 1.0);
        assert!(report.results.profit_factor > 1.0);
    }

    #[tokio::test]
    async fn a_run_without_events_in_the_window_is_an_error_not_a_result() {
        let engine = BacktestingEngine::new();
        // Everything in the lake predates the requested window.
        let events = vec![snapshot(&[(40_000.0, 4.0)], &[(40_001.0, 1.0)], T0 - 10_000)];

        let error = engine
            .run_simulation(&config(T0, T0 + 60_000, 10_000.0), &events)
            .await
            .expect_err("an empty window cannot support a report");
        assert!(matches!(error, SimulationError::DataLoadingError(_)), "{:?}", error);
    }

    #[tokio::test]
    async fn events_beyond_the_window_are_rejected_as_look_ahead() {
        let engine = BacktestingEngine::new();
        let events = vec![
            snapshot(&[(40_000.0, 4.0)], &[(40_001.0, 1.0)], T0 + 1_000),
            snapshot(&[(40_000.0, 4.0)], &[(40_001.0, 1.0)], T0 + 120_000),
        ];

        let error = engine
            .run_simulation(&config(T0, T0 + 60_000, 10_000.0), &events)
            .await
            .expect_err("data past the end time is future data");
        assert!(matches!(error, SimulationError::LookAheadBias(_)), "{:?}", error);
    }

    #[tokio::test]
    async fn out_of_order_events_are_rejected() {
        let engine = BacktestingEngine::new();
        let events = vec![
            snapshot(&[(40_000.0, 4.0), (39_999.0, 4.0)], &[(40_001.0, 1.0)], T0 + 5_000),
            trade(40_000.0, 1.0, T0 + 4_000),
        ];

        let error = engine
            .run_simulation(&config(T0, T0 + 60_000, 10_000.0), &events)
            .await
            .expect_err("a replayed feed has to be ordered");
        assert!(matches!(error, SimulationError::TimestampViolation(_)), "{:?}", error);
    }

    #[tokio::test]
    async fn two_runs_on_one_ledger_are_refused() {
        let engine = BacktestingEngine::new();
        // Force a run to be open, then try to start another.
        {
            let mut guard = engine.run.write().unwrap();
            *guard = Some(RunState::new(config(T0, T0 + 60_000, 10_000.0)).unwrap());
        }

        let error = engine
            .run_simulation(&config(T0, T0 + 60_000, 10_000.0), &[])
            .await
            .expect_err("one ledger, one run");
        assert!(matches!(error, SimulationError::ExecutionError(_)), "{:?}", error);
    }

    #[test]
    fn invalid_configurations_are_refused_before_replay() {
        // `validate` is an associated function - it judges the configuration alone, which
        // is the point: no state a run happens to be in can excuse an empty window, a
        // threshold outside (0, 1), or a negative fee.
        let empty_window = config(T0, T0, 10_000.0);
        assert!(
            BacktestingEngine::validate(&empty_window).is_err(),
            "an empty window measures nothing and must not be replayed"
        );

        let mut bad_threshold = config(T0, T0 + 60_000, 10_000.0);
        bad_threshold.strategy.obi_threshold = 1.5;
        assert!(
            BacktestingEngine::validate(&bad_threshold).is_err(),
            "OBI is bounded by [-1, 1], so a 1.5 threshold asks for a signal that cannot exist"
        );

        let mut negative_fees = config(T0, T0 + 60_000, 10_000.0);
        negative_fees.maker_fee_rate = -0.001;
        assert!(
            BacktestingEngine::validate(&negative_fees).is_err(),
            "a negative fee would pay the strategy for trading"
        );
    }

    #[tokio::test]
    async fn streaming_events_outside_a_run_are_ignored() {
        // `SimulationEngine::ingest_lake_events` can be called before any run exists,
        // and an event with no run to judge it against is dropped rather than failed.
        let engine = BacktestingEngine::new();
        engine
            .process_market_event(trade(40_000.0, 1.0, T0))
            .await
            .expect("events outside a run are ignored, not failed");
        assert!(!engine.is_running());
        assert_eq!(engine.get_current_time(), 0);
    }

    #[test]
    fn drawdown_and_dispersion_helpers_agree_with_hand_computed_values() {
        let curve = vec![
            (0, 100.0),
            (1_000, 120.0),
            (2_000, 90.0),
            (3_000, 110.0),
        ];
        assert_eq!(max_drawdown(&curve), 0.25);
        // Win rate / profit factor over closed rounds.
        assert_eq!(win_rate(&[10.0, -5.0, 5.0]), 2.0 / 3.0);
        assert_eq!(profit_factor(&[10.0, -5.0, 5.0]), 3.0);
        assert_eq!(profit_factor(&[]), 0.0);
        assert_eq!(
            profit_factor(&[1.0, 2.0]),
            crate::core::constants::MAX_PROFIT_FACTOR
        );
        assert_eq!(mean(&[]), 0.0);
        // A flat curve has no dispersion to divide by, and must not report infinity.
        assert_eq!(sharpe_ratio(&[(0, 100.0), (1_000, 100.0)]), 0.0);
        assert_eq!(sharpe_ratio(&[(0, 100.0)]), 0.0);
        assert!(sharpe_ratio(&curve) > 0.0);
    }

    #[test]
    fn funding_is_charged_against_open_exposure() {
        let funding = |symbol: &str, timestamp: i64, rate: f64| FundingRateEvent {
            symbol: symbol.to_string(),
            timestamp,
            rate,
        };

        let mut run = RunState::new(config(T0, T0 + 60_000, 10_000.0)).unwrap();
        run.current_time = T0 + 1_000;
        run.last_prices.insert("BTCUSDT".to_string(), 40_000.0);

        // Flat: funding is free.
        run.on_funding(funding("BTCUSDT", T0 + 1_000, 0.001)).unwrap();
        assert_eq!(run.funding_paid, 0.0);

        // A rate for a symbol the run has no print for is a gap in the feed. It has to
        // stay non-fatal and must not book a charge against an unknown mark.
        run.on_funding(funding("ETHUSDT", T0 + 1_500, 0.001))
            .expect("a missing mark price is reported, not fatal");
        assert_eq!(run.funding_paid, 0.0);

        // rate * mark * signed exposure: a 1 BTC long pays 40 USDT at 0.1%.
        run.pnl
            .record_trade_execution(&intent(TradeSide::Buy, 1.0), 40_000.0, true)
            .expect("a maker fill is recorded");
        run.on_funding(funding("BTCUSDT", T0 + 2_000, 0.001)).unwrap();
        assert!(
            (run.funding_paid - 40.0).abs() < 1e-9,
            "a long should have paid 40 USDT, paid {}",
            run.funding_paid
        );

        // The mirror case decides the sign, not the size: the same rate is received by a
        // short. Kept in a second ledger so the first one's exposure stays 1.0.
        let mut short_run = RunState::new(config(T0, T0 + 60_000, 10_000.0)).unwrap();
        short_run.current_time = T0 + 1_000;
        short_run.last_prices.insert("BTCUSDT".to_string(), 40_000.0);
        short_run
            .pnl
            .record_trade_execution(&intent(TradeSide::Sell, 1.0), 40_000.0, true)
            .expect("a maker fill is recorded");
        short_run
            .on_funding(funding("BTCUSDT", T0 + 2_000, 0.001))
            .unwrap();
        assert!(
            (short_run.funding_paid + 40.0).abs() < 1e-9,
            "a short should have received 40 USDT, paid {}",
            short_run.funding_paid
        );
    }

}
