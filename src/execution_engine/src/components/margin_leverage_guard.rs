//! Margin & Leverage Guard implementation

use crate::core::{TradeIntent, OrderSide, ExecutionError};
use tracing::{info, warn, error, debug};
use crate::core::constants::*;

/// Margin & Leverage Guard
pub struct MarginLeverageGuard {
    default_leverage: u32,
    min_notional: f64,
    tick_size: f64,
}

impl MarginLeverageGuard {
    /// Create a new Margin & Leverage Guard
    pub fn new() -> Self {
        Self {
            default_leverage: DEFAULT_LEVERAGE,
            min_notional: MIN_NOTIONAL,
            tick_size: DEFAULT_TICK_SIZE,
        }
    }
    
    /// Validate order parameters before sending to exchange
    pub fn validate_order_parameters(&self, intent: &TradeIntent) -> bool {
        debug!("Validating order parameters for symbol: {}", intent.symbol);
        
        // Check notional value
        let notional = intent.price * intent.size;
        if notional < self.min_notional {
            error!("Order notional {} is below minimum {}", notional, self.min_notional);
            return false;
        }
        
        // Check lot size constraints
        if !self.validate_lot_size(intent.size) {
            error!("Order quantity {} violates lot size constraints", intent.size);
            return false;
        }
        
        // Check price tick size constraints
        if !self.validate_price_tick(intent.price) {
            error!("Order price {} violates tick size constraints", intent.price);
            return false;
        }
        
        // Check bracket order validity (stop loss and take profit)
        if !self.validate_bracket_orders(intent) {
            error!("Bracket orders are invalid for symbol: {}", intent.symbol);
            return false;
        }
        
        info!("Order parameters validated successfully for symbol: {}", intent.symbol);
        true
    }
    
    /// Validate lot size constraints
    fn validate_lot_size(&self, quantity: f64) -> bool {
        // In a real implementation, we would fetch the actual step size from Binance
        // For now, we'll use a simple validation
        let step_size = 0.001; // Example step size
        let remainder = quantity % step_size;
        remainder.abs() < f64::EPSILON || (step_size - remainder).abs() < f64::EPSILON
    }
    
    /// Validate price tick size constraints
    fn validate_price_tick(&self, price: f64) -> bool {
        let remainder = price % self.tick_size;
        remainder.abs() < f64::EPSILON || (self.tick_size - remainder).abs() < f64::EPSILON
    }
    
    /// Validate bracket orders (stop loss and take profit)
    fn validate_bracket_orders(&self, intent: &TradeIntent) -> bool {
        // Stop loss should be in the opposite direction
        let stop_loss_valid = match intent.side {
            OrderSide::Buy => intent.stop_loss < intent.price,
            OrderSide::Sell => intent.stop_loss > intent.price,
        };
        
        // Take profit should be in the same direction
        let take_profit_valid = match intent.side {
            OrderSide::Buy => intent.take_profit > intent.price,
            OrderSide::Sell => intent.take_profit < intent.price,
        };
        
        // Reasonable risk/reward ratio (at least 1:1)
        let risk = (intent.price - intent.stop_loss).abs();
        let reward = (intent.take_profit - intent.price).abs();
        let risk_reward_valid = reward >= risk;
        
        stop_loss_valid && take_profit_valid && risk_reward_valid
    }
    
    /// Pre-trade notional check
    pub fn pre_trade_notional_check(&self, price: f64, quantity: f64) -> Result<f64, ExecutionError> {
        let notional = price * quantity;
        
        if notional < self.min_notional {
            error!("Notional value {} is below minimum {}", notional, self.min_notional);
            return Err(ExecutionError::InsufficientMargin(
                format!("Notional value {} is below minimum {}", notional, self.min_notional)
            ));
        }
        
        Ok(notional)
    }
    
    /// Adapt to constraint violations
    pub fn adapt_to_constraints(&self, price: f64, quantity: f64) -> Option<(f64, f64)> {
        let notional = price * quantity;
        
        if notional < self.min_notional {
            // Try to adjust quantity to meet minimum notional
            let adjusted_quantity = self.min_notional / price;
            
            // Check if adjusted quantity is reasonable
            if adjusted_quantity > quantity * 10.0 {
                // Adjustment would increase position by more than 10x, reject
                warn!("Constraint adaptation would increase position by more than 10x, rejecting");
                return None;
            }
            
            // Round to valid lot size
            let step_size = 0.001; // Example step size
            let rounded_quantity = (adjusted_quantity / step_size).round() * step_size;
            
            Some((price, rounded_quantity))
        } else {
            Some((price, quantity))
        }
    }
    
    /// Set default leverage
    pub fn set_default_leverage(&mut self, leverage: u32) {
        self.default_leverage = leverage;
    }
    
    /// Set minimum notional value
    pub fn set_min_notional(&mut self, min_notional: f64) {
        self.min_notional = min_notional;
    }
    
    /// Set tick size
    pub fn set_tick_size(&mut self, tick_size: f64) {
        self.tick_size = tick_size;
    }
}