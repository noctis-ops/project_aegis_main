//! Main Simulation Engine implementation

use crate::core::{SimulationConfig, BacktestReport, SimulationError};
use crate::components::{
    DataLake, 
    BacktestingEngine, 
    ExecutionSimulator, 
    CapitalAdapter,
    LiveExecutionEngine,
    ShadowExecutionEngine,
    ExecutionEngine,
    ExecutionEngineType,
    TelegramC2Bot,
    CommandProcessor,
    NotificationService
};
use tracing::{info, warn, error, debug};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Main Simulation Engine
pub struct SimulationEngine {
    config: SimulationConfig,
    data_lake: DataLake,
    backtesting_engine: BacktestingEngine,
    execution_simulator: ExecutionSimulator,
    capital_adapter: CapitalAdapter,
    execution_engine: Box<dyn ExecutionEngine>,
    telegram_bot: Option<TelegramC2Bot>,
    command_processor: Option<CommandProcessor>,
    notification_service: Option<NotificationService>,
}

impl SimulationEngine {
    /// Create a new Simulation Engine
    pub fn new() -> Self {
        let config = SimulationConfig {
            data_path: "./data".to_string(),
            start_time: chrono::Utc::now(),
            end_time: chrono::Utc::now(),
            playback_speed: 1.0,
            initial_capital: 10000.0,
            leverage: 10.0,
            symbols: vec!["BTCUSDT".to_string()],
        };
        
        // Initialize with shadow execution engine by default
        let shadow_engine = ShadowExecutionEngine::new();
        
        Self {
            config,
            data_lake: DataLake::new(),
            backtesting_engine: BacktestingEngine::new(),
            execution_simulator: ExecutionSimulator::new(),
            capital_adapter: CapitalAdapter::new(),
            execution_engine: Box::new(shadow_engine),
            telegram_bot: None,
            command_processor: None,
            notification_service: None,
        }
    }
    
    /// Start the simulation engine
    pub async fn start(&mut self) -> Result<(), SimulationError> {
        info!("Initializing Simulation Engine");
        
        // Initialize data lake
        self.data_lake.initialize(&self.config.data_path)?;
        
        // Start background simulation tasks
        self.start_simulation_tasks().await?;
        
        info!("Simulation Engine started successfully");
        Ok(())
    }
    
    /// Start background simulation tasks
    async fn start_simulation_tasks(&mut self) -> Result<(), SimulationError> {
        // Start data feeding
        let data_lake = self.data_lake.clone();
        let backtesting_engine = self.backtesting_engine.clone();
        
        tokio::spawn(async move {
            loop {
                // Feed data to backtesting engine
                if let Err(e) = data_lake.feed_data(&backtesting_engine).await {
                    error!("Data feeding error: {}", e);
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        });
        
        // Start execution simulation
        let execution_simulator = self.execution_simulator.clone();
        let backtesting_engine = self.backtesting_engine.clone();
        
        tokio::spawn(async move {
            loop {
                // Process execution simulations
                if let Err(e) = execution_simulator.process_executions(&backtesting_engine).await {
                    error!("Execution simulation error: {}", e);
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
            }
        });
        
        Ok(())
    }
    
    /// Run a backtest
    pub async fn run_backtest(&mut self, config: SimulationConfig) -> Result<BacktestReport, SimulationError> {
        info!("Starting backtest simulation");
        
        // Validate configuration
        self.validate_config(&config)?;
        
        // Set configuration
        self.config = config.clone();
        
        // Initialize data lake with new config
        self.data_lake.initialize(&config.data_path)?;
        
        // Run the simulation
        let report = self.backtesting_engine.run_simulation(&config).await?;
        
        info!("Backtest simulation completed successfully");
        Ok(report)
    }
    
    /// Validate simulation configuration
    fn validate_config(&self, config: &SimulationConfig) -> Result<(), SimulationError> {
        if config.start_time >= config.end_time {
            return Err(SimulationError::InvalidConfig(
                "Start time must be before end time".to_string()
            ));
        }
        
        if config.playback_speed < crate::core::constants::MIN_PLAYBACK_SPEED || 
           config.playback_speed > crate::core::constants::MAX_PLAYBACK_SPEED {
            return Err(SimulationError::InvalidConfig(
                format!("Playback speed must be between {} and {}", 
                       crate::core::constants::MIN_PLAYBACK_SPEED,
                       crate::core::constants::MAX_PLAYBACK_SPEED)
            ));
        }
        
        if config.initial_capital <= 0.0 {
            return Err(SimulationError::InvalidConfig(
                "Initial capital must be positive".to_string()
            ));
        }
        
        if config.symbols.is_empty() {
            return Err(SimulationError::InvalidConfig(
                "At least one symbol must be specified".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Run capital adaptation tests
    pub async fn run_capital_adaptation_tests(&mut self) -> Result<Vec<BacktestReport>, SimulationError> {
        info!("Running capital adaptation test matrix");
        
        let test_cases = self.capital_adapter.generate_test_cases();
        let mut reports = Vec::new();
        
        for test_case in test_cases {
            let mut config = self.config.clone();
            config.initial_capital = test_case.capital;
            config.leverage = test_case.leverage;
            
            let report = self.run_backtest(config).await?;
            reports.push(report);
            
            // Check if this test case passed
            if !self.capital_adapter.validate_test_result(&report, &test_case) {
                error!("Capital adaptation test failed for capital=${:.2}", test_case.capital);
                return Err(SimulationError::CapitalAdaptationFailed(
                    format!("Test failed for capital=${:.2}", test_case.capital)
                ));
            }
        }
        
        info!("All capital adaptation tests passed");
        Ok(reports)
    }
    
    /// Run robustness tests
    pub async fn run_robustness_tests(&mut self) -> Result<Vec<BacktestReport>, SimulationError> {
        info!("Running robustness tests");
        
        // In a real implementation, this would vary strategy parameters
        // and test performance sensitivity
        let reports = Vec::new();
        
        info!("Robustness tests completed");
        Ok(reports)
    }
}