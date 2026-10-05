//! Project AEGIS - Layer 5
//! Backtesting Infrastructure, Simulation & CI/CD Deployment
//!
//! Thin launcher: the entire engine lives in the library crate, so this binary only
//! wires up logging, starts the engine and maps the outcome to a process exit code.
//! Declaring the modules here again (as before) made cargo compile the whole layer
//! twice - every error in it was reported twice, with different warnings per target
//! - which is exactly what Layers 1 to 4 stopped doing.
//!
//! `start()` spawns the feeding and execution-simulation tasks and returns; the park
//! loop below is what keeps those tasks (and the process) alive.

use simulation_engine::SimulationEngine;
use tracing::{info, error};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Initialize structured logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 5 Simulation Engine");
    
    let mut engine = SimulationEngine::new();
    info!(
        "Active execution backend: {:?} (a live backend has to be armed explicitly)",
        engine.execution_mode()
    );
    
    if let Err(e) = engine.start().await {
        error!("Simulation engine terminated with error: {}", e);
        return std::process::ExitCode::FAILURE;
    }
    
    info!("Project AEGIS - Layer 5 Simulation Engine started, parking main task");
    
    // The simulation runs on the tasks `start()` spawned; nothing here to do but stay
    // alive. `start()` returning Ok never means "finished", so exiting would kill the
    // backtest mid-flight.
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}
