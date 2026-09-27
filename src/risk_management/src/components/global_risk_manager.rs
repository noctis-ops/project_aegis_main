//! Global Risk Manager implementation

use crate::core::{RiskConfig, PortfolioState, TradeRecord, MaeMfeMetrics};
use tracing::{info, warn, debug};
use std::collections::VecDeque;

/// Global Risk Manager
pub struct GlobalRiskManager {
    config: RiskConfig,
    current_risk_percentage: f64,
    consecutive_losses: usize,
    trade_history: VecDeque<TradeRecord>,
    mae_mfe_tracking: Vec<MaeMfeMetrics>,
    max_history_size: usize,
}

impl GlobalRiskManager {
    /// Create a new Global Risk Manager
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            current_risk_percentage: config.default_risk_percentage,
            consecutive_losses: 0,
            trade_history: VecDeque::new(),
            mae_mfe_tracking: Vec::new(),
            max_history_size: 1000, // Keep last 1000 trades
        }
    }
    
    /// Calculate risk amount based on live equity
    pub fn calculate_risk_amount(&self, portfolio_state: &PortfolioState) -> f64 {
        portfolio_state.available_balance * self.current_risk_percentage
    }
    
    /// Update risk percentage based on trade results
    pub fn update_risk_percentage(&mut self, trade_record: &TradeRecord) {
        if trade_record.is_winner {
            // Reset consecutive losses counter
            self.consecutive_losses = 0;
            
            // Gradually increase risk if we have consistent winners
            if self.trade_history.len() >= 3 {
                let recent_wins = self.trade_history.iter().rev().take(3).all(|t| t.is_winner);
                if recent_wins {
                    self.current_risk_percentage = (self.current_risk_percentage * 1.1)
                        .min(self.config.default_risk_percentage); // Cap at default
                    info!("Increasing risk percentage to {:.4}%", self.current_risk_percentage * 100.0);
                }
            }
        } else {
            // Apply consecutive loss dampener
            self.consecutive_losses += 1;
            self.current_risk_percentage *= crate::core::constants::CONSECUTIVE_LOSS_DAMPENER;
            warn!("Decreasing risk percentage to {:.4}% due to consecutive loss #{}", 
                  self.current_risk_percentage * 100.0, self.consecutive_losses);
        }
        
        // Add to trade history
        self.trade_history.push_back(trade_record.clone());
        
        // Maintain history size
        if self.trade_history.len() > self.max_history_size {
            self.trade_history.pop_front();
        }
    }
    
    /// Calculate Net Expected Value (NEV) for a trade
    pub fn calculate_net_expected_value(&self, trade_record: &TradeRecord) -> f64 {
        // Net EV = (Exit Price - Entry Price) - (Taker Fees * 2) - Slippage
        let price_difference = if trade_record.side == crate::core::TradeSide::Buy {
            trade_record.exit_price - trade_record.entry_price
        } else {
            trade_record.entry_price - trade_record.exit_price
        };
        
        let total_fees = trade_record.fees * 2.0; // Entry + Exit fees
        let net_ev = price_difference - total_fees - trade_record.slippage;
        
        debug!("Trade NEV calculation: Price Diff={} - Fees={} - Slippage={} = Net EV={}",
               price_difference, total_fees, trade_record.slippage, net_ev);
        
        net_ev
    }
    
    /// Calculate rolling Net Expected Value for recent trades
    pub fn calculate_rolling_net_ev(&self, window_size: usize) -> f64 {
        let window_size = window_size.min(self.trade_history.len());
        if window_size == 0 {
            return 0.0;
        }
        
        let recent_trades: Vec<&TradeRecord> = self.trade_history.iter().rev().take(window_size).collect();
        let total_ev: f64 = recent_trades.iter()
            .map(|trade| self.calculate_net_expected_value(trade))
            .sum();
            
        total_ev / window_size as f64
    }
    
    /// Check if Net EV is positive (system is profitable)
    pub fn is_positive_expectancy(&self, window_size: usize) -> bool {
        self.calculate_rolling_net_ev(window_size) > 0.0
    }
    
    /// Track MAE/MFE for a trade
    pub fn track_mae_mfe(&mut self, metrics: MaeMfeMetrics) {
        self.mae_mfe_tracking.push(metrics);
        
        // Check if stop loss is being consistently breached
        if metrics.is_stop_loss_breached {
            let breach_count = self.mae_mfe_tracking.iter()
                .filter(|m| m.is_stop_loss_breached)
                .count();
                
            let total_count = self.mae_mfe_tracking.len();
            
            if total_count > 10 && (breach_count as f64 / total_count as f64) > 0.8 {
                warn!("High stop loss breach rate detected: {}/{} ({:.1}%)", 
                      breach_count, total_count, (breach_count as f64 / total_count as f64) * 100.0);
                // This would trigger an adjustment in Layer 2 signal generation
            }
        }
    }
    
    /// Get current risk percentage
    pub fn get_current_risk_percentage(&self) -> f64 {
        self.current_risk_percentage
    }
    
    /// Get consecutive losses count
    pub fn get_consecutive_losses(&self) -> usize {
        self.consecutive_losses
    }
    
    /// Reset risk percentage to default (used during recovery)
    pub fn reset_risk_percentage(&mut self) {
        self.current_risk_percentage = self.config.default_risk_percentage;
        self.consecutive_losses = 0;
        info!("Risk percentage reset to default: {:.4}%", self.current_risk_percentage * 100.0);
    }
    
    /// Set cautious risk percentage (during recovery)
    pub fn set_cautious_risk_percentage(&mut self) {
        self.current_risk_percentage = crate::core::constants::CAUTIOUS_RESUMPTION_RISK;
        info!("Risk percentage set to cautious level: {:.4}%", self.current_risk_percentage * 100.0);
    }
}