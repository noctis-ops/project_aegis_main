//! Main Risk Management System implementation

use crate::core::{PortfolioState, TradeRecord, CircuitBreakerStatus, RecoveryState, RiskConfig};
use crate::components::{
    GlobalRiskManager, 
    CircuitBreakerSystem, 
    MarketRegimeGuard, 
    PerformanceAnalyzer
};
use tracing::{info, warn, error, debug};
use std::sync::Arc;
use dashmap::DashMap;

/// Main Risk Management System
pub struct RiskManagementSystem {
    config: RiskConfig,
    global_risk_manager: GlobalRiskManager,
    circuit_breaker_system: CircuitBreakerSystem,
    market_regime_guard: MarketRegimeGuard,
    performance_analyzer: PerformanceAnalyzer,
    portfolio_state: Arc<DashMap<String, PortfolioState>>,
    trade_records: Arc<DashMap<String, TradeRecord>>,
    circuit_breaker_status: Arc<std::sync::RwLock<CircuitBreakerStatus>>,
    recovery_state: Arc<std::sync::RwLock<RecoveryState>>,
}

impl RiskManagementSystem {
    /// Create a new Risk Management System
    pub fn new() -> Self {
        let config = RiskConfig::default();
        
        // Built from the same configuration the risk manager consumes below, and
        // before `config` moves into it (struct-literal fields are evaluated in
        // declaration order): the drawdown breakers have to honour the configured
        // limits, not the constants they used to hard-code.
        let circuit_breaker_system = CircuitBreakerSystem::with_config(&config);
        
        Self {
            config: config.clone(),
            global_risk_manager: GlobalRiskManager::new(config),
            circuit_breaker_system,
            market_regime_guard: MarketRegimeGuard::new(),
            performance_analyzer: PerformanceAnalyzer::new(),
            portfolio_state: Arc::new(DashMap::new()),
            trade_records: Arc::new(DashMap::new()),
            circuit_breaker_status: Arc::new(std::sync::RwLock::new(CircuitBreakerStatus::Normal)),
            recovery_state: Arc::new(std::sync::RwLock::new(RecoveryState::Normal)),
        }
    }
    
    /// Start the risk management system
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing Risk Management System");
        
        // Start background monitoring tasks
        self.start_monitoring_tasks().await?;
        
        info!("Risk Management System started successfully");
        Ok(())
    }
    
    /// Start background monitoring tasks
    async fn start_monitoring_tasks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Start portfolio monitoring
        let portfolio_state = self.portfolio_state.clone();
        let circuit_breaker_system = self.circuit_breaker_system.clone();
        let market_regime_guard = self.market_regime_guard.clone();
        
        tokio::spawn(async move {
            loop {
                // Monitor portfolio state
                for entry in portfolio_state.iter() {
                    let state = entry.value();
                    circuit_breaker_system.evaluate_portfolio_state(state);
                    market_regime_guard.evaluate_market_conditions(state);
                }
                
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        });
        
        // Start performance monitoring
        let trade_records = self.trade_records.clone();
        let performance_analyzer = self.performance_analyzer.clone();
        let circuit_breaker_system = self.circuit_breaker_system.clone();
        
        tokio::spawn(async move {
            loop {
                // Analyze performance metrics
                let metrics = performance_analyzer.calculate_metrics(&trade_records);
                circuit_breaker_system.evaluate_performance_metrics(&metrics);
                
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
        
        // Start recovery state monitoring
        let recovery_state = self.recovery_state.clone();
        let circuit_breaker_status = self.circuit_breaker_status.clone();
        
        tokio::spawn(async move {
            loop {
                // Check recovery state transitions
                Self::check_recovery_transitions(&recovery_state, &circuit_breaker_status).await;
                
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            }
        });
        
        Ok(())
    }
    
    /// Check recovery state transitions
    async fn check_recovery_transitions(
        recovery_state: &Arc<std::sync::RwLock<RecoveryState>>,
        circuit_breaker_status: &Arc<std::sync::RwLock<CircuitBreakerStatus>>
    ) {
        let current_status = {
            let status = circuit_breaker_status
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            status.clone()
        };
        
        match current_status {
            CircuitBreakerStatus::KillSwitch => {
                // Enter cooldown. Checked and written under the same lock, and only
                // logged on the transition: this task ticks every 10s while the
                // breaker stays tripped, so a repeated "cooldown started" line
                // buries the reason that actually mattered.
                let mut recovery = recovery_state
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                
                if *recovery != RecoveryState::Cooldown {
                    *recovery = RecoveryState::Cooldown;
                    warn!("RECOVERY: Kill switch tripped - trading halted, cooldown engaged");
                }
            }
            _ => {
                // Reading the cooldown clock and stepping into cautious resumption
                // belongs to RecoveryProtocol, which owns the cooldown start instant,
                // the 15-minute window and the successful-trade ladder. This task
                // holds neither, so it deliberately does not release the halt: an
                // engine that stays halted on a timer it cannot verify is fail-safe,
                // one that resumes on a guess is not.
                let recovery = recovery_state
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                
                if let RecoveryState::Cooldown = *recovery {
                    debug!("RECOVERY: still in cooldown; awaiting RecoveryProtocol resumption");
                }
            }
        }
    }
    
    /// Update portfolio state
    pub fn update_portfolio_state(&self, currency: String, state: PortfolioState) {
        self.portfolio_state.insert(currency, state);
    }
    
    /// Record trade execution
    pub fn record_trade(&self, trade_id: String, record: TradeRecord) {
        // Feed the analyzer from a borrow first, then move the record into the book
        // of record. Cloning it to satisfy both would allocate on every fill for a
        // copy the analyzer no longer keeps (it only counts trades).
        self.performance_analyzer.update_with_trade(&record);
        self.trade_records.insert(trade_id, record);
    }
    
    /// The configuration this system enforces.
    ///
    /// Exposed for the dashboard and the Telegram C2: an operator adjusting risk has
    /// to be able to read the limits currently in force, otherwise the numbers on
    /// the panel and the numbers tripping the breakers can drift apart unnoticed.
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }
    
    /// Get current risk percentage
    pub fn get_current_risk_percentage(&self) -> f64 {
        self.global_risk_manager.get_current_risk_percentage()
    }
    
    /// Get circuit breaker status
    pub fn get_circuit_breaker_status(&self) -> CircuitBreakerStatus {
        let status = self.circuit_breaker_status.read().unwrap();
        status.clone()
    }
    
    /// Trigger kill switch
    pub fn trigger_kill_switch(&self) {
        let mut status = self.circuit_breaker_status.write().unwrap();
        *status = CircuitBreakerStatus::KillSwitch;
        
        error!("KILL SWITCH ACTIVATED - Emergency shutdown initiated");
        
        // In a real implementation, this would:
        // 1. Send emergency cancel all orders command to Layer 3
        // 2. Send flatten all positions command
        // 3. Shut down trading activities
        // 4. Send alerts to engineers
    }
    
    /// Check if trading is allowed
    pub fn is_trading_allowed(&self) -> bool {
        let status = self.circuit_breaker_status.read().unwrap();
        
        match *status {
            CircuitBreakerStatus::Normal | CircuitBreakerStatus::Warning => true,
            CircuitBreakerStatus::HaltEntries | CircuitBreakerStatus::KillSwitch => false,
        }
    }
    
    /// Get recovery state
    pub fn get_recovery_state(&self) -> RecoveryState {
        let state = self.recovery_state.read().unwrap();
        state.clone()
    }
}
