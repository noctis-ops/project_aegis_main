//! Custom error types for the Alpha Engine

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AlphaError {
    #[error("Insufficient capital for trade: {0}")]
    InsufficientCapital(String),
    
    #[error("Invalid signal parameters: {0}")]
    InvalidSignal(String),
    
    #[error("Market data unavailable for symbol: {0}")]
    MarketDataUnavailable(String),
    
    #[error("Position sizing calculation error: {0}")]
    PositionSizingError(String),
    
    #[error("Signal expired: {0}")]
    SignalExpired(String),
    
    #[error("Other error: {0}")]
    Other(String),
}