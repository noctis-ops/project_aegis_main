//! Main Execution Engine implementation

use crate::core::{TradeIntent, Order, ExecutionError};
use crate::components::{
    SmartOrderRouter, 
    OrderLifecycleManager, 
    StateReconciliationEngine, 
    MarginLeverageGuard
};
use crossbeam::channel::{Receiver, Sender};
use tracing::{info, warn, error, debug};
use std::collections::HashMap;

/// Main Execution Engine
pub struct ExecutionEngine {
    symbols: Vec<String>,
    smart_order_router: SmartOrderRouter,
    order_lifecycle_manager: OrderLifecycleManager,
    state_reconciliation_engine: StateReconciliationEngine,
    margin_leverage_guard: MarginLeverageGuard,
    trade_intent_rx: Option<Receiver<TradeIntent>>,
    execution_report_tx: Option<Sender<crate::core::ExecutionReport>>,
}

impl ExecutionEngine {
    /// Create a new Execution Engine
    pub fn new() -> Self {
        Self {
            symbols: vec!["BTCUSDT".to_string()], // Default symbol
            smart_order_router: SmartOrderRouter::new(),
            order_lifecycle_manager: OrderLifecycleManager::new(),
            state_reconciliation_engine: StateReconciliationEngine::new(),
            margin_leverage_guard: MarginLeverageGuard::new(),
            trade_intent_rx: None,
            execution_report_tx: None,
        }
    }
    
    /// Add a symbol to track
    pub fn add_symbol(&mut self, symbol: String) {
        if !self.symbols.contains(&symbol) {
            self.symbols.push(symbol);
        }
    }
    
    /// Set trade intent receiver channel
    pub fn set_trade_intent_receiver(&mut self, rx: Receiver<TradeIntent>) {
        self.trade_intent_rx = Some(rx);
    }
    
    /// Set execution report sender channel
    pub fn set_execution_report_sender(&mut self, tx: Sender<crate::core::ExecutionReport>) {
        self.execution_report_tx = Some(tx);
    }
    
    /// Start the execution engine
    pub async fn start(&mut self) -> Result<(), ExecutionError> {
        info!("Initializing Execution Engine for symbols: {:?}", self.symbols);
        
        // Initialize Binance connections
        self.initialize_binance_connections().await?;
        
        // Start the main processing loop
        self.run_processing_loop().await?;
        
        Ok(())
    }
    
    /// Initialize Binance connections
    async fn initialize_binance_connections(&mut self) -> Result<(), ExecutionError> {
        info!("Initializing Binance connections");
        
        // Initialize user data stream
        self.state_reconciliation_engine.initialize_user_data_stream().await?;
        
        // Initialize order API connection
        self.smart_order_router.initialize_order_api().await?;
        
        info!("Binance connections initialized successfully");
        Ok(())
    }
    
    /// Main processing loop
    async fn run_processing_loop(&mut self) -> Result<(), ExecutionError> {
        info!("Starting main processing loop");
        
        // We need channels for communication
        if self.trade_intent_rx.is_none() {
            return Err(ExecutionError::Other("Trade intent receiver must be set".to_string()));
        }
        
        let trade_intent_rx = self.trade_intent_rx.as_ref().unwrap();
        
        loop {
            // Process trade intents from Layer 2
            if let Ok(intent) = trade_intent_rx.try_recv() {
                self.process_trade_intent(intent).await?;
            }
            
            // Process execution reports from Binance
            self.state_reconciliation_engine.process_user_data_stream().await?;
            
            // Check for order timeouts
            self.check_order_timeouts().await?;
            
            // Small delay to prevent busy looping
            tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;
        }
    }
    
    /// Process trade intent from Layer 2
    async fn process_trade_intent(&mut self, intent: TradeIntent) -> Result<(), ExecutionError> {
        debug!("Processing trade intent for symbol: {}", intent.symbol);
        
        // Validate order parameters
        if !self.margin_leverage_guard.validate_order_parameters(&intent) {
            error!("Invalid order parameters for symbol: {}", intent.symbol);
            return Ok(());
        }
        
        // Check rate limits
        if !self.smart_order_router.check_rate_limits() {
            warn!("Rate limit exceeded, skipping order for symbol: {}", intent.symbol);
            return Ok(());
        }
        
        // Route the order
        let order = Order::from_trade_intent(&intent);
        self.smart_order_router.route_order(order).await?;
        
        // Track order lifecycle
        self.order_lifecycle_manager.track_order(order);
        
        Ok(())
    }
    
    /// Check for order timeouts
    async fn check_order_timeouts(&mut self) -> Result<(), ExecutionError> {
        let timed_out_orders = self.order_lifecycle_manager.get_timed_out_orders();
        
        for order in timed_out_orders {
            warn!("Order timed out: {}", order.client_order_id);
            
            // Cancel timed out order
            self.smart_order_router.cancel_order(
                order.symbol.clone(), 
                order.client_order_id.clone()
            ).await?;
            
            // Notify about timeout
            if let Some(tx) = &self.execution_report_tx {
                // In a real implementation, we would send a synthetic execution report
                // for the cancellation
            }
        }
        
        Ok(())
    }
}