//! Project AEGIS - Layer 1
//! High-Frequency Trading System for Binance Perpetual Futures
//! 
//! Thin launcher: the entire engine lives in the library crate, so this
//! binary only wires up logging, starts the engine and maps the outcome
//! to a process exit code. Declaring the modules here again (as before)
//! made cargo compile the whole engine twice and produced target-specific
//! dead-code/unused-import warnings.

use binance_hft::TradingEngine;
use tracing::{info, error};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Initialize structured logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 1 HFT Engine");
    
    let mut engine = TradingEngine::new();
    
    // `start()` drives the event loop for the whole lifetime of the process;
    // it only returns when a fatal error occurs, so no separate keep-alive
    // loop is needed after it.
    if let Err(e) = engine.start().await {
        error!("Trading engine terminated with error: {}", e);
        return std::process::ExitCode::FAILURE;
    }
    
    info!("Project AEGIS - Layer 1 HFT Engine shut down cleanly");
    std::process::ExitCode::SUCCESS
}
