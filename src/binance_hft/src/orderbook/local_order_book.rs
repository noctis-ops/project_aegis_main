//! Local Order Book (LOB) implementation with pre-allocated arrays

use crate::core::{OrderBookLevel, OrderBookUpdate, OrderBookSnapshot, HftError};
use tracing::{debug, error};
use crate::core::constants::*;
use crate::orderbook::sync_protocol::LocalOrderBookTrait;

/// Local Order Book implementation using pre-allocated arrays
pub struct LocalOrderBook {
    symbol: String,
    bids: Vec<OrderBookLevel>,
    asks: Vec<OrderBookLevel>,
    last_update_id: u64,
    previous_final_update_id: u64,
    is_synced: bool,
}

impl LocalOrderBook {
    /// Create a new local order book with pre-allocated capacity
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            bids: Vec::with_capacity(ORDER_BOOK_DEPTH),
            asks: Vec::with_capacity(ORDER_BOOK_DEPTH),
            last_update_id: 0,
            previous_final_update_id: 0,
            is_synced: false,
        }
    }
    
    /// Apply an order book snapshot to initialize the LOB
    pub fn apply_snapshot(&mut self, snapshot: OrderBookSnapshot) -> Result<(), HftError> {
        debug!("Applying snapshot with last_update_id: {}", snapshot.last_update_id);
        
        // Clear existing data
        self.bids.clear();
        self.asks.clear();
        
        // Process bids (buy orders) - should be sorted descending
        for bid in snapshot.bids {
            let price = bid[0].parse::<f64>()
                .map_err(|_| HftError::InvalidMessage("Invalid bid price".to_string()))?;
            let quantity = bid[1].parse::<f64>()
                .map_err(|_| HftError::InvalidMessage("Invalid bid quantity".to_string()))?;
            
            if quantity > 0.0 {
                self.bids.push(OrderBookLevel::new(price, quantity));
            }
        }
        
        // Process asks (sell orders) - should be sorted ascending
        for ask in snapshot.asks {
            let price = ask[0].parse::<f64>()
                .map_err(|_| HftError::InvalidMessage("Invalid ask price".to_string()))?;
            let quantity = ask[1].parse::<f64>()
                .map_err(|_| HftError::InvalidMessage("Invalid ask quantity".to_string()))?;
            
            if quantity > 0.0 {
                self.asks.push(OrderBookLevel::new(price, quantity));
            }
        }
        
        self.last_update_id = snapshot.last_update_id;
        self.is_synced = true;
        
        Ok(())
    }
    
    /// Apply an order book update to the LOB
    pub fn apply_update(&mut self, update: &OrderBookUpdate) -> Result<(), HftError> {
        // Check for sequence gap
        if self.is_synced && update.first_update_id > self.last_update_id + 1 {
            error!(
                "Sequence gap detected! Expected: {}, Got: {}",
                self.last_update_id + 1,
                update.first_update_id
            );
            return Err(HftError::SequenceGap {
                expected: self.last_update_id + 1,
                actual: update.first_update_id,
            });
        }
        
        // Check previous update ID consistency
        if let Some(prev_id) = update.previous_final_update_id {
            if self.is_synced && prev_id != self.last_update_id {
                error!(
                    "Previous update ID mismatch! Expected: {}, Got: {}",
                    self.last_update_id,
                    prev_id
                );
                return Err(HftError::OrderBookCorruption);
            }
        }
        
        // Process bids updates
        for bid_update in &update.bids {
            let price = bid_update[0].parse::<f64>()
                .map_err(|_| HftError::InvalidMessage("Invalid bid price".to_string()))?;
            let quantity = bid_update[1].parse::<f64>()
                .map_err(|_| HftError::InvalidMessage("Invalid bid quantity".to_string()))?;
            
            self.update_bid_level(price, quantity);
        }
        
        // Process asks updates
        for ask_update in &update.asks {
            let price = ask_update[0].parse::<f64>()
                .map_err(|_| HftError::InvalidMessage("Invalid ask price".to_string()))?;
            let quantity = ask_update[1].parse::<f64>()
                .map_err(|_| HftError::InvalidMessage("Invalid ask quantity".to_string()))?;
            
            self.update_ask_level(price, quantity);
        }
        
        // Update tracking IDs
        self.previous_final_update_id = self.last_update_id;
        self.last_update_id = update.final_update_id;
        
        Ok(())
    }
    
    /// Update a bid level in the order book
    fn update_bid_level(&mut self, price: f64, quantity: f64) {
        // Find existing level or insert new one
        let mut found = false;
        
        for level in self.bids.iter_mut() {
            if (level.price - price).abs() < f64::EPSILON {
                level.quantity = quantity;
                found = true;
                break;
            }
        }
        
        if !found && quantity > 0.0 {
            self.bids.push(OrderBookLevel::new(price, quantity));
            // Keep bids sorted in descending order
            self.bids.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
        } else if found && quantity <= 0.0 {
            // Remove empty levels
            self.bids.retain(|level| !level.is_empty());
        }
    }
    
    /// Update an ask level in the order book
    fn update_ask_level(&mut self, price: f64, quantity: f64) {
        // Find existing level or insert new one
        let mut found = false;
        
        for level in self.asks.iter_mut() {
            if (level.price - price).abs() < f64::EPSILON {
                level.quantity = quantity;
                found = true;
                break;
            }
        }
        
        if !found && quantity > 0.0 {
            self.asks.push(OrderBookLevel::new(price, quantity));
            // Keep asks sorted in ascending order
            self.asks.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
        } else if found && quantity <= 0.0 {
            // Remove empty levels
            self.asks.retain(|level| !level.is_empty());
        }
    }
    
    /// Get the best bid price
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|level| level.price)
    }
    
    /// Get the best ask price
    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|level| level.price)
    }
    
    /// Check if the order book is synchronized
    pub fn is_synchronized(&self) -> bool {
        self.is_synced
    }
    
    /// Reset the order book state
    pub fn reset(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.last_update_id = 0;
        self.previous_final_update_id = 0;
        self.is_synced = false;
    }
}

impl LocalOrderBookTrait for LocalOrderBook {
    fn apply_snapshot(&mut self, snapshot: OrderBookSnapshot) -> Result<(), HftError> {
        self.apply_snapshot(snapshot)
    }
    
    fn apply_update(&mut self, update: &OrderBookUpdate) -> Result<(), HftError> {
        self.apply_update(update)
    }
    
    fn get_last_update_id(&self) -> u64 {
        self.last_update_id
    }
    
    fn reset(&mut self) {
        self.reset();
    }
}