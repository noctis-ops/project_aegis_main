//! Signal generator implementation

use crate::core::{OrderBookSnapshot, AggTrade, TradeSignal, TradeSide};
use crate::features::{calculate_obi, TradeFlowToxicity, detect_liquidity_voids};
use tracing::debug;

/// Signal generator for a specific symbol
pub struct SignalGenerator {
    symbol: String,
    obi_threshold: f64,
    toxicity_analyzer: TradeFlowToxicity,
    last_signal_time: u64,
    signal_cooldown: u64, // milliseconds
}

impl SignalGenerator {
    /// Create a new signal generator
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            obi_threshold: 0.4, // Default threshold
            toxicity_analyzer: TradeFlowToxicity::new(100.0, 0.8), // 100 contract buckets, 0.8 toxicity threshold
            last_signal_time: 0,
            signal_cooldown: 1000, // 1 second cooldown
        }
    }
    
    /// Generate signal from order book update
    pub fn generate_signal_from_order_book(&mut self, order_book: &OrderBookSnapshot) -> Option<TradeSignal> {
        let current_time = order_book.timestamp;
        
        // Check cooldown
        if current_time < self.last_signal_time + self.signal_cooldown {
            return None;
        }
        
        // Calculate Order Book Imbalance
        let obi = calculate_obi(order_book, 5); // Top 5 levels
        
        // Check for toxic flow
        if self.toxicity_analyzer.is_toxic() {
            debug!("Toxic flow detected for symbol {}, skipping signal generation", self.symbol);
            return None;
        }
        
        // Check for liquidity voids
        let voids = detect_liquidity_voids(order_book, 0.1, 3.0); // Assuming 0.1 tick size
        
        // Generate buy signal conditions
        if obi > self.obi_threshold {
            // Strong buying pressure
            if let Some(best_bid) = order_book.bids.first() {
                // Check if we're near a liquidity void that supports upward movement
                let is_near_void = voids.iter().any(|v| {
                    v.direction == crate::features::LiquidityDirection::Up && 
                    (v.price_level - best_bid.price).abs() < 1.0 // Within 1 tick
                });
                
                if is_near_void || voids.is_empty() { // Generate signal even without voids for now
                    self.last_signal_time = current_time;
                    
                    return Some(TradeSignal {
                        symbol: self.symbol.clone(),
                        side: TradeSide::Buy,
                        price: best_bid.price,
                        size: 0.0, // Will be sized later
                        stop_loss: best_bid.price * 0.995, // 0.5% stop loss
                        take_profit: best_bid.price * 1.01, // 1% take profit
                        time_to_live: 200, // 200ms TTL
                        confidence: obi, // Use OBI as confidence measure
                        timestamp: current_time,
                    });
                }
            }
        }
        // Generate sell signal conditions
        else if obi < -self.obi_threshold {
            // Strong selling pressure
            if let Some(best_ask) = order_book.asks.first() {
                // Check if we're near a liquidity void that supports downward movement
                let is_near_void = voids.iter().any(|v| {
                    v.direction == crate::features::LiquidityDirection::Down && 
                    (v.price_level - best_ask.price).abs() < 1.0 // Within 1 tick
                });
                
                if is_near_void || voids.is_empty() { // Generate signal even without voids for now
                    self.last_signal_time = current_time;
                    
                    return Some(TradeSignal {
                        symbol: self.symbol.clone(),
                        side: TradeSide::Sell,
                        price: best_ask.price,
                        size: 0.0, // Will be sized later
                        stop_loss: best_ask.price * 1.005, // 0.5% stop loss
                        take_profit: best_ask.price * 0.99, // 1% take profit
                        time_to_live: 200, // 200ms TTL
                        confidence: -obi, // Use absolute OBI as confidence measure
                        timestamp: current_time,
                    });
                }
            }
        }
        
        None
    }
    
    /// Update trade flow analyzer with new trade data
    pub fn update_trade_flow(&mut self, trade: &AggTrade) {
        self.toxicity_analyzer.process_trade(trade);
    }
    
    /// Set OBI threshold for signal generation
    pub fn set_obi_threshold(&mut self, threshold: f64) {
        self.obi_threshold = threshold;
    }
    
    /// Set signal cooldown period
    pub fn set_signal_cooldown(&mut self, cooldown_ms: u64) {
        self.signal_cooldown = cooldown_ms;
    }
}