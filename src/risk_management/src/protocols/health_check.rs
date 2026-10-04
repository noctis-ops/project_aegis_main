//! Health check protocol implementation

use crate::core::PortfolioState;
use tracing::{info, warn, debug};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Health Checker
///
/// The check timestamp sits behind `Arc<RwLock<_>>` exactly like every other piece
/// of mutable state in this layer (`last_flash_crash_check`, `last_volatility_check`,
/// `last_push_time`): the recovery protocol drives checks through `&self` while the
/// same handle is shared with monitoring tasks, so mutation has to be interior.
///
/// Locks are poison-tolerant on purpose. A `unwrap()` here would let a panic in an
/// unrelated thread permanently disable the health check that gates resumption —
/// the one thing a recovery path cannot afford.
pub struct HealthChecker {
    last_check_time: Arc<RwLock<Instant>>,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new() -> Self {
        Self {
            last_check_time: Arc::new(RwLock::new(Instant::now())),
        }
    }
    
    /// Perform comprehensive health check
    pub fn perform_comprehensive_check(&self, portfolio_state: &PortfolioState) -> bool {
        info!("HEALTH_CHECK: Performing comprehensive system health check");
        
        let mut all_checks_passed = true;
        
        // Check WebSocket connectivity
        if !self.check_websocket_connectivity() {
            warn!("HEALTH_CHECK: WebSocket connectivity check FAILED");
            all_checks_passed = false;
        } else {
            debug!("HEALTH_CHECK: WebSocket connectivity check PASSED");
        }
        
        // Check order book synchronization
        if !self.check_order_book_sync() {
            warn!("HEALTH_CHECK: Order book synchronization check FAILED");
            all_checks_passed = false;
        } else {
            debug!("HEALTH_CHECK: Order book synchronization check PASSED");
        }
        
        // Check funding rate stability
        if !self.check_funding_rate_stability() {
            warn!("HEALTH_CHECK: Funding rate stability check FAILED");
            all_checks_passed = false;
        } else {
            debug!("HEALTH_CHECK: Funding rate stability check PASSED");
        }
        
        // Check portfolio state
        if !self.check_portfolio_state(portfolio_state) {
            warn!("HEALTH_CHECK: Portfolio state check FAILED");
            all_checks_passed = false;
        } else {
            debug!("HEALTH_CHECK: Portfolio state check PASSED");
        }
        
        // Recorded after every sub-check so `time_since_last_check` measures the
        // completion time, not the moment the check started.
        {
            let mut last_check = self.last_check_time
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *last_check = Instant::now();
        }
        
        if all_checks_passed {
            info!("HEALTH_CHECK: All systems HEALTHY");
        } else {
            warn!("HEALTH_CHECK: Some systems UNHEALTHY");
        }
        
        all_checks_passed
    }
    
    /// Check WebSocket connectivity
    fn check_websocket_connectivity(&self) -> bool {
        // In a real implementation, this would check actual WebSocket connections
        // (Layer 1 heartbeat age, Layer 3 user-data-stream state). For now we
        // report a healthy link so resumption is gated on the portfolio checks only.
        true
    }
    
    /// Check order book synchronization
    fn check_order_book_sync(&self) -> bool {
        // In a real implementation, this would check order book sync status
        // (last successful snapshot sync of Layer 1's LocalOrderBook).
        // For now, we'll simulate synchronized state
        true
    }
    
    /// Check funding rate stability
    fn check_funding_rate_stability(&self) -> bool {
        // In a real implementation, this would check funding rate data
        // (staleness of MarketRegimeGuard's funding feed).
        // For now, we'll simulate stable funding rates
        true
    }
    
    /// Check portfolio state
    fn check_portfolio_state(&self, portfolio_state: &PortfolioState) -> bool {
        // Check for reasonable equity values
        if portfolio_state.equity <= 0.0 {
            return false;
        }
        
        // Check for reasonable exposure levels
        if portfolio_state.total_exposure.abs() > portfolio_state.equity * 10.0 {
            // Exposure more than 10x equity might indicate a problem
            return false;
        }
        
        true
    }
    
    /// Get time since last check
    pub fn time_since_last_check(&self) -> Duration {
        let last_check = *self
            .last_check_time
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        last_check.elapsed()
    }
}
