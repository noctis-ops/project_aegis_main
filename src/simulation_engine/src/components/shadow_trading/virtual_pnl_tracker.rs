//! Virtual PnL Tracker for Shadow Trading

use crate::core::{TradeIntent, TradeSide, SimulationError};
use tracing::{info, debug, warn, error};

/// Virtual PnL Tracker
pub struct VirtualPnlTracker {
    positions: Vec<Position>,
    trades: Vec<ExecutedTrade>,
    maker_fee_rate: f64,
    taker_fee_rate: f64,
    is_initialized: bool,
}

/// Position tracking
#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: TradeSide,
    pub entry_price: f64,
    pub quantity: f64,
    pub entry_timestamp: i64,
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
    pub timestamp: i64,
    pub is_maker: bool,
}

impl VirtualPnlTracker {
    /// Create a new virtual PnL tracker
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            trades: Vec::new(),
            maker_fee_rate: 0.0002, // 0.02% maker fee
            taker_fee_rate: 0.0004, // 0.04% taker fee
            is_initialized: false,
        }
    }
    
    /// Initialize the PnL tracker
    pub fn initialize(&mut self, maker_fee: f64, taker_fee: f64) -> Result<(), SimulationError> {
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
            side: intent.side.clone(),
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
    fn update_positions(&mut self, intent: &TradeIntent, price: f64, quantity: f64) {
        match intent.side {
            TradeSide::Buy => {
                self.positions.push(Position {
                    symbol: intent.symbol.clone(),
                    side: TradeSide::Buy,
                    entry_price: price,
                    quantity,
                    entry_timestamp: intent.timestamp,
                });
            }
            TradeSide::Sell => {
                self.positions.push(Position {
                    symbol: intent.symbol.clone(),
                    side: TradeSide::Sell,
                    entry_price: price,
                    quantity,
                    entry_timestamp: intent.timestamp,
                });
            }
        }
    }
    
    /// Calculate PnL for a single trade
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
    
    /// Get total realized PnL
    pub fn get_realized_pnl(&self) -> f64 {
        self.trades.iter().map(|trade| {
            match trade.side {
                TradeSide::Buy => (trade.price - trade.price) * trade.quantity - trade.fees - trade.slippage,
                TradeSide::Sell => (trade.price - trade.price) * trade.quantity - trade.fees - trade.slippage,
            }
        }).sum()
    }
    
    /// Get current positions
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