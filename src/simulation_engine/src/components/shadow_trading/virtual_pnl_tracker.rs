//! Virtual PnL Tracker for Shadow Trading

use crate::core::{SimulationError, TradeIntent, TradeSide};
use tracing::info;

/// Quantities at or below this are treated as flat: float dust left over by a
/// partial close must not survive as a "position" that every later aggregate
/// (exposure, unrealized PnL) keeps counting.
const MIN_LOT_QUANTITY: f64 = 1e-12;

/// Virtual PnL Tracker
pub struct VirtualPnlTracker {
    positions: Vec<Position>,
    trades: Vec<ExecutedTrade>,
    /// Gross PnL booked by fills that closed open lots; see [`Self::get_realized_pnl`].
    realized_gross_pnl: f64,
    maker_fee_rate: f64,
    taker_fee_rate: f64,
    is_initialized: bool,
}

/// One open lot.
///
/// Fills are kept as separate lots (rather than merged into an average entry) so
/// that a partial close can be netted against the exact entry prices it closes.
#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: TradeSide,
    pub entry_price: f64,
    pub quantity: f64,
    /// Fill time in milliseconds since the epoch, kept in the unit the signal
    /// carries (see [`TradeIntent::timestamp`]).
    pub entry_timestamp: u64,
}

/// Executed trade record
#[derive(Debug, Clone)]
pub struct ExecutedTrade {
    pub symbol: String,
    pub side: TradeSide,
    pub price: f64,
    pub quantity: f64,
    pub fees: f64,
    pub slippage: f64,
    /// Execution time in milliseconds since the epoch (see [`Position::entry_timestamp`]).
    pub timestamp: u64,
    pub is_maker: bool,
}

impl VirtualPnlTracker {
    /// Create a new virtual PnL tracker
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            trades: Vec::new(),
            realized_gross_pnl: 0.0,
            maker_fee_rate: 0.0002, // 0.02% maker fee
            taker_fee_rate: 0.0004, // 0.04% taker fee
            is_initialized: false,
        }
    }
    
    /// Initialize the PnL tracker
    pub fn initialize(&mut self, maker_fee: f64, taker_fee: f64) -> Result<(), SimulationError> {
        // The fee schedule is subtracted from every PnL number this tracker
        // produces; a negative rate would inflate a strategy's result instead of
        // costing it, so it is rejected rather than trusted.
        if maker_fee < 0.0 || taker_fee < 0.0 {
            return Err(SimulationError::InvalidConfig(format!(
                "Fee rates must not be negative (maker: {}, taker: {})",
                maker_fee, taker_fee
            )));
        }

        self.maker_fee_rate = maker_fee;
        self.taker_fee_rate = taker_fee;
        self.is_initialized = true;
        info!("Virtual PnL Tracker initialized with maker fee: {:.4}%, taker fee: {:.4}%", 
              maker_fee * 100.0, taker_fee * 100.0);
        Ok(())
    }
    
    /// Record a virtual trade execution
    pub fn record_trade_execution(
        &mut self, 
        intent: &TradeIntent, 
        execution_price: f64,
        is_maker: bool
    ) -> Result<f64, SimulationError> {
        if !self.is_initialized {
            return Err(SimulationError::ExecutionError(
                "PnL tracker not initialized".to_string()
            ));
        }
        
        // Calculate slippage
        let slippage = match intent.side {
            TradeSide::Buy => (execution_price - intent.price).abs(),
            TradeSide::Sell => (intent.price - execution_price).abs(),
        };
        
        // Calculate fees
        let fees = if is_maker {
            execution_price * intent.size * self.maker_fee_rate
        } else {
            execution_price * intent.size * self.taker_fee_rate
        };
        
        // Create executed trade record
        let executed_trade = ExecutedTrade {
            symbol: intent.symbol.clone(),
            side: intent.side,
            price: execution_price,
            quantity: intent.size,
            fees,
            slippage,
            timestamp: intent.timestamp,
            is_maker,
        };
        
        self.trades.push(executed_trade);
        
        // Update positions
        self.update_positions(intent, execution_price, intent.size);
        
        // Calculate PnL for this trade
        let pnl = self.calculate_trade_pnl(intent, execution_price, fees, slippage);
        
        info!("Recorded virtual trade execution: {} {} @ {} (PnL: {:.4})", 
              intent.symbol, 
              match intent.side {
                  TradeSide::Buy => "BUY",
                  TradeSide::Sell => "SELL",
              },
              execution_price,
              pnl);
        
        Ok(pnl)
    }
    
    /// Update positions based on trade execution
    ///
    /// A same-side fill opens a new lot; an opposite-side fill *closes* the open
    /// lots of the other side first (FIFO) and only opens a lot with whatever
    /// quantity is left, which is what turns a fill into realized PnL.
    fn update_positions(&mut self, intent: &TradeIntent, price: f64, quantity: f64) {
        self.book_fill(&intent.symbol, intent.side, price, quantity, intent.timestamp);
    }

    /// Net a fill against the opposite-side lots of `symbol`, booking the gross
    /// PnL of the closed quantity.
    fn book_fill(
        &mut self,
        symbol: &str,
        side: TradeSide,
        price: f64,
        quantity: f64,
        timestamp: u64,
    ) {
        let opposite = match side {
            TradeSide::Buy => TradeSide::Sell,
            TradeSide::Sell => TradeSide::Buy,
        };

        let mut remaining = quantity;

        for lot in &mut self.positions {
            if remaining <= 0.0 {
                break;
            }
            if lot.symbol != symbol || lot.side != opposite {
                continue;
            }

            let closed = lot.quantity.min(remaining);
            lot.quantity -= closed;
            remaining -= closed;

            // Buying covers a short (profit when the entry was above the exit),
            // selling disposes of a long (profit when the exit is above the entry).
            self.realized_gross_pnl += match side {
                TradeSide::Buy => (lot.entry_price - price) * closed,
                TradeSide::Sell => (price - lot.entry_price) * closed,
            };
        }

        // Lots closed to nothing are gone, so `positions` only ever holds exposure
        // that is actually open.
        self.positions.retain(|lot| lot.quantity > MIN_LOT_QUANTITY);

        if remaining > MIN_LOT_QUANTITY {
            self.positions.push(Position {
                symbol: symbol.to_string(),
                side,
                entry_price: price,
                quantity: remaining,
                entry_timestamp: timestamp,
            });
        }
    }
    
    /// Calculate PnL for a single trade
    ///
    /// This is fill quality for one leg - the executed price against the price the
    /// signal asked for, net of that leg's fees and slippage - not a round trip.
    /// The round trip is [`Self::get_realized_pnl`].
    fn calculate_trade_pnl(
        &self, 
        intent: &TradeIntent, 
        execution_price: f64, 
        fees: f64, 
        slippage: f64
    ) -> f64 {
        let gross_pnl = match intent.side {
            TradeSide::Buy => (execution_price - intent.price) * intent.size,
            TradeSide::Sell => (intent.price - execution_price) * intent.size,
        };
        
        gross_pnl - fees - slippage
    }
    
    /// Calculate current unrealized PnL
    pub fn calculate_unrealized_pnl(&self, current_prices: &std::collections::HashMap<String, f64>) -> f64 {
        let mut total_pnl = 0.0;
        
        for position in &self.positions {
            if let Some(current_price) = current_prices.get(&position.symbol) {
                let unrealized_pnl = match position.side {
                    TradeSide::Buy => (current_price - position.entry_price) * position.quantity,
                    TradeSide::Sell => (position.entry_price - current_price) * position.quantity,
                };
                total_pnl += unrealized_pnl;
            }
        }
        
        total_pnl
    }
    
    /// Get total realized PnL: the gross of everything this tracker has closed,
    /// minus the fees charged on all fills.
    ///
    /// Slippage is deliberately not subtracted again here: both legs are priced at
    /// their *execution* prices, which already reflect it.
    ///
    /// Until now this summed `(trade.price - trade.price) * trade.quantity`, which
    /// is identically zero, so a backtest's "realized" PnL was only the fee bill -
    /// a profitable strategy was reported as a loss and a break-even one as a drawdown.
    pub fn get_realized_pnl(&self) -> f64 {
        let fees: f64 = self.trades.iter().map(|trade| trade.fees).sum();
        self.realized_gross_pnl - fees
    }
    
    /// Get open positions (lots that have not been closed out)
    pub fn get_positions(&self) -> &[Position] {
        &self.positions
    }
    
    /// Get trade history
    pub fn get_trade_history(&self) -> &[ExecutedTrade] {
        &self.trades
    }
    
    /// Get current exposure by symbol
    pub fn get_exposure_by_symbol(&self, symbol: &str) -> f64 {
        self.positions.iter()
            .filter(|p| p.symbol == symbol)
            .map(|p| p.quantity * match p.side {
                TradeSide::Buy => 1.0,
                TradeSide::Sell => -1.0,
            })
            .sum()
    }
    
    /// Check if tracker is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(side: TradeSide, price: f64, size: f64) -> TradeIntent {
        TradeIntent {
            symbol: "BTCUSDT".to_string(),
            side,
            price,
            size,
            stop_loss: 0.0,
            take_profit: 0.0,
            time_to_live: 60_000,
            timestamp: 1_700_000_000_000,
        }
    }

    /// A tracker with a zero fee schedule, so the assertions below isolate the
    /// position math from the fee math.
    fn tracker_without_fees() -> VirtualPnlTracker {
        let mut tracker = VirtualPnlTracker::new();
        tracker.initialize(0.0, 0.0).expect("zero fees are a valid schedule");
        tracker
    }

    #[test]
    fn round_trip_realizes_the_closed_leg() {
        let mut tracker = tracker_without_fees();

        tracker
            .record_trade_execution(&intent(TradeSide::Buy, 100.0, 1.0), 100.0, true)
            .unwrap();
        tracker
            .record_trade_execution(&intent(TradeSide::Sell, 110.0, 1.0), 110.0, false)
            .unwrap();

        assert_eq!(tracker.get_realized_pnl(), 10.0);
        assert!(tracker.get_positions().is_empty(), "closed lots must not stay open");
        assert_eq!(tracker.get_exposure_by_symbol("BTCUSDT"), 0.0);
    }

    #[test]
    fn partial_close_keeps_the_unclosed_quantity() {
        let mut tracker = tracker_without_fees();

        tracker
            .record_trade_execution(&intent(TradeSide::Buy, 100.0, 2.0), 100.0, true)
            .unwrap();
        tracker
            .record_trade_execution(&intent(TradeSide::Sell, 110.0, 0.5), 110.0, true)
            .unwrap();

        assert_eq!(tracker.get_realized_pnl(), 5.0);
        assert_eq!(tracker.get_positions().len(), 1);
        assert_eq!(tracker.get_positions()[0].quantity, 1.5);
        assert_eq!(tracker.get_positions()[0].side, TradeSide::Buy);
    }

    #[test]
    fn oversized_flip_closes_then_reopens_on_the_other_side() {
        let mut tracker = tracker_without_fees();

        tracker
            .record_trade_execution(&intent(TradeSide::Buy, 100.0, 1.0), 100.0, true)
            .unwrap();
        tracker
            .record_trade_execution(&intent(TradeSide::Sell, 110.0, 3.0), 110.0, true)
            .unwrap();

        assert_eq!(tracker.get_realized_pnl(), 10.0);
        assert_eq!(tracker.get_exposure_by_symbol("BTCUSDT"), -2.0);
    }

    #[test]
    fn short_cover_books_the_loss() {
        let mut tracker = tracker_without_fees();

        // A short entry, then a cover at a higher price: the loss is real.
        tracker
            .record_trade_execution(&intent(TradeSide::Sell, 100.0, 1.0), 100.0, true)
            .unwrap();
        tracker
            .record_trade_execution(&intent(TradeSide::Buy, 120.0, 1.0), 120.0, true)
            .unwrap();

        assert_eq!(tracker.get_realized_pnl(), -20.0);
    }

    #[test]
    fn same_side_fills_stack_and_close_lot_by_lot() {
        let mut tracker = tracker_without_fees();

        // Two lots opened at different prices, one closing order that covers both.
        tracker
            .record_trade_execution(&intent(TradeSide::Buy, 100.0, 1.0), 100.0, true)
            .unwrap();
        tracker
            .record_trade_execution(&intent(TradeSide::Buy, 120.0, 1.0), 120.0, true)
            .unwrap();
        assert_eq!(tracker.get_positions().len(), 2);

        tracker
            .record_trade_execution(&intent(TradeSide::Sell, 130.0, 2.0), 130.0, true)
            .unwrap();

        // FIFO: 30 on the 100 lot, 10 on the 120 lot.
        assert_eq!(tracker.get_realized_pnl(), 40.0);
        assert!(tracker.get_positions().is_empty());
    }

    #[test]
    fn closed_positions_stop_contributing_unrealized_pnl() {
        let mut tracker = tracker_without_fees();
        let mut prices = std::collections::HashMap::new();
        prices.insert("BTCUSDT".to_string(), 500.0_f64);

        tracker
            .record_trade_execution(&intent(TradeSide::Buy, 100.0, 1.0), 100.0, true)
            .unwrap();
        assert_eq!(tracker.calculate_unrealized_pnl(&prices), 400.0);

        tracker
            .record_trade_execution(&intent(TradeSide::Sell, 100.0, 1.0), 100.0, true)
            .unwrap();
        assert_eq!(tracker.calculate_unrealized_pnl(&prices), 0.0);
    }

    #[test]
    fn fees_are_charged_on_every_fill() {
        let mut tracker = VirtualPnlTracker::new();
        tracker.initialize(0.001, 0.002).unwrap();

        // Break-even round trip at 100 -> 100, both legs takers (0.2 each).
        tracker
            .record_trade_execution(&intent(TradeSide::Buy, 100.0, 1.0), 100.0, false)
            .unwrap();
        tracker
            .record_trade_execution(&intent(TradeSide::Sell, 100.0, 1.0), 100.0, false)
            .unwrap();

        assert_eq!(tracker.get_realized_pnl(), -0.4);
    }

    #[test]
    fn negative_fee_schedule_is_rejected() {
        let mut tracker = VirtualPnlTracker::new();
        assert!(tracker.initialize(-0.001, 0.0004).is_err());
        assert!(!tracker.is_initialized());
    }
}
