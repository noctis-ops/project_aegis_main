//! Data interface for receiving market data from Layer 1

use crate::core::MarketDataEvent;
use crossbeam::channel::Receiver;
use tracing::{info, error};

/// Data interface for receiving market data
pub struct DataInterface {
    receiver: Receiver<MarketDataEvent>,
}

impl DataInterface {
    /// Create a new data interface
    pub fn new(receiver: Receiver<MarketDataEvent>) -> Self {
        Self { receiver }
    }
    
    /// Receive market data event (non-blocking)
    pub fn receive_event(&self) -> Option<MarketDataEvent> {
        match self.receiver.try_recv() {
            Ok(event) => Some(event),
            Err(crossbeam::channel::TryRecvError::Empty) => None,
            Err(crossbeam::channel::TryRecvError::Disconnected) => {
                error!("Data channel disconnected");
                None
            }
        }
    }
    
    /// Check if there are pending events
    pub fn has_pending_events(&self) -> bool {
        !self.receiver.is_empty()
    }
}