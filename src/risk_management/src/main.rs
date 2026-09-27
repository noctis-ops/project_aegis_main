//! Project AEGIS - Layer 4
//! Telemetry, Risk Management & Circuit Breakers
//!
//! This is the main entry point for the risk management system that implements
//! anti-martingale risk management, hierarchical circuit breakers, and telemetry.

mod core;
mod components;
mod telemetry;
mod protocols;

use tracing::{info, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    info!("Starting Project AEGIS - Layer 4 Risk Management System");
    
    // Initialize the risk management system
    let mut risk_manager = components::RiskManagementSystem::new();
    
    // Start the system
    if let Err(e) = risk_manager.start().await {
        error!("Failed to start risk management system: {}", e);
        return Err(Box::new(e));
    }
    
    info!("Project AEGIS Layer 4 started successfully");
    
    // Keep the application running
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}