//! Project AEGIS - Layer 1
//! High-Frequency Trading System for Binance Perpetual Futures
//!
//! This library provides the core components for a deterministic,
//! zero-trust, zero-allocation HFT system.

pub mod core;
pub mod network;
pub mod orderbook;
pub mod engine;

pub use engine::trading_engine::TradingEngine;

/// Initialize the HFT system
pub fn init_hft_system(symbols: Vec<String>) -> TradingEngine {
    let mut engine = TradingEngine::new();
    
    for symbol in symbols {
        engine.add_symbol(symbol);
    }
    
    engine
}