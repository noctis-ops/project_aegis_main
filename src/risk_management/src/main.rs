//! Project AEGIS - Layer 4
//! Telemetry, Risk Management & Circuit Breakers
//!
//! Thin launcher: the entire system lives in the library crate, so this binary only
//! wires up logging, starts the system and maps the outcome to a process exit code.
//! Declaring the modules here again (as before) made cargo compile the whole layer
//! twice and produced target-specific dead-code/unused-import warnings - the same
//! shape Layers 1 to 3 already use.
//!
//! Standalone note: `start()` spawns the monitoring tasks and returns; the loop
//! below parks the main task so the process (and its logging threads) stays alive.

use risk_management::RiskManagementSystem;
use tracing::{info, error};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Initialize structured logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 4 Risk Management System");
    
    // Initialize and start the risk management system
    let mut risk_manager = RiskManagementSystem::new();
    
    if let Err(e) = risk_manager.start().await {
        error!("Risk management system failed to start: {}", e);
        return std::process::ExitCode::FAILURE;
    }
    
    info!("Project AEGIS Layer 4 started successfully");
    
    // Keep the application running; the monitoring tasks own the work.
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}
