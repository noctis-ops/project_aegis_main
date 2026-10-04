//! Hierarchical Circuit Breakers implementation

use crate::core::{PortfolioState, PerformanceMetrics, CircuitBreakerStatus, trim_to_last};
use tracing::{info, warn, error};
use std::sync::{Arc, RwLock};

/// Circuit Breaker System
#[derive(Clone)]
pub struct CircuitBreakerSystem {
    status: Arc<RwLock<CircuitBreakerStatus>>,
    latency_history: Arc<std::sync::RwLock<Vec<f64>>>,
    rejection_rate_history: Arc<std::sync::RwLock<Vec<f64>>>,
    last_flash_crash_check: Arc<std::sync::RwLock<std::time::Instant>>,
}

impl CircuitBreakerSystem {
    /// Create a new Circuit Breaker System
    pub fn new() -> Self {
        Self {
            status: Arc::new(RwLock::new(CircuitBreakerStatus::Normal)),
            latency_history: Arc::new(RwLock::new(Vec::new())),
            rejection_rate_history: Arc::new(RwLock::new(Vec::new())),
            last_flash_crash_check: Arc::new(RwLock::new(std::time::Instant::now())),
        }
    }
    
    /// Evaluate portfolio state for circuit breaker triggers
    pub fn evaluate_portfolio_state(&self, portfolio_state: &PortfolioState) {
        // Check for daily drawdown
        if self.check_daily_drawdown(portfolio_state) {
            self.activate_circuit_breaker(CircuitBreakerStatus::HaltEntries, 
                                       "Daily drawdown limit exceeded");
            return;
        }
        
        // Check for weekly drawdown
        if self.check_weekly_drawdown(portfolio_state) {
            self.activate_circuit_breaker(CircuitBreakerStatus::KillSwitch, 
                                       "Weekly drawdown limit exceeded");
            return;
        }
    }
    
    /// Evaluate performance metrics for circuit breaker triggers
    pub fn evaluate_performance_metrics(&self, metrics: &PerformanceMetrics) {
        // Check for negative expectancy
        if metrics.rolling_net_ev < 0.0 {
            self.activate_circuit_breaker(CircuitBreakerStatus::HaltEntries, 
                                       "Negative rolling net expected value detected");
            return;
        }
    }
    
    /// Check for daily drawdown
    fn check_daily_drawdown(&self, portfolio_state: &PortfolioState) -> bool {
        // In a real implementation, we would compare current equity to daily high
        // For now, we'll use a simplified check
        let drawdown = (portfolio_state.realized_pnl / portfolio_state.equity).abs();
        drawdown > crate::core::constants::ORANGE_HALT_DAILY_DRAWDOWN
    }
    
    /// Check for weekly drawdown
    fn check_weekly_drawdown(&self, portfolio_state: &PortfolioState) -> bool {
        // In a real implementation, we would compare current equity to weekly high
        // For now, we'll use a simplified check
        let drawdown = (portfolio_state.realized_pnl / portfolio_state.equity).abs();
        drawdown > crate::core::constants::RED_KILLSWITCH_WEEKLY_DRAWDOWN
    }
    
    /// Check for latency issues (Yellow Alert)
    pub fn check_latency_alert(&self, latency_ms: f64) -> bool {
        // Add to latency history
        {
            let mut history = self.latency_history.write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            history.push(latency_ms);
            
            // Bounded window: this is fed on every latency sample for the lifetime
            // of the process, so the cap is what keeps it from growing forever.
            trim_to_last(&mut *history, crate::core::constants::LATENCY_HISTORY_MAX);
        }
        
        // Check if current latency exceeds threshold
        latency_ms > crate::core::constants::YELLOW_ALERT_LATENCY_MS as f64
    }
    
    /// Check for order rejection rate issues (Yellow Alert)
    pub fn check_rejection_rate_alert(&self, rejection_rate: f64) -> bool {
        // Add to rejection rate history
        {
            let mut history = self.rejection_rate_history.write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            history.push(rejection_rate);
            
            trim_to_last(&mut *history, crate::core::constants::REJECTION_RATE_HISTORY_MAX);
        }
        
        // Check if current rejection rate exceeds threshold
        rejection_rate > crate::core::constants::YELLOW_ALERT_REJECTION_RATE
    }
    
    /// Check for flash crash conditions (Red Kill-Switch)
    pub fn check_flash_crash(&self, price_change_percent: f64) -> bool {
        let now = std::time::Instant::now();
        let last_check = {
            let guard = self.last_flash_crash_check.read().unwrap();
            *guard
        };
        
        // Only check every 100ms to avoid excessive computation
        if now.duration_since(last_check).as_millis() < 100 {
            return false;
        }
        
        // Update last check time
        {
            let mut guard = self.last_flash_crash_check.write().unwrap();
            *guard = now;
        }
        
        // Check for rapid price movement (>3% in <10 seconds)
        price_change_percent.abs() > crate::core::constants::RED_KILLSWITCH_FLASH_CRASH_THRESHOLD
    }
    
    /// Activate circuit breaker
    fn activate_circuit_breaker(&self, status: CircuitBreakerStatus, reason: &str) {
        let current_status = {
            let guard = self.status.read().unwrap();
            guard.clone()
        };
        
        // Only escalate, don't downgrade
        if self.should_escalate(&current_status, &status) {
            {
                let mut guard = self.status.write().unwrap();
                *guard = status.clone();
            }
            
            match status {
                CircuitBreakerStatus::Warning => {
                    warn!("YELLOW ALERT: {} - {}", reason, status_to_string(&status));
                }
                CircuitBreakerStatus::HaltEntries => {
                    warn!("ORANGE HALT: {} - {}", reason, status_to_string(&status));
                }
                CircuitBreakerStatus::KillSwitch => {
                    error!("RED KILL-SWITCH: {} - {}", reason, status_to_string(&status));
                    // In a real implementation, this would trigger emergency procedures
                }
                CircuitBreakerStatus::Normal => {
                    info!("NORMAL OPERATION RESUMED: {}", reason);
                }
            }
        }
    }
    
    /// Check if we should escalate circuit breaker status
    fn should_escalate(&self, current: &CircuitBreakerStatus, new: &CircuitBreakerStatus) -> bool {
        match (current, new) {
            (_, CircuitBreakerStatus::KillSwitch) => true, // Always escalate to kill switch
            (CircuitBreakerStatus::KillSwitch, _) => false, // Never downgrade from kill switch
            (_, CircuitBreakerStatus::HaltEntries) => {
                *current != CircuitBreakerStatus::HaltEntries && 
                *current != CircuitBreakerStatus::KillSwitch
            }
            (_, CircuitBreakerStatus::Warning) => {
                *current == CircuitBreakerStatus::Normal
            }
            _ => false
        }
    }
    
    /// Get current circuit breaker status
    pub fn get_status(&self) -> CircuitBreakerStatus {
        let guard = self.status.read().unwrap();
        guard.clone()
    }
    
    /// Reset circuit breaker to normal (used during recovery)
    pub fn reset_to_normal(&self) {
        let mut guard = self.status.write().unwrap();
        *guard = CircuitBreakerStatus::Normal;
        info!("Circuit breaker reset to NORMAL status");
    }
}

/// Convert circuit breaker status to string
fn status_to_string(status: &CircuitBreakerStatus) -> &'static str {
    match status {
        CircuitBreakerStatus::Normal => "Normal Operation",
        CircuitBreakerStatus::Warning => "Yellow Alert - Reduced Risk",
        CircuitBreakerStatus::HaltEntries => "Orange Halt - No New Entries",
        CircuitBreakerStatus::KillSwitch => "Red Kill-Switch - Full Shutdown",
    }
}
