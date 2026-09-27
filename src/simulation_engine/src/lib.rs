//! Project AEGIS - Layer 5
//! Backtesting Infrastructure, Simulation & CI/CD Deployment
//!
//! This library provides the core components for deterministic backtesting,
//! pessimistic execution simulation, and deployment pipeline automation.

pub mod core;
pub mod components;
pub mod infrastructure;
pub mod pipeline;

pub use components::simulation_engine::SimulationEngine;
pub use core::{SimulationConfig, BacktestReport, MarketEvent};

/// Initialize the simulation engine with default settings
pub fn init_simulation_engine() -> SimulationEngine {
    SimulationEngine::new()
}