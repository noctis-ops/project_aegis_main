//! Main trading engine that coordinates all components

use crate::core::{HftError, MarketDataEvent, OrderBookUpdate};
use crate::network::{WebSocketManager, RestClient, HeartbeatMonitor};
use crate::orderbook::{LocalOrderBook, SyncProtocol};
use crate::engine::EventBus;
use tokio::sync::mpsc;
use tracing::{info, error, warn};
use std::collections::HashMap;
use crate::core::constants::*;

/// Main trading engine
pub struct TradingEngine {
    symbols: Vec<String>,
    order_books: HashMap<String, LocalOrderBook>,
    event_bus: EventBus,
    sync_protocols: HashMap<String, SyncProtocol>,
    rest_client: RestClient,
    market_data_tx: mpsc::UnboundedSender<MarketDataEvent>,
    market_data_rx: mpsc::UnboundedReceiver<MarketDataEvent>,
    heartbeat_monitors: HashMap<String, HeartbeatMonitor>,
}

impl TradingEngine {
    /// Create a new trading engine
    pub fn new() -> Self {
        let (market_data_tx, market_data_rx) = mpsc::unbounded_channel();
        let rest_client = RestClient::new().expect("Failed to create REST client");
        
        Self {
            symbols: vec!["BTCUSDT".to_string()], // Default symbol for testing
            order_books: HashMap::new(),
            event_bus: EventBus::new(),
            sync_protocols: HashMap::new(),
            rest_client,
            market_data_tx,
            market_data_rx,
            heartbeat_monitors: HashMap::new(),
        }
    }
    
    /// Add a symbol to track
    pub fn add_symbol(&mut self, symbol: String) {
        if !self.symbols.contains(&symbol) {
            self.symbols.push(symbol.clone());
            self.order_books.insert(symbol.clone(), LocalOrderBook::new(symbol.clone()));
            self.sync_protocols.insert(
                symbol.clone(), 
                SyncProtocol::new().expect("Failed to create sync protocol")
            );
        }
    }
    
    /// Start the trading engine
    pub async fn start(&mut self) -> Result<(), HftError> {
        info!("Initializing trading engine for symbols: {:?}", self.symbols);
        
        // Initialize components for each symbol
        for symbol in &self.symbols {
            self.initialize_symbol(symbol).await?;
        }
        
        // Start the main event loop
        self.run_event_loop().await?;
        
        Ok(())
    }
    
    /// Initialize components for a symbol
    async fn initialize_symbol(&mut self, symbol: &str) -> Result<(), HftError> {
        info!("Initializing components for symbol: {}", symbol);
        
        // Create order book
        self.order_books.insert(symbol.to_string(), LocalOrderBook::new(symbol.to_string()));
        
        // Create sync protocol
        self.sync_protocols.insert(
            symbol.to_string(), 
            SyncProtocol::new()?
        );
        
        // Create WebSocket manager
        let ws_manager = WebSocketManager::new(symbol.to_string(), self.market_data_tx.clone());
        
        // Start WebSocket connection
        ws_manager.start().await?;
        
        // Create heartbeat monitor
        let heartbeat_monitor = HeartbeatMonitor::new(self.market_data_tx.clone());
        self.heartbeat_monitors.insert(symbol.to_string(), heartbeat_monitor);
        
        info!("Components initialized for symbol: {}", symbol);
        Ok(())
    }
    
    /// Main event loop
    async fn run_event_loop(&mut self) -> Result<(), HftError> {
        info!("Starting main event loop");
        
        loop {
            // Process market data events
            if let Ok(event) = self.market_data_rx.try_recv() {
                self.process_market_data_event(event).await?;
            }
            
            // Process events from the internal bus
            if let Ok(event) = self.event_bus.receive() {
                self.process_internal_event(event).await?;
            }
            
            // Small delay to prevent busy looping
            tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;
        }
    }
    
    /// Process market data events from WebSocket
    async fn process_market_data_event(&mut self, event: MarketDataEvent) -> Result<(), HftError> {
        match event {
            MarketDataEvent::OrderBookUpdate(update) => {
                self.handle_order_book_update(update).await?;
            }
            MarketDataEvent::AggTrade(_trade) => {
                // Process aggregated trades
            }
            MarketDataEvent::MarkPriceUpdate(_mark_price) => {
                // Process mark price updates
            }
            MarketDataEvent::ForceOrder(_force_order) => {
                // Process force orders (liquidations)
            }
            MarketDataEvent::MarketDataHalt => {
                self.handle_market_data_halt().await?;
            }
        }
        
        Ok(())
    }
    
    /// Process internal events from the event bus
    async fn process_internal_event(&mut self, event: MarketDataEvent) -> Result<(), HftError> {
        // Forward events to appropriate handlers
        self.process_market_data_event(event).await
    }
    
    /// Handle order book updates
    async fn handle_order_book_update(&mut self, update: OrderBookUpdate) -> Result<(), HftError> {
        let symbol = update.symbol.clone();
        
        // Update heartbeat
        if let Some(heartbeat_monitor) = self.heartbeat_monitors.get_mut(&symbol) {
            // Note: In a real implementation, we would need a mutable reference to update heartbeat
        }
        
        // Get order book for this symbol
        if let Some(order_book) = self.order_books.get_mut(&symbol) {
            // If order book is not synced, buffer the update
            if !order_book.is_synchronized() {
                if let Some(sync_protocol) = self.sync_protocols.get_mut(&symbol) {
                    sync_protocol.buffer_update(update)?;
                }
                return Ok(());
            }
            
            // Apply update to order book
            match order_book.apply_update(&update) {
                Ok(_) => {
                    // Successfully applied update
                }
                Err(HftError::SequenceGap { .. }) | Err(HftError::OrderBookCorruption) => {
                    // Sequence gap or corruption detected, initiate recovery
                    self.initiate_recovery(&symbol).await?;
                }
                Err(e) => {
                    error!("Failed to apply order book update: {}", e);
                    return Err(e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Handle market data halt (heartbeat timeout)
    async fn handle_market_data_halt(&mut self) -> Result<(), HftError> {
        warn!("Market data halt detected, initiating emergency procedures");
        
        // Cancel all open orders for all symbols
        for symbol in &self.symbols {
            if let Err(e) = self.cancel_all_orders(symbol).await {
                error!("Failed to cancel orders for symbol {}: {}", symbol, e);
            }
        }
        
        // Reset all order books
        for (_, order_book) in self.order_books.iter_mut() {
            order_book.reset();
        }
        
        // Clear all sync buffers
        for (_, sync_protocol) in self.sync_protocols.iter_mut() {
            sync_protocol.clear_buffer();
        }
        
        // TODO: Implement reconnection logic
        
        Ok(())
    }
    
    /// Initiate recovery procedure when sequence gap or corruption is detected
    async fn initiate_recovery(&mut self, symbol: &str) -> Result<(), HftError> {
        warn!("Initiating recovery for symbol: {}", symbol);
        
        // Reset the order book
        if let Some(order_book) = self.order_books.get_mut(symbol) {
            order_book.reset();
        }
        
        // Clear buffered updates
        if let Some(sync_protocol) = self.sync_protocols.get_mut(symbol) {
            sync_protocol.clear_buffer();
        }
        
        // Fetch new snapshot and resynchronize
        if let Some(sync_protocol) = self.sync_protocols.get_mut(symbol) {
            if let Some(order_book) = self.order_books.get_mut(symbol) {
                sync_protocol.sync_with_snapshot(symbol, order_book).await?;
            }
        }
        
        Ok(())
    }
    
    /// Cancel all open orders (emergency function)
    async fn cancel_all_orders(&self, symbol: &str) -> Result<(), HftError> {
        // In a real implementation, we would use API keys from configuration
        // For now, we'll just log the action
        info!("Would cancel all orders for symbol: {}", symbol);
        Ok(())
    }
}