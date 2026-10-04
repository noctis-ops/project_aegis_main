//! Market Regime & Funding Guard implementation

use crate::core::{PortfolioState, FundingRateInfo, VolatilityMetrics, trim_to_last};
use tracing::{warn, debug};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Market Regime Guard
///
/// Funding rates and volatility samples arrive on their own cadence and are kept in
/// bounded, append-only buffers. Both readers below take the *newest* sample per
/// symbol: funding is a point-in-time cost, so a stale entry tells the opposite of
/// the truth about whether a LONG is worth opening.
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
    
    /// Evaluate market conditions for one portfolio.
    ///
    /// The two halves are gated differently on purpose: funding is a cost carried
    /// by *open* positions, while a volatility spike gates *new* entries — so a flat
    /// book still has to see the volatility check.
    pub fn evaluate_market_conditions(&self, portfolio_state: &PortfolioState) {
        // A flat book pays no funding; re-scanning the funding history for it turned
        // one stale rate into one warning per stored sample, every second.
        if portfolio_state.total_exposure.abs() > f64::EPSILON {
            self.check_funding_rates();
        } else {
            debug!("MARKET_REGIME: book is flat, skipping funding-rate evaluation");
        }
        
        // Check volatility anomalies (gates new entries, so it always runs)
        self.check_volatility_anomalies();
    }
    
    /// Update funding rate information
    pub fn update_funding_rate(&self, info: FundingRateInfo) {
        let mut rates = self.funding_rates
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        rates.push(info);
        
        trim_to_last(&mut *rates, crate::core::constants::FUNDING_RATE_HISTORY_MAX);
    }
    
    /// Check the newest known funding rate of every symbol for negative values.
    fn check_funding_rates(&self) {
        let rates = self.funding_rates
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        
        // The buffer is append-only, so the last entry per symbol wins; walking the
        // whole history instead turned one bad funding rate into one warning per
        // stored sample per second.
        let mut latest: HashMap<&str, &FundingRateInfo> = HashMap::new();
        for rate_info in rates.iter() {
            latest.insert(rate_info.symbol.as_str(), rate_info);
        }
        
        for rate_info in latest.values() {
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
        let mut history = self.volatility_history
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        history.push(volatility);
        
        // 1000 samples at the 100ms sampling cadence is the 100s window the
        // anomaly detector averages over; beyond that the ratio stops reacting.
        trim_to_last(&mut *history, crate::core::constants::VOLATILITY_HISTORY_MAX);
    }
    
    /// Check if a trade should proceed based on funding costs
    pub fn should_proceed_with_funding_cost(&self, symbol: &str, expected_alpha: f64) -> bool {
        let rates = self.funding_rates
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        
        // Newest sample first: the buffer is append-only, so a forward `find` would
        // price the trade off the oldest funding rate still in the window.
        if let Some(rate_info) = rates.iter().rev().find(|r| r.symbol == symbol) {
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
