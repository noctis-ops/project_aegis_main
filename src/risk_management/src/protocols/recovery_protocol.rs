//! Recovery and resumption protocol implementation

use crate::core::{RecoveryState, PortfolioState};
// Imported from their defining modules, not through `protocols`' own glob
// re-export: the re-export is a convenience for external callers and would make
// this module depend on its own parent's export list.
use crate::protocols::health_check::HealthChecker;
use crate::protocols::recalibration::Recalibrator;
use tracing::{info, warn, error};
use std::sync::{Arc, RwLock};
use std::time::{Instant, Duration};

/// Recovery Protocol
pub struct RecoveryProtocol {
    state: Arc<RwLock<RecoveryState>>,
    cooldown_start: Arc<RwLock<Option<Instant>>>,
    health_checker: HealthChecker,
    recalibrator: Recalibrator,
}

impl RecoveryProtocol {
    /// Create a new recovery protocol
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RecoveryState::Normal)),
            cooldown_start: Arc::new(RwLock::new(None)),
            health_checker: HealthChecker::new(),
            recalibrator: Recalibrator::new(),
        }
    }
    
    /// Initiate recovery protocol
    pub fn initiate_recovery(&self) {
        let mut state = self.state.write().unwrap();
        *state = RecoveryState::Cooldown;
        
        let mut cooldown_start = self.cooldown_start.write().unwrap();
        *cooldown_start = Some(Instant::now());
        
        info!("RECOVERY_PROTOCOL: Initiating cooldown period for {} seconds", 
              crate::core::constants::COOLDOWN_PERIOD_SECONDS);
        
        // Trigger emergency procedures
        self.trigger_emergency_procedures();
    }
    
    /// Trigger emergency procedures
    fn trigger_emergency_procedures(&self) {
        error!("EMERGENCY_PROCEDURES: Activating full system shutdown");
        // In a real implementation, this would:
        // 1. Send emergency cancel all orders to Layer 3
        // 2. Send flatten all positions command
        // 3. Shut down trading activities
        // 4. Send alerts to engineers
    }
    
    /// Check if cooldown period is over
    pub fn is_cooldown_over(&self) -> bool {
        let state = self.state.read().unwrap();
        if *state != RecoveryState::Cooldown {
            return false;
        }
        
        let cooldown_start = self.cooldown_start.read().unwrap();
        if let Some(start) = *cooldown_start {
            start.elapsed() >= Duration::from_secs(crate::core::constants::COOLDOWN_PERIOD_SECONDS)
        } else {
            false
        }
    }
    
    /// Transition to cautious resumption
    pub fn transition_to_cautious_resumption(&self) {
        let mut state = self.state.write().unwrap();
        *state = RecoveryState::CautiousResumption { successful_trades: 0 };
        
        info!("RECOVERY_PROTOCOL: Transitioning to cautious resumption mode");
        
        // Reset risk management to cautious levels
        self.recalibrator.set_cautious_risk_levels();
    }
    
    /// Record successful trade during recovery
    pub fn record_successful_trade_during_recovery(&self) {
        let mut state = self.state.write().unwrap();
        
        if let RecoveryState::CautiousResumption { ref mut successful_trades } = *state {
            *successful_trades += 1;
            info!("RECOVERY_PROTOCOL: Successful trade recorded ({}/{})", 
                  *successful_trades, crate::core::constants::SUCCESSFUL_TRADES_FOR_FULL_RECOVERY);
                  
            // Check if we can transition to full operation
            if *successful_trades >= crate::core::constants::SUCCESSFUL_TRADES_FOR_FULL_RECOVERY {
                self.transition_to_normal_operation();
            }
        }
    }
    
    /// Transition to normal operation
    pub fn transition_to_normal_operation(&self) {
        let mut state = self.state.write().unwrap();
        *state = RecoveryState::Normal;
        
        info!("RECOVERY_PROTOCOL: Transitioning to normal operation");
        
        // Reset risk management to normal levels
        self.recalibrator.reset_to_normal_risk_levels();
    }
    
    /// Perform health check during recovery
    pub fn perform_health_check(&self, portfolio_state: &PortfolioState) -> bool {
        let healthy = self.health_checker.perform_comprehensive_check(portfolio_state);
        
        // A failed check gates the whole resumption ladder, and nothing else in the
        // system notices: the caller only receives a bool, so this is where the
        // reason has to reach the operator trail.
        if !healthy {
            warn!(
                "RECOVERY_PROTOCOL: health check FAILED {}s after the previous one; \
                 resumption stays gated and the system remains in {:?}",
                self.health_checker.time_since_last_check().as_secs(),
                self.get_recovery_state()
            );
        }
        
        healthy
    }
    
    /// Get current recovery state
    pub fn get_recovery_state(&self) -> RecoveryState {
        let state = self.state.read().unwrap();
        state.clone()
    }
    
    /// Check if system is ready for trading
    pub fn is_ready_for_trading(&self) -> bool {
        let state = self.state.read().unwrap();
        match *state {
            RecoveryState::Normal => true,
            RecoveryState::CautiousResumption { .. } => true,
            RecoveryState::Cooldown => false,
        }
    }
}
