//! Project AEGIS - Layer 3
//! Order Management System & Execution Engine
//!
//! This is the main entry point for the execution engine that implements
//! order lifecycle management, smart order routing, and high-frequency execution.

mod core;
mod components;
mod integration;
mod security;

use tracing::{info, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 3 Execution Engine");
    
    // Initialize the execution engine
    let mut engine = components::ExecutionEngine::new();
    
    // Start the engine
    if let Err(e) = engine.start().await {
        error!("Failed to start execution engine: {}", e);
        return Err(Box::new(e));
    }
    
    info!("Project AEGIS Layer 3 started successfully");
    
    // Keep the application running
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}