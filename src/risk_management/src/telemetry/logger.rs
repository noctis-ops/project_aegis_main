//! Asynchronous logging system
//!
//! Like every other AEGIS layer, Layer 4 emits through the process-wide `tracing`
//! subscriber installed by the launcher. When an operator wants the risk trail in a
//! dedicated non-blocking file, the writer is built with
//! `tracing_appender::non_blocking`, the resulting `NonBlocking` is installed as a
//! `fmt` layer, and the `WorkerGuard` returned next to it is handed to
//! [`AsyncLogger::with_guard`].
//!
//! The guard is *owned* here for the whole run on purpose: dropping it flushes and
//! stops the writer thread, so a logger that only borrowed it would silently discard
//! buffered records at shutdown - and the records this layer buffers are exactly the
//! kill-switch trail an operator reads afterwards.

use tracing::{info, debug, warn, error};
use tracing_appender::non_blocking::WorkerGuard;
use std::sync::Arc;

/// Asynchronous logger
pub struct AsyncLogger {
    /// Keeps the non-blocking writer thread alive. `None` means records go straight
    /// to the process-wide subscriber instead of a dedicated sink.
    _guard: Option<Arc<WorkerGuard>>,
    has_dedicated_sink: bool,
}

impl AsyncLogger {
    /// Log through the process-wide subscriber only (no dedicated file sink).
    pub fn new() -> Self {
        Self {
            _guard: None,
            has_dedicated_sink: false,
        }
    }
    
    /// Attach a caller-created non-blocking writer guard and keep it (with its writer
    /// thread) alive for as long as this logger lives.
    pub fn with_guard(guard: WorkerGuard) -> Self {
        Self {
            _guard: Some(Arc::new(guard)),
            has_dedicated_sink: true,
        }
    }
    
    /// Whether a dedicated non-blocking sink is attached to this logger.
    pub fn has_dedicated_sink(&self) -> bool {
        self.has_dedicated_sink
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
}
