//! Market Regime & Funding Guard implementation

use crate::core::{PortfolioState, FundingRateInfo, VolatilityMetrics};
use tracing::{info, warn, debug};
use std::sync::{Arc, RwLock};

/// Market Regime Guard
#[derive(Clone)]
pub struct MarketRegimeGuard {
    funding_rates: Arc<std::sync::RwLock<Vec<FundingRateInfo>>>,
    volatility_history: Arc<std::sync::RwLock<Vec<f64>>>,
    last_volatility_check: Arc<std::sync::RwLock<std::time::Instant>>,
}

impl MarketRegimeGuard {
    /// Create a new Market Regime Guard
    pub fn new() -> Self {
        Self {
            funding_rates: Arc::new(RwLock::new(Vec::new())),
            volatility_history: Arc::new(RwLock::new(Vec::new())),
            last_volatility_check: Arc::new(RwLock::new(std::time::Instant::now())),
        }
    }
    
    /// Evaluate market conditions
    pub fn evaluate_market_conditions(&self, portfolio_state: &PortfolioState) {
        // Check funding rates for all positions
        self.check_funding_rates();
        
        // Check volatility anomalies
        self.check_volatility_anomalies();
    }
    
    /// Update funding rate information
    pub fn update_funding_rate(&self, info: FundingRateInfo) {
        let mut rates = self.funding_rates.write().unwrap();
        rates.push(info);
        
        // Keep only last 100 funding rates
        if rates.len() > 100 {
            rates.drain(..rates.len()-100);
        }
    }
    
    /// Check funding rates for negative values
    fn check_funding_rates(&self) {
        let rates = self.funding_rates.read().unwrap();
        
        for rate_info in rates.iter() {
            if rate_info.rate < crate::core::constants::MAX_NEGATIVE_FUNDING_RATE {
                warn!("Negative funding rate detected for {}: {:.4}% - Consider avoiding LONG positions", 
                      rate_info.symbol, rate_info.rate * 100.0);
            }
        }
    }
    
    /// Check for volatility anomalies
    pub fn check_volatility_anomalies(&self) -> Option<VolatilityMetrics> {
        let now = std::time::Instant::now();
        let last_check = {
            let guard = self.last_volatility_check.read().unwrap();
            *guard
        };
        
        // Only check every 100ms to avoid excessive computation
        if now.duration_since(last_check).as_millis() < 100 {
            return None;
        }
        
        // Update last check time
        {
            let mut guard = self.last_volatility_check.write().unwrap();
            *guard = now;
        }
        
        // Calculate current volatility metrics
        let metrics = self.calculate_volatility_metrics();
        
        if let Some(ref metrics) = metrics {
            if metrics.is_anomalous {
                warn!("VOLATILITY ANOMALY DETECTED: Ratio={:.2}x (threshold: {:.1}x)", 
                      metrics.volatility_ratio, crate::core::constants::VOLATILITY_ANOMALY_THRESHOLD);
                // This would trigger circuit breaker level 2
            }
        }
        
        metrics
    }
    
    /// Calculate volatility metrics
    fn calculate_volatility_metrics(&self) -> Option<VolatilityMetrics> {
        let history = self.volatility_history.read().unwrap();
        
        if history.len() < 10 {
            return None; // Not enough data
        }
        
        // Calculate current volatility (last 10 seconds worth of data)
        let window_size = (crate::core::constants::VOLATILITY_WINDOW_SECONDS * 10) as usize; // 100ms intervals
        let current_window_size = window_size.min(history.len());
        
        let current_volatility: f64 = if current_window_size > 0 {
            history.iter().rev().take(current_window_size).sum::<f64>() / current_window_size as f64
        } else {
            0.0
        };
        
        // Calculate average volatility over longer period
        let average_volatility: f64 = history.iter().sum::<f64>() / history.len() as f64;
        
        // Calculate ratio
        let volatility_ratio = if average_volatility > 0.0 {
            current_volatility / average_volatility
        } else {
            1.0
        };
        
        // Check if anomalous
        let is_anomalous = volatility_ratio > crate::core::constants::VOLATILITY_ANOMALY_THRESHOLD;
        
        Some(VolatilityMetrics {
            current_volatility,
            average_volatility,
            volatility_ratio,
            is_anomalous,
        })
    }
    
    /// Update volatility measurement
    pub fn update_volatility(&self, volatility: f64) {
        let mut history = self.volatility_history.write().unwrap();
        history.push(volatility);
        
        // Keep only last 1000 measurements (100 seconds of data)
        if history.len() > 1000 {
            history.drain(..history.len()-1000);
        }
    }
    
    /// Check if a trade should proceed based on funding costs
    pub fn should_proceed_with_funding_cost(&self, symbol: &str, expected_alpha: f64) -> bool {
        let rates = self.funding_rates.read().unwrap();
        
        if let Some(rate_info) = rates.iter().find(|r| r.symbol == symbol) {
            // Check if funding cost exceeds expected alpha by required margin
            let funding_cost = rate_info.rate.abs();
            let required_alpha = funding_cost * crate::core::constants::FUNDING_COST_MULTIPLIER;
            
            if expected_alpha < required_alpha {
                warn!("Funding cost for {} ({:.4}%) exceeds required alpha ({:.4}%)", 
                      symbol, funding_cost * 100.0, required_alpha * 100.0);
                return false;
            }
        }
        
        true
    }
}