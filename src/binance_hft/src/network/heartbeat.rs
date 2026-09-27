//! Heartbeat monitoring for WebSocket connections

use tokio::time::{interval, Duration};
use tracing::{warn, info};
use crate::core::{HftError, MarketDataEvent};
use tokio::sync::mpsc;
use crate::core::constants::*;

/// Heartbeat monitor for detecting silent disconnections
pub struct HeartbeatMonitor {
    interval_ms: u64,
    last_heartbeat: std::time::Instant,
    tx: mpsc::UnboundedSender<MarketDataEvent>,
}

impl HeartbeatMonitor {
    /// Create a new heartbeat monitor
    pub fn new(tx: mpsc::UnboundedSender<MarketDataEvent>) -> Self {
        Self {
            interval_ms: HEARTBEAT_TIMEOUT_MS,
            last_heartbeat: std::time::Instant::now(),
            tx,
        }
    }
    
    /// Update the last heartbeat time
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = std::time::Instant::now();
    }
    
    /// Start the heartbeat monitoring task
    pub fn start(mut self) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(self.interval_ms));
            
            loop {
                interval.tick().await;
                
                let elapsed = self.last_heartbeat.elapsed().as_millis() as u64;
                
                if elapsed > self.interval_ms {
                    warn!("Heartbeat timeout detected! Elapsed: {}ms, Threshold: {}ms", elapsed, self.interval_ms);
                    
                    // Send market data halt event
                    if let Err(e) = self.tx.send(MarketDataEvent::MarketDataHalt) {
                        warn!("Failed to send MarketDataHalt event: {}", e);
                    }
                }
            }
        });
    }
}