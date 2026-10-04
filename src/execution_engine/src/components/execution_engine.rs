//! Main Execution Engine implementation

use crate::core::{
    TradeIntent, Order, OrderSide, OrderType, TimeInForce, ExecutionReport, ExecutionError,
};
use crate::components::{
    SmartOrderRouter, 
    OrderLifecycleManager, 
    StateReconciliationEngine, 
    MarginLeverageGuard
};
use crossbeam::channel::{Receiver, Sender};
use tracing::{info, warn, error, debug};
use crate::core::constants::*;

/// Main Execution Engine
pub struct ExecutionEngine {
    symbols: Vec<String>,
    smart_order_router: SmartOrderRouter,
    order_lifecycle_manager: OrderLifecycleManager,
    state_reconciliation_engine: StateReconciliationEngine,
    margin_leverage_guard: MarginLeverageGuard,
    trade_intent_rx: Option<Receiver<TradeIntent>>,
    execution_report_tx: Option<Sender<ExecutionReport>>,
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
    
    /// Set trade intent receiver channel (integration seam with Layer 2)
    pub fn set_trade_intent_receiver(&mut self, rx: Receiver<TradeIntent>) {
        self.trade_intent_rx = Some(rx);
    }
    
    /// Set execution report sender channel (integration seam with Layer 2/4)
    pub fn set_execution_report_sender(&mut self, tx: Sender<ExecutionReport>) {
        self.execution_report_tx = Some(tx);
    }
    
    /// Start the execution engine.
    ///
    /// Drives the processing loop for the whole lifetime of the process; it
    /// only returns when a fatal error occurs (e.g. a cancel that cannot be
    /// delivered — an intentional fail-stop for an order-management system).
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
        
        if self.trade_intent_rx.is_none() {
            return Err(ExecutionError::TradeIntentReceiverNotSet);
        }
        // Take ownership of the channel: this loop is its terminal consumer
        // (a single consumer per crossbeam queue, as designed).
        let trade_intent_rx = self.trade_intent_rx.take().unwrap();
        
        // Bridge the synchronous crossbeam channel into the async runtime:
        // a dedicated blocking thread parks on recv() (zero CPU while idle,
        // immediate wake-up) and forwards intents into a tokio channel that
        // the select! below can await. This replaces the old 100k/sec
        // try_recv busy-poll.
        let (intent_tx, mut intent_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::task::spawn_blocking(move || {
            for intent in trade_intent_rx {
                if intent_tx.send(intent).is_err() {
                    break; // Engine dropped its side: stop bridging.
                }
            }
        });
        
        let mut timeout_ticker = tokio::time::interval(std::time::Duration::from_millis(
            ORDER_TIMEOUT_CHECK_INTERVAL_MS,
        ));
        // Track whether the intent stream is still connected. Once Layer 2
        // disconnects we keep running to manage open orders (timeout
        // sweeps, cancels) but stop selecting on the closed channel.
        let mut intents_connected = true;
        
        loop {
            tokio::select! {
                intent = intent_rx.recv(), if intents_connected => {
                    match intent {
                        Some(intent) => self.process_trade_intent(intent).await?,
                        None => {
                            warn!("Trade intent channel disconnected; continuing to manage open orders");
                            intents_connected = false;
                        }
                    }
                }
                _ = timeout_ticker.tick() => {
                    // Process execution reports from Binance
                    self.state_reconciliation_engine.process_user_data_stream().await?;
                    
                    // Check for order timeouts
                    self.check_order_timeouts().await?;
                }
            }
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
        
        // Route the order. The router only reads the order, so it borrows it
        // and the same Order instance is tracked afterwards — no clone on
        // the hot path.
        let order = Order::from_trade_intent(&intent);
        match self.smart_order_router.route_order(&order).await {
            Ok(()) => {
                // Track order lifecycle
                self.order_lifecycle_manager.track_order(order);
            }
            Err(ExecutionError::OrderRejected(reason)) | Err(ExecutionError::InsufficientMargin(reason)) => {
                // Routine market-condition outcomes for post-only entries
                // (GTX entries are rejected whenever they would cross the
                // book). The order never reached the exchange, so it is not
                // tracked and the engine keeps running.
                warn!("Order {} not routed: {}", order.client_order_id, reason);
            }
            Err(e) => {
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    /// Check for order timeouts
    async fn check_order_timeouts(&mut self) -> Result<(), ExecutionError> {
        let timed_out_orders = self.order_lifecycle_manager.get_timed_out_orders();
        
        for order in timed_out_orders {
            warn!("Order timed out: {}", order.client_order_id);
            
            // Cancel timed out order. Failures propagate on purpose
            // (fail-stop): an execution engine that cannot manage its open
            // orders must not keep trading blind.
            //
            // One case is exempt: Binance -2011 "Unknown order sent" means the
            // order already reached a terminal state on the exchange (filled,
            // expired or canceled), which is exactly what this sweep wants. The
            // real state arrives on the user data stream, so the remaining
            // timed-out orders keep being swept instead of aborting the engine.
            if let Err(err) = self.smart_order_router.cancel_order(
                order.symbol.clone(), 
                order.client_order_id.clone()
            ).await {
                match err {
                    ExecutionError::OrderNotFound(reason) => {
                        warn!("Cancel of {} was a no-op: {}", order.client_order_id, reason);
                    }
                    other => return Err(other),
                }
            }
            
            // Stop tracking the order. Without this it would remain
            // "active" and be re-canceled on every timeout sweep forever.
            self.order_lifecycle_manager.mark_order_canceled(&order.client_order_id);
            
            // Publish a synthetic execution report so downstream layers
            // (risk management / telemetry) learn about the timeout.
            if let Some(tx) = &self.execution_report_tx {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                
                let report = ExecutionReport {
                    event_type: "executionReport".to_string(),
                    event_time: now,
                    symbol: order.symbol.clone(),
                    client_order_id: order.client_order_id.clone(),
                    side: match order.side {
                        OrderSide::Buy => "BUY".to_string(),
                        OrderSide::Sell => "SELL".to_string(),
                    },
                    order_type: match order.order_type {
                        OrderType::Limit => "LIMIT".to_string(),
                        OrderType::Market => "MARKET".to_string(),
                        OrderType::StopMarket => "STOP_MARKET".to_string(),
                        OrderType::TakeProfitMarket => "TAKE_PROFIT_MARKET".to_string(),
                    },
                    time_in_force: match order.time_in_force {
                        TimeInForce::GTC => "GTC".to_string(),
                        TimeInForce::IOC => "IOC".to_string(),
                        TimeInForce::FOK => "FOK".to_string(),
                        TimeInForce::GTX => "GTX".to_string(),
                    },
                    original_quantity: order.quantity.to_string(),
                    original_price: order.price.to_string(),
                    average_price: order.avg_price.to_string(),
                    stop_price: "0".to_string(),
                    execution_type: "CANCELED".to_string(),
                    order_status: "EXPIRED".to_string(),
                    order_id: 0, // Synthetic report: no exchange order ID
                    last_executed_quantity: "0".to_string(),
                    cumulative_filled_quantity: order.filled_quantity.to_string(),
                    last_executed_price: "0".to_string(),
                    commission: "0".to_string(),
                    commission_asset: "USDT".to_string(),
                    transaction_time: now,
                    trade_id: 0,
                };
                
                if let Err(e) = tx.send(report) {
                    debug!("Execution report channel closed: {}", e);
                }
            }
        }
        
        Ok(())
    }
}
