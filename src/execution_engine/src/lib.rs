//! Project AEGIS - Layer 3
//! Order Management System & Execution Engine
//!
//! This library provides the core components for managing order lifecycle,
//! smart order routing, and high-frequency execution.

pub mod core;
pub mod components;
pub mod integration;
pub mod security;

pub use components::execution_engine::ExecutionEngine;
pub use core::{TradeIntent, ExecutionReport};

/// Initialize the execution engine with default settings
pub fn init_execution_engine(symbols: Vec<String>) -> ExecutionEngine {
    let mut engine = ExecutionEngine::new();
    
    for symbol in symbols {
        engine.add_symbol(symbol);
    }
    
    engine
}