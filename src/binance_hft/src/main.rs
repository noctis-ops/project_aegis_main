//! Project AEGIS - Layer 1
//! High-Frequency Trading System for Binance Perpetual Futures
//! 
//! This is the main entry point for the HFT engine that implements
//! deterministic time processing, zero-trust networking, and zero dynamic allocation.

mod core;
mod network;
mod orderbook;
mod engine;

use tracing::{info, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 1 HFT Engine");
    
    // Initialize the trading engine
    let mut engine = engine::TradingEngine::new();
    
    // Start the engine
    if let Err(e) = engine.start().await {
        error!("Failed to start trading engine: {}", e);
        return Err(Box::new(e));
    }
    
    info!("Project AEGIS started successfully");
    
    // Keep the application running
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}