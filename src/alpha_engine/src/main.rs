//! Project AEGIS - Layer 2
//! Alpha Generation & Signal Logic Engine
//!
//! Thin launcher: the entire engine lives in the library crate. Declaring
//! the modules here again (as before) made cargo compile the engine twice
//! and produced target-specific unused-import warnings.
//!
//! Standalone note: the engine requires its crossbeam channels to be wired
//! before start() — `set_market_data_receiver` (from Layer 1) and
//! `set_trade_signal_sender` (to Layer 3). Without them it exits with a
//! clear `ChannelsNotInitialized` error instead of pretending to run.

use alpha_engine::AlphaEngine;
use tracing::{info, error};

fn main() -> std::process::ExitCode {
    // Initialize structured logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 2 Alpha Engine");
    
    let mut engine = AlphaEngine::new();
    
    // `start()` drives the processing loop for the whole lifetime of the
    // process; it only returns when the market data channel disconnects or
    // a fatal error occurs, so no separate keep-alive loop is needed.
    if let Err(e) = engine.start() {
        error!("Alpha engine terminated with error: {}", e);
        return std::process::ExitCode::FAILURE;
    }
    
    info!("Project AEGIS - Layer 2 Alpha Engine shut down cleanly");
    std::process::ExitCode::SUCCESS
}
