//! Main Simulation Engine implementation

use crate::core::{BacktestReport, SimulationConfig, SimulationError, TradeIntent};
use crate::components::{
    BacktestingEngine,
    CapitalAdapter,
    DataLake,
    ExecutionEngine,
    ExecutionEngineType,
    ExecutionSimulator,
    ShadowExecutionEngine,
};
use tracing::{error, info};

/// Main Simulation Engine
pub struct SimulationEngine {
    config: SimulationConfig,
    data_lake: DataLake,
    backtesting_engine: BacktestingEngine,
    execution_simulator: ExecutionSimulator,
    capital_adapter: CapitalAdapter,
    execution_engine: Box<dyn ExecutionEngine>,
}

impl SimulationEngine {
    /// Create a new Simulation Engine
    pub fn new() -> Self {
        // The defaults live in one place (`SimulationConfig::default`), so the strategy
        // knobs and fee schedule the replay needs cannot be missing from a constructed
        // engine while present in a configured one.
        let config = SimulationConfig {
            initial_capital: 10_000.0,
            leverage: 10.0,
            symbols: vec!["BTCUSDT".to_string()],
            ..SimulationConfig::default()
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
        }
    }
    
    /// Replace the execution backend.
    ///
    /// This is the seam a live [`crate::components::LiveExecutionEngine`] (or the
    /// Telegram `/mode` command, once it is wired) is armed through. It is
    /// deliberately never called from `new()`: a live backend submits real orders,
    /// so switching to it has to be an explicit action.
    pub fn set_execution_engine(&mut self, execution_engine: Box<dyn ExecutionEngine>) {
        info!(
            "Execution backend switched from {:?} to {:?}",
            self.execution_engine.engine_type(),
            execution_engine.engine_type()
        );
        self.execution_engine = execution_engine;
    }
    
    /// The execution backend currently armed (shadow unless a live engine was set).
    pub fn execution_mode(&self) -> ExecutionEngineType {
        self.execution_engine.engine_type()
    }
    
    /// Route a trade intent through the active execution backend.
    pub async fn execute_trade(&self, intent: TradeIntent) -> Result<(), SimulationError> {
        self.execution_engine.execute_trade(intent).await
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
    ///
    /// A second task used to call `DataLake::feed_data` every 10 ms. That was harmless
    /// while the lake manufactured one event, and would be disk thrash plus duplicated
    /// events now that it reads real files: ingestion is an explicit step
    /// ([`SimulationEngine::ingest_lake_events`]), and a replay loads its own window.
    async fn start_simulation_tasks(&mut self) -> Result<(), SimulationError> {
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
    
    /// Run a backtest over the recorded data for this configuration.
    ///
    /// The events are loaded once and passed to the engine as a fixed sequence, so two
    /// runs over the same directory and window produce the same report - the property
    /// the deterministic path exists to give the strategy gates above it.
    pub async fn run_backtest(&mut self, config: SimulationConfig) -> Result<BacktestReport, SimulationError> {
        info!("Starting backtest simulation");
        
        // Validate configuration
        self.validate_config(&config)?;
        
        // The data is read before the configuration is adopted: a run that failed for
        // want of data must not leave the engine reporting the window it never replayed.
        self.data_lake.initialize(&config.data_path)?;
        let events = self.data_lake.collect_events(&config)?;
        self.config = config.clone();
        
        let report = self.backtesting_engine.run_simulation(&config, &events).await?;
        
        info!(
            "Backtest replay measured {:+.2}% over {} fill(s), drawdown {:.2}%, execution quality {:.1}%",
            report.results.total_return * 100.0,
            report.results.total_trades,
            report.results.max_drawdown * 100.0,
            report.results.execution_quality * 100.0
        );
        Ok(report)
    }
    
    /// Push everything the lake holds into the run that is currently active.
    ///
    /// For a shadow run driven from outside a replay; a backtest does not need it,
    /// because `run_backtest` loads its own window. Events that the active run rejects
    /// (past its window, out of order) are counted out of the delivered total.
    pub async fn ingest_lake_events(&self) -> Result<usize, SimulationError> {
        let delivered = self.data_lake.feed_data(&self.backtesting_engine).await?;
        info!("Ingested {} event(s) from the data lake", delivered);
        Ok(delivered)
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
            
            // Validate while `report` is still owned, then keep it for the return
            // value: pushing first and reading afterwards was a use-after-move.
            if !self.capital_adapter.validate_test_result(&report, &test_case) {
                error!("Capital adaptation test failed for capital=${:.2}", test_case.capital);
                return Err(SimulationError::CapitalAdaptationFailed(
                    format!("Test failed for capital=${:.2}", test_case.capital)
                ));
            }
            
            reports.push(report);
        }
        
        info!("All capital adaptation tests passed");
        Ok(reports)
    }
    
    /// Run robustness tests by perturbing one strategy parameter at a time.
    ///
    /// One knob at a time is the point: a configuration that only works when several
    /// parameters move together is not robust, it is fitted to a single grid point.
    /// Every variant replays the *same* events, so the spread across the reports is
    /// attributable to the parameter rather than to the feed.
    pub async fn run_robustness_tests(&mut self) -> Result<Vec<BacktestReport>, SimulationError> {
        info!("Running robustness tests");
        
        let params = self.capital_adapter.generate_robustness_params();
        if params.parameter_ranges.is_empty() || params.test_points == 0 {
            return Err(SimulationError::InvalidConfig(
                "Robustness testing needs at least one parameter range and one test point".to_string(),
            ));
        }
        
        self.data_lake.initialize(&self.config.data_path)?;
        let events = self.data_lake.collect_events(&self.config)?;
        
        let mut reports = Vec::new();
        for (name, min_value, max_value) in &params.parameter_ranges {
            let step = (max_value - min_value) / params.test_points as f64;
            
            for index in 0..params.test_points {
                let mut config = self.config.clone();
                // One table of swept names, shared with the validation pipeline
                // (`StrategyParams::set_knob`), so the two cannot disagree about what
                // the simulation is able to apply.
                config.strategy.set_knob(name, min_value + index as f64 * step)?;
                reports.push(self.backtesting_engine.run_simulation(&config, &events).await?);
            }
        }
        
        // The gate is called, not just defined: an empty or decorative sweep is what
        // this function used to be.
        if !self.capital_adapter.validate_robustness_results(&reports) {
            return Err(SimulationError::RobustnessTestFailed(format!(
                "{} replay(s) across the swept ranges did not hold up together",
                reports.len()
            )));
        }
        
        info!("Robustness tests completed with {} test cases", reports.len());
        Ok(reports)
    }
    
}
