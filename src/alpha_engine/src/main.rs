//! Project AEGIS - Layer 2
//! Alpha Generation & Signal Logic Engine
//!
//! This is the main entry point for the alpha engine that implements
//! weighted confluence scoring, microstructure analysis, and signal generation.

mod core;
mod features;
mod logic;
mod integration;

use tracing::{info, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 2 Alpha Engine");
    
    // Initialize the alpha engine
    let mut engine = logic::AlphaEngine::new();
    
    // Start the engine
    if let Err(e) = engine.start().await {
        error!("Failed to start alpha engine: {}", e);
        return Err(Box::new(e));
    }
    
    info!("Project AEGIS Layer 2 started successfully");
    
    // Keep the application running
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}