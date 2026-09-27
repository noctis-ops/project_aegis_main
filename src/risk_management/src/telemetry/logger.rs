//! Asynchronous logging system

use tracing::{info, debug, warn, error};
use tracing_appender::non_blocking::WorkerGuard;
use std::sync::Arc;

/// Asynchronous logger
pub struct AsyncLogger {
    _guard: Arc<WorkerGuard>, // Keep guard alive
    is_initialized: bool,
}

impl AsyncLogger {
    /// Create a new asynchronous logger
    pub fn new() -> Self {
        // In a real implementation, this would set up tracing with non-blocking appenders
        // For now, we'll use the global subscriber that's already initialized
        
        Self {
            _guard: Arc::new(WorkerGuard::default()), // Placeholder
            is_initialized: true,
        }
    }
    
    /// Log a trade execution event
    pub fn log_trade_execution(&self, trade_id: &str, symbol: &str, pnl: f64) {
        info!("TRADE_EXECUTION: {} {} PNL={:.4}", trade_id, symbol, pnl);
    }
    
    /// Log a circuit breaker event
    pub fn log_circuit_breaker(&self, level: &str, reason: &str) {
        match level {
            "WARNING" => warn!("CIRCUIT_BREAKER: {} - {}", level, reason),
            "HALT" => warn!("CIRCUIT_BREAKER: {} - {}", level, reason),
            "KILLSWITCH" => error!("CIRCUIT_BREAKER: {} - {}", level, reason),
            _ => info!("CIRCUIT_BREAKER: {} - {}", level, reason),
        }
    }
    
    /// Log a risk adjustment event
    pub fn log_risk_adjustment(&self, reason: &str, old_risk: f64, new_risk: f64) {
        info!("RISK_ADJUSTMENT: {} Old={:.4}% New={:.4}%", reason, old_risk * 100.0, new_risk * 100.0);
    }
    
    /// Log a system health check
    pub fn log_health_check(&self, component: &str, status: &str) {
        debug!("HEALTH_CHECK: {} - {}", component, status);
    }
    
    /// Log a recovery event
    pub fn log_recovery_event(&self, event_type: &str, details: &str) {
        info!("RECOVERY: {} - {}", event_type, details);
    }
    
    /// Check if logger is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}