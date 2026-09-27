//! Custom error types for the Execution Engine

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] tokio_tungstenite::tungstenite::Error),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("Order not found: {0}")]
    OrderNotFound(String),
    
    #[error("Insufficient margin: {0}")]
    InsufficientMargin(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    
    #[error("Invalid order parameters: {0}")]
    InvalidOrderParameters(String),
    
    #[error("Order rejected: {0}")]
    OrderRejected(String),
    
    #[error("User data stream disconnected")]
    UserDataStreamDisconnected,
    
    #[error("Order timeout: {0}")]
    OrderTimeout(String),
    
    #[error("Other error: {0}")]
    Other(String),
}