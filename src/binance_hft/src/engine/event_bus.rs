//! Internal event bus using ring buffers for zero-lock communication

use crate::core::MarketDataEvent;
use rtrb::{RingBuffer, Producer, Consumer};
use tracing::{error, warn};
use crate::core::constants::*;

/// Internal event bus using SPSC ring buffers
pub struct EventBus {
    producer: Producer<MarketDataEvent>,
    consumer: Consumer<MarketDataEvent>,
}

impl EventBus {
    /// Create a new event bus with specified buffer size
    pub fn new() -> Self {
        let (producer, consumer) = RingBuffer::new(RING_BUFFER_SIZE);
        Self { producer, consumer }
    }
    
    /// Send an event through the bus (non-blocking)
    pub fn send(&mut self, event: MarketDataEvent) -> Result<(), MarketDataEvent> {
        self.producer.push(event)
    }
    
    /// Receive an event from the bus (non-blocking)
    pub fn receive(&mut self) -> Result<MarketDataEvent, rtrb::PopError> {
        self.consumer.pop()
    }
    
    /// Check if the bus is empty
    pub fn is_empty(&self) -> bool {
        self.consumer.is_empty()
    }
    
    /// Check if the bus is full
    pub fn is_full(&self) -> bool {
        self.producer.is_full()
    }
    
    /// Get the number of available slots
    pub fn slots_available(&self) -> usize {
        self.producer.slots()
    }
    
    /// Get the number of pending events
    pub fn pending_events(&self) -> usize {
        self.consumer.slots()
    }
}