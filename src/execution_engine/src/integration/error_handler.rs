//! Error handler for Binance API responses

use crate::core::ExecutionError;
use tracing::{warn, error};

/// Error handler for Binance API responses
pub struct ErrorHandler;

impl ErrorHandler {
    /// Handle Binance API error response
    pub fn handle_binance_error(error_code: i32, error_msg: &str) -> ExecutionError {
        match error_code {
            -2019 => {
                // Insufficient margin
                error!("Insufficient margin error: {}", error_msg);
                ExecutionError::InsufficientMargin(error_msg.to_string())
            }
            -5022 => {
                // Post-only order rejection
                warn!("Post-only order rejected: {}", error_msg);
                ExecutionError::OrderRejected(error_msg.to_string())
            }
            -1003 => {
                // Rate limit exceeded
                error!("Rate limit exceeded: {}", error_msg);
                ExecutionError::RateLimitExceeded(error_msg.to_string())
            }
            -1013 => {
                // Invalid order parameters
                error!("Invalid order parameters: {}", error_msg);
                ExecutionError::InvalidOrderParameters(error_msg.to_string())
            }
            -2011 => {
                // "Unknown order sent": the order is already terminal (filled,
                // expired or canceled). Callers treat this as a completed intent —
                // the timeout sweep wanted the order off the book, and the
                // reconciliation engine learns its final state from the user data
                // stream — so it must not be reported as an execution failure.
                warn!("Unknown order sent: {}", error_msg);
                ExecutionError::OrderNotFound(error_msg.to_string())
            }
            _ => {
                // Unknown error
                error!("Unknown Binance error {}: {}", error_code, error_msg);
                ExecutionError::Other(format!("Binance error {}: {}", error_code, error_msg))
            }
        }
    }
    
    /// Handle network errors
    pub fn handle_network_error(error: reqwest::Error) -> ExecutionError {
        error!("Network error: {}", error);
        ExecutionError::NetworkError(error)
    }
    
    /// Handle WebSocket errors
    pub fn handle_websocket_error(error: tokio_tungstenite::tungstenite::Error) -> ExecutionError {
        error!("WebSocket error: {}", error);
        ExecutionError::WebSocketError(error)
    }
}