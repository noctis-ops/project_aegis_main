//! Execution interface for sending trade intents to Layer 3

use crate::core::TradeIntent;
use crossbeam::channel::Sender;
use tracing::{info, error};

/// Execution interface for sending trade intents
pub struct ExecutionInterface {
    sender: Sender<TradeIntent>,
}

impl ExecutionInterface {
    /// Create a new execution interface
    pub fn new(sender: Sender<TradeIntent>) -> Self {
        Self { sender }
    }
    
    /// Send trade intent to execution layer
    pub fn send_trade_intent(&self, intent: TradeIntent) -> Result<(), Box<dyn std::error::Error>> {
        // `send` consumes the intent, so log its identity BEFORE the move —
        // no clone/allocation on the hot path.
        info!("Sending trade intent for symbol: {}", intent.symbol);
        
        match self.sender.send(intent) {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("Failed to send trade intent: {}", e);
                Err(Box::new(e))
            }
        }
    }
    
    /// Check if channel is full
    pub fn is_channel_full(&self) -> bool {
        self.sender.is_full()
    }
}