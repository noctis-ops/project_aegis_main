//! Risk management module

use crate::core::{TradeSignal, TradeSide};

/// Risk manager
pub struct RiskManager {
    max_slippage: f64, // Maximum allowed slippage (as fraction of price)
    maker_fee: f64,    // Maker fee rate
    taker_fee: f64,    // Taker fee rate
    min_expected_value: f64, // Minimum net expected value for a trade
}

impl RiskManager {
    /// Create a new risk manager
    pub fn new() -> Self {
        Self {
            max_slippage: 0.001, // 0.1% maximum slippage
            maker_fee: 0.0002,   // 0.02% maker fee
            taker_fee: 0.0004,   // 0.04% taker fee
            min_expected_value: 0.0, // No minimum by default
        }
    }
    
    /// Validate a trade signal based on risk criteria
    pub fn validate_signal(&self, signal: &TradeSignal) -> bool {
        // Check if signal has expired
        if self.is_signal_expired(signal) {
            return false;
        }
        
        // Check net expected value
        if !self.check_net_expected_value(signal) {
            return false;
        }
        
        // Check slippage limits
        if !self.check_slippage_limits(signal) {
            return false;
        }
        
        // Check reasonable take profit to stop loss ratio
        if !self.check_risk_reward_ratio(signal) {
            return false;
        }
        
        true
    }
    
    /// Check if signal has expired
    fn is_signal_expired(&self, signal: &TradeSignal) -> bool {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
            
        current_time > (signal.timestamp + signal.time_to_live)
    }
    
    /// Check net expected value (NEV)
    fn check_net_expected_value(&self, signal: &TradeSignal) -> bool {
        let entry_fee = signal.price * self.maker_fee; // Maker order for entry
        
        let exit_fee = if signal.side == TradeSide::Buy {
            signal.take_profit * self.taker_fee // Taker order for exit (worst case)
        } else {
            signal.stop_loss * self.taker_fee
        };
        
        let estimated_slippage = if signal.side == TradeSide::Buy {
            signal.price * self.max_slippage
        } else {
            signal.price * self.max_slippage
        };
        
        let potential_profit = if signal.side == TradeSide::Buy {
            signal.take_profit - signal.price
        } else {
            signal.price - signal.take_profit
        };
        
        let net_expected_value = potential_profit - entry_fee - exit_fee - estimated_slippage;
        
        net_expected_value > self.min_expected_value
    }
    
    /// Check slippage limits
    fn check_slippage_limits(&self, signal: &TradeSignal) -> bool {
        let stop_loss_distance = if signal.side == TradeSide::Buy {
            signal.price - signal.stop_loss
        } else {
            signal.stop_loss - signal.price
        };
        
        // Stop loss should not be tighter than max slippage
        stop_loss_distance > (signal.price * self.max_slippage)
    }
    
    /// Check risk/reward ratio
    fn check_risk_reward_ratio(&self, signal: &TradeSignal) -> bool {
        let risk = if signal.side == TradeSide::Buy {
            signal.price - signal.stop_loss
        } else {
            signal.stop_loss - signal.price
        };
        
        let reward = if signal.side == TradeSide::Buy {
            signal.take_profit - signal.price
        } else {
            signal.price - signal.take_profit
        };
        
        // Minimum 1:1 risk/reward ratio
        reward >= risk
    }
    
    /// Set risk parameters
    pub fn set_parameters(
        &mut self, 
        max_slippage: f64, 
        maker_fee: f64, 
        taker_fee: f64, 
        min_expected_value: f64
    ) {
        self.max_slippage = max_slippage;
        self.maker_fee = maker_fee;
        self.taker_fee = taker_fee;
        self.min_expected_value = min_expected_value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{TradeSignal, TradeSide};
    
    #[test]
    fn test_signal_validation() {
        let risk_manager = RiskManager::new();
        
        let signal = TradeSignal {
            symbol: "BTCUSDT".to_string(),
            side: TradeSide::Buy,
            price: 50000.0,
            size: 0.0,
            stop_loss: 49500.0, // 1% stop loss
            take_profit: 51000.0, // 2% take profit
            time_to_live: 200,
            confidence: 0.7,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        };
        
        assert!(risk_manager.validate_signal(&signal));
    }
}