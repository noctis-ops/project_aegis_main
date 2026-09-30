//! Synchronization protocol for maintaining LOB consistency with Binance

use crate::core::{HftError, OrderBookSnapshot, OrderBookUpdate};
use crate::network::RestClient;
use tracing::{info, warn, error};
use std::collections::VecDeque;
use crate::core::constants::*;

/// Order book synchronization protocol
pub struct SyncProtocol {
    rest_client: RestClient,
    buffered_updates: VecDeque<OrderBookUpdate>,
    max_buffer_size: usize,
}

impl SyncProtocol {
    /// Create a new synchronization protocol
    pub fn new() -> Result<Self, HftError> {
        let rest_client = RestClient::new()?;
        
        Ok(Self {
            rest_client,
            buffered_updates: VecDeque::with_capacity(RING_BUFFER_SIZE),
            max_buffer_size: RING_BUFFER_SIZE,
        })
    }
    
    /// Buffer an update while waiting for snapshot
    pub fn buffer_update(&mut self, update: OrderBookUpdate) -> Result<(), HftError> {
        // Implement drop-oldest strategy if buffer is full
        if self.buffered_updates.len() >= self.max_buffer_size {
            warn!("Buffer full, dropping oldest update");
            self.buffered_updates.pop_front();
        }
        
        self.buffered_updates.push_back(update);
        Ok(())
    }
    
    /// Fetch and apply snapshot, then process buffered updates
    pub async fn sync_with_snapshot<T>(
        &mut self,
        symbol: &str,
        order_book: &mut T,
    ) -> Result<(), HftError>
    where
        T: LocalOrderBookTrait,
    {
        info!("Fetching order book snapshot for {}", symbol);
        
        // Fetch snapshot from REST API
        let snapshot = self.rest_client
            .get_orderbook_snapshot(symbol, ORDER_BOOK_DEPTH)
            .await?;
            
        info!("Received snapshot with last_update_id: {}", snapshot.last_update_id);
        
        // Apply snapshot to order book
        order_book.apply_snapshot(snapshot)?;
        
        // Process buffered updates that are newer than snapshot
        let mut processed_count = 0;
        let buffer_len = self.buffered_updates.len();
        
        self.buffered_updates.retain(|update| {
            if update.final_update_id > order_book.get_last_update_id() {
                match order_book.apply_update(update) {
                    Ok(_) => {
                        processed_count += 1;
                        false // Remove from buffer
                    }
                    Err(e) => {
                        error!("Failed to apply buffered update: {}", e);
                        true // Keep in buffer for retry
                    }
                }
            } else {
                // Update is older than snapshot, discard
                false
            }
        });
        
        info!(
            "Applied {} of {} buffered updates after snapshot sync",
            processed_count,
            buffer_len
        );
        
        Ok(())
    }
    
    /// Clear the update buffer
    pub fn clear_buffer(&mut self) {
        self.buffered_updates.clear();
    }
    
    /// Check if there are buffered updates
    pub fn has_buffered_updates(&self) -> bool {
        !self.buffered_updates.is_empty()
    }
}

/// Trait for order book implementations that can be synchronized
pub trait LocalOrderBookTrait {
    fn apply_snapshot(&mut self, snapshot: OrderBookSnapshot) -> Result<(), HftError>;
    fn apply_update(&mut self, update: &OrderBookUpdate) -> Result<(), HftError>;
    fn get_last_update_id(&self) -> u64;
    fn reset(&mut self);
}