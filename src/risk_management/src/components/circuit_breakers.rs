//! Hierarchical Circuit Breakers implementation

use crate::core::{PortfolioState, PerformanceMetrics, CircuitBreakerStatus, RiskConfig, trim_to_last};
use tracing::{info, warn, error};
use std::sync::{Arc, RwLock};

/// Circuit Breaker System
///
/// The drawdown thresholds are configuration, not compile-time constants: sizing and
/// risk limits come from `RiskConfig`, so a portfolio tuned to a different risk
/// budget must not keep tripping on the shipped defaults. They are plain fields
/// because they never change after construction — a clone handed to a monitoring
/// task therefore observes the same limits, and cannot drift from the ones the
/// Global Risk Manager was built with.
#[derive(Clone)]
pub struct CircuitBreakerSystem {
    status: Arc<RwLock<CircuitBreakerStatus>>,
    latency_history: Arc<std::sync::RwLock<Vec<f64>>>,
    rejection_rate_history: Arc<std::sync::RwLock<Vec<f64>>>,
    last_flash_crash_check: Arc<std::sync::RwLock<std::time::Instant>>,
    daily_drawdown_limit: f64,
    weekly_drawdown_limit: f64,
}

impl CircuitBreakerSystem {
    /// Create a new Circuit Breaker System with the shipped default limits
    pub fn new() -> Self {
        Self {
            status: Arc::new(RwLock::new(CircuitBreakerStatus::Normal)),
            latency_history: Arc::new(RwLock::new(Vec::new())),
            rejection_rate_history: Arc::new(RwLock::new(Vec::new())),
            last_flash_crash_check: Arc::new(RwLock::new(std::time::Instant::now())),
            daily_drawdown_limit: crate::core::constants::ORANGE_HALT_DAILY_DRAWDOWN,
            weekly_drawdown_limit: crate::core::constants::RED_KILLSWITCH_WEEKLY_DRAWDOWN,
        }
    }
    
    /// Create a Circuit Breaker System whose drawdown limits follow the risk
    /// configuration, so `RiskConfig` is the single source of truth.
    pub fn with_config(config: &RiskConfig) -> Self {
        Self {
            daily_drawdown_limit: config.daily_drawdown_limit,
            weekly_drawdown_limit: config.weekly_drawdown_limit,
            ..Self::new()
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
        // For now, we'll use a simplified check against the configured limit
        let drawdown = (portfolio_state.realized_pnl / portfolio_state.equity).abs();
        drawdown > self.daily_drawdown_limit
    }
    
    /// Check for weekly drawdown
    fn check_weekly_drawdown(&self, portfolio_state: &PortfolioState) -> bool {
        // In a real implementation, we would compare current equity to weekly high
        // For now, we'll use a simplified check against the configured limit
        let drawdown = (portfolio_state.realized_pnl / portfolio_state.equity).abs();
        drawdown > self.weekly_drawdown_limit
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

#[cfg(test)]
mod tests {
    use super::CircuitBreakerSystem;
    use crate::core::{CircuitBreakerStatus, PortfolioState, RiskConfig};
    
    fn portfolio_with_realized_loss(equity: f64, realized_pnl: f64) -> PortfolioState {
        PortfolioState {
            equity,
            available_balance: equity,
            total_exposure: 0.0,
            floating_pnl: 0.0,
            realized_pnl,
            timestamp: 0,
        }
    }
    
    #[test]
    fn five_percent_down_halts_entries_on_the_shipped_limits() {
        // 5% realized loss against the 2% daily limit -> orange halt, and the daily
        // check runs first so the weekly one is not consulted.
        let system = CircuitBreakerSystem::new();
        system.evaluate_portfolio_state(&portfolio_with_realized_loss(1000.0, -50.0));
        assert_eq!(system.get_status(), CircuitBreakerStatus::HaltEntries);
    }
    
    #[test]
    fn the_configured_daily_limit_is_what_gates_the_breaker() {
        // The same 5% drawdown against a widened 10% / 20% configuration stays green:
        // proof that RiskConfig drives the trigger, which it previously could not.
        let config = RiskConfig {
            daily_drawdown_limit: 0.10,
            weekly_drawdown_limit: 0.20,
            ..RiskConfig::default()
        };
        
        let system = CircuitBreakerSystem::with_config(&config);
        system.evaluate_portfolio_state(&portfolio_with_realized_loss(1000.0, -50.0));
        assert_eq!(system.get_status(), CircuitBreakerStatus::Normal);
    }
    
    #[test]
    fn the_weekly_limit_still_reaches_the_kill_switch() {
        // Daily is looser than weekly here, so the loss passes the daily gate and the
        // weekly one escalates all the way to the kill switch.
        let config = RiskConfig {
            daily_drawdown_limit: 0.30,
            weekly_drawdown_limit: 0.20,
            ..RiskConfig::default()
        };
        
        let system = CircuitBreakerSystem::with_config(&config);
        system.evaluate_portfolio_state(&portfolio_with_realized_loss(1000.0, -250.0));
        assert_eq!(system.get_status(), CircuitBreakerStatus::KillSwitch);
    }
}
