//! Project AEGIS - Layer 2
//! Alpha Generation & Signal Logic Engine
//!
//! This library provides the core components for generating trading signals
//! based on market microstructure analysis.

pub mod core;
pub mod features;
pub mod logic;
pub mod integration;

pub use logic::alpha_engine::AlphaEngine;
pub use core::{TradeSignal, TradeIntent, MarketDataEvent, WalletBalance};

/// Initialize the alpha engine with default settings
pub fn init_alpha_engine(symbols: Vec<String>) -> AlphaEngine {
    let mut engine = AlphaEngine::new();
    
    for symbol in symbols {
        engine.add_symbol(symbol);
    }
    
    engine
}