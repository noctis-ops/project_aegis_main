//! Custom error types for the Risk Management System

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RiskError {
    #[error("Circuit breaker activated: {0}")]
    CircuitBreakerActivated(String),
    
    #[error("Portfolio drawdown exceeded limit: {0}")]
    DrawdownExceeded(String),
    
    #[error("Insufficient equity for trade: {0}")]
    InsufficientEquity(String),
    
    #[error("Risk limit exceeded: {0}")]
    RiskLimitExceeded(String),
    
    #[error("Funding rate too negative: {0}")]
    NegativeFundingRate(String),
    
    #[error("Volatility anomaly detected: {0}")]
    VolatilityAnomaly(String),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("Telemetry error: {0}")]
    TelemetryError(String),
    
    #[error("Recovery protocol error: {0}")]
    RecoveryError(String),
    
    #[error("Other error: {0}")]
    Other(String),
}