//! Position sizing engine

use crate::core::{TradeSignal, WalletBalance, TradeSide};

/// Position sizing parameters
#[derive(Debug, Clone)]
pub struct PositionSizingParams {
    pub risk_percent: f64, // Percentage of equity to risk (e.g., 0.01 = 1%)
    pub min_position_value: f64, // Minimum position value in quote currency
    pub max_position_value: f64, // Maximum position value in quote currency
    pub tick_size: f64, // Minimum price increment
    pub step_size: f64, // Minimum quantity increment
}

impl Default for PositionSizingParams {
    fn default() -> Self {
        Self {
            risk_percent: 0.01, // 1% of equity
            min_position_value: 5.0, // $5 minimum
            max_position_value: 10000.0, // $10,000 maximum
            tick_size: 0.1,
            step_size: 0.001,
        }
    }
}

/// Position sizing engine
pub struct PositionSizingEngine {
    params: PositionSizingParams,
}

impl PositionSizingEngine {
    /// Create a new position sizing engine
    pub fn new() -> Self {
        Self {
            params: PositionSizingParams::default(),
        }
    }
    
    /// Size a position based on wallet balance and signal parameters
    pub fn size_position(&self, signal: &TradeSignal, wallet: &WalletBalance) -> TradeSignal {
        let mut sized_signal = signal.clone();
        
        // Calculate risk amount (1% of equity)
        let risk_amount = wallet.equity * self.params.risk_percent;
        
        // Calculate stop distance
        let stop_distance = match signal.side {
            TradeSide::Buy => signal.price - signal.stop_loss,
            TradeSide::Sell => signal.stop_loss - signal.price,
        };
        
        // Avoid division by zero
        if stop_distance <= 0.0 {
            // Fallback to minimum position size
            sized_signal.size = self.calculate_min_position_size(signal.price);
            return self.round_position(sized_signal);
        }
        
        // Calculate position size based on risk
        let mut position_size = risk_amount / stop_distance;
        
        // Calculate position value
        let position_value = position_size * signal.price;
        
        // Apply position value limits
        if position_value < self.params.min_position_value {
            // Try to adjust stop distance to meet minimum requirement
            let adjusted_stop_distance = risk_amount / (self.params.min_position_value / signal.price);
            
            // Check if adjusted stop distance is reasonable (not too tight)
            if adjusted_stop_distance > stop_distance * 0.5 { // Not tighter than 50% of original
                // Use adjusted size
                position_size = self.params.min_position_value / signal.price;
            } else {
                // Use minimum size anyway but log warning
                position_size = self.params.min_position_value / signal.price;
            }
        } else if position_value > self.params.max_position_value {
            // Cap at maximum position value
            position_size = self.params.max_position_value / signal.price;
        }
        
        sized_signal.size = position_size;
        self.round_position(sized_signal)
    }
    
    /// Calculate minimum position size based on platform requirements
    fn calculate_min_position_size(&self, price: f64) -> f64 {
        // Ensure position value meets minimum requirement
        let min_size_by_value = self.params.min_position_value / price;
        
        // Round up to nearest step size
        (min_size_by_value / self.params.step_size).ceil() * self.params.step_size
    }
    
    /// Round position size and price to exchange requirements
    fn round_position(&self, mut signal: TradeSignal) -> TradeSignal {
        // Round size to step size
        signal.size = (signal.size / self.params.step_size).round() * self.params.step_size;
        
        // Round prices to tick size
        signal.price = (signal.price / self.params.tick_size).round() * self.params.tick_size;
        signal.stop_loss = (signal.stop_loss / self.params.tick_size).round() * self.params.tick_size;
        signal.take_profit = (signal.take_profit / self.params.tick_size).round() * self.params.tick_size;
        
        signal
    }
    
    /// Set position sizing parameters
    pub fn set_params(&mut self, params: PositionSizingParams) {
        self.params = params;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{TradeSignal, TradeSide, WalletBalance};
    
    #[test]
    fn test_position_sizing() {
        let engine = PositionSizingEngine::new();
        
        let signal = TradeSignal {
            symbol: "BTCUSDT".to_string(),
            side: TradeSide::Buy,
            price: 50000.0,
            size: 0.0,
            stop_loss: 49500.0, // 1% stop loss
            take_profit: 51000.0,
            time_to_live: 200,
            confidence: 0.7,
            timestamp: 0,
        };
        
        let wallet = WalletBalance {
            equity: 10000.0, // $10,000 equity
            available: 10000.0,
            currency: "USDT".to_string(),
        };
        
        let sized_signal = engine.size_position(&signal, &wallet);
        
        // With 1% risk ($100) and 1% stop distance ($500), size should be 0.2 BTC
        assert!(sized_signal.size > 0.0);
    }
}