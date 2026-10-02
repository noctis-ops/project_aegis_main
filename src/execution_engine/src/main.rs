//! Project AEGIS - Layer 3
//! Order Management System & Execution Engine
//!
//! Thin launcher: the entire engine lives in the library crate. Declaring
//! the modules here again (as before) made cargo compile the engine twice
//! and produced target-specific unused-import warnings.
//!
//! Standalone note: the engine requires its crossbeam channels to be wired
//! before start() — `set_trade_intent_receiver` (from Layer 2) and
//! optionally `set_execution_report_sender` (to Layer 2/4). Without them it
//! exits with a clear `TradeIntentReceiverNotSet` error instead of
//! pretending to run.

use execution_engine::ExecutionEngine;
use tracing::{info, error};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Initialize structured logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 3 Execution Engine");
    
    let mut engine = ExecutionEngine::new();
    
    // `start()` drives the processing loop for the whole lifetime of the
    // process; it only returns when a fatal error occurs, so no separate
    // keep-alive loop is needed after it.
    if let Err(e) = engine.start().await {
        error!("Execution engine terminated with error: {}", e);
        return std::process::ExitCode::FAILURE;
    }
    
    info!("Project AEGIS - Layer 3 Execution Engine shut down cleanly");
    std::process::ExitCode::SUCCESS
}
