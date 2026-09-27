//! Main Alpha Engine implementation

use crate::core::{MarketDataEvent, TradeSignal, TradeIntent, SignalState, WalletBalance};
use crate::features::{
    calculate_obi, 
    TradeFlowToxicity, 
    detect_liquidity_voids, 
    OpenInterestDelta
};
use crate::logic::{SignalGenerator, PositionSizingEngine, RiskManager};
use crossbeam::channel::{Receiver, Sender, unbounded};
use tracing::{info, warn, error, debug};
use std::collections::HashMap;

/// Main Alpha Engine
pub struct AlphaEngine {
    symbols: Vec<String>,
    signal_states: HashMap<String, SignalState>,
    signal_generators: HashMap<String, SignalGenerator>,
    position_sizers: HashMap<String, PositionSizingEngine>,
    risk_managers: HashMap<String, RiskManager>,
    market_data_rx: Option<Receiver<MarketDataEvent>>,
    trade_signal_tx: Option<Sender<TradeIntent>>,
    wallet_balances: HashMap<String, WalletBalance>,
}

impl AlphaEngine {
    /// Create a new Alpha Engine
    pub fn new() -> Self {
        Self {
            symbols: vec!["BTCUSDT".to_string()], // Default symbol
            signal_states: HashMap::new(),
            signal_generators: HashMap::new(),
            position_sizers: HashMap::new(),
            risk_managers: HashMap::new(),
            market_data_rx: None,
            trade_signal_tx: None,
            wallet_balances: HashMap::new(),
        }
    }
    
    /// Add a symbol to track
    pub fn add_symbol(&mut self, symbol: String) {
        if !self.symbols.contains(&symbol) {
            self.symbols.push(symbol.clone());
            self.signal_states.insert(symbol.clone(), SignalState::Idle);
            
            // Initialize components for this symbol
            self.signal_generators.insert(
                symbol.clone(), 
                SignalGenerator::new(symbol.clone())
            );
            self.position_sizers.insert(
                symbol.clone(), 
                PositionSizingEngine::new()
            );
            self.risk_managers.insert(
                symbol.clone(), 
                RiskManager::new()
            );
        }
    }
    
    /// Set market data receiver channel
    pub fn set_market_data_receiver(&mut self, rx: Receiver<MarketDataEvent>) {
        self.market_data_rx = Some(rx);
    }
    
    /// Set trade signal sender channel
    pub fn set_trade_signal_sender(&mut self, tx: Sender<TradeIntent>) {
        self.trade_signal_tx = Some(tx);
    }
    
    /// Update wallet balance
    pub fn update_wallet_balance(&mut self, currency: String, balance: WalletBalance) {
        self.wallet_balances.insert(currency, balance);
    }
    
    /// Start the alpha engine
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing Alpha Engine for symbols: {:?}", self.symbols);
        
        // Initialize components for each symbol
        for symbol in &self.symbols {
            self.initialize_symbol(symbol)?;
        }
        
        // Start the main processing loop
        self.run_processing_loop().await?;
        
        Ok(())
    }
    
    /// Initialize components for a symbol
    fn initialize_symbol(&mut self, symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing components for symbol: {}", symbol);
        
        // Initialize signal state
        self.signal_states.insert(symbol.to_string(), SignalState::Idle);
        
        // Initialize signal generator
        self.signal_generators.insert(
            symbol.to_string(), 
            SignalGenerator::new(symbol.to_string())
        );
        
        // Initialize position sizer
        self.position_sizers.insert(
            symbol.to_string(), 
            PositionSizingEngine::new()
        );
        
        // Initialize risk manager
        self.risk_managers.insert(
            symbol.to_string(), 
            RiskManager::new()
        );
        
        info!("Components initialized for symbol: {}", symbol);
        Ok(())
    }
    
    /// Main processing loop
    async fn run_processing_loop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting main processing loop");
        
        // We need channels for communication
        if self.market_data_rx.is_none() || self.trade_signal_tx.is_none() {
            return Err("Market data receiver and trade signal sender must be set".into());
        }
        
        let market_data_rx = self.market_data_rx.as_ref().unwrap();
        let trade_signal_tx = self.trade_signal_tx.as_ref().unwrap();
        
        loop {
            // Process market data events
            if let Ok(event) = market_data_rx.try_recv() {
                self.process_market_data_event(event, trade_signal_tx)?;
            }
            
            // Small delay to prevent busy looping
            tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;
        }
    }
    
    /// Process market data events
    fn process_market_data_event(
        &mut self, 
        event: MarketDataEvent, 
        trade_signal_tx: &Sender<TradeIntent>
    ) -> Result<(), Box<dyn std::error::Error>> {
        match event {
            MarketDataEvent::OrderBookUpdate(order_book) => {
                self.process_order_book_update(order_book, trade_signal_tx)?;
            }
            MarketDataEvent::AggTrade(trade) => {
                self.process_agg_trade(trade)?;
            }
        }
        
        Ok(())
    }
    
    /// Process order book updates
    fn process_order_book_update(
        &mut self, 
        order_book: crate::core::OrderBookSnapshot, 
        trade_signal_tx: &Sender<TradeIntent>
    ) -> Result<(), Box<dyn std::error::Error>> {
        let symbol = order_book.symbol.clone();
        
        // Get signal generator for this symbol
        if let Some(generator) = self.signal_generators.get_mut(&symbol) {
            // Generate potential signals
            if let Some(signal) = generator.generate_signal_from_order_book(&order_book) {
                // Validate signal with risk manager
                if let Some(risk_manager) = self.risk_managers.get(&symbol) {
                    if risk_manager.validate_signal(&signal) {
                        // Size the position
                        if let Some(position_sizer) = self.position_sizers.get(&symbol) {
                            if let Some(wallet_balance) = self.wallet_balances.get(&symbol) {
                                let sized_signal = position_sizer.size_position(&signal, wallet_balance);
                                
                                // Convert to trade intent and send
                                let trade_intent = TradeIntent {
                                    symbol: sized_signal.symbol,
                                    side: sized_signal.side,
                                    price: sized_signal.price,
                                    size: sized_signal.size,
                                    stop_loss: sized_signal.stop_loss,
                                    take_profit: sized_signal.take_profit,
                                    time_to_live: sized_signal.time_to_live,
                                    timestamp: sized_signal.timestamp,
                                };
                                
                                // Send trade intent to execution layer
                                if let Err(e) = trade_signal_tx.send(trade_intent) {
                                    error!("Failed to send trade intent: {}", e);
                                } else {
                                    debug!("Sent trade intent for symbol: {}", symbol);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Process aggregated trades
    fn process_agg_trade(&mut self, trade: crate::core::AggTrade) -> Result<(), Box<dyn std::error::Error>> {
        let symbol = trade.symbol.clone();
        
        // Update toxicity analyzer
        if let Some(generator) = self.signal_generators.get_mut(&symbol) {
            generator.update_trade_flow(&trade);
        }
        
        Ok(())
    }
}