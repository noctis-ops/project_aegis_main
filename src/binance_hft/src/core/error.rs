//! Custom error types for the HFT system

use thiserror::Error;

#[derive(Error, Debug)]
pub enum HftError {
    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] tokio_tungstenite::tungstenite::Error),
    
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] simd_json::Error),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
    
    #[error("Sequence gap detected: expected {expected}, got {actual}")]
    SequenceGap { expected: u64, actual: u64 },
    
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),
    
    #[error("Order book corruption detected")]
    OrderBookCorruption,
    
    #[error("Market data halted")]
    MarketDataHalted,
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Connection timeout")]
    ConnectionTimeout,
    
    #[error("Other error: {0}")]
    Other(String),
}