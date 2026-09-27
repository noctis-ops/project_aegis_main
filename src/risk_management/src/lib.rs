//! Project AEGIS - Layer 4
//! Telemetry, Risk Management & Circuit Breakers
//!
//! This library provides the core components for anti-martingale risk management,
//! hierarchical circuit breakers, and telemetry systems.

pub mod core;
pub mod components;
pub mod telemetry;
pub mod protocols;

pub use components::risk_management_system::RiskManagementSystem;
pub use core::{PortfolioState, TradeRecord, CircuitBreakerStatus, RecoveryState};

/// Initialize the risk management system with default settings
pub fn init_risk_management_system() -> RiskManagementSystem {
    RiskManagementSystem::new()
}