//! Pessimistic Local Matching Engine for Shadow Trading

use crate::core::{OrderBookSnapshot, TradeIntent, TradeSide, SimulationError};
use tracing::{info, debug, warn, error};
use std::collections::HashMap;

/// Pessimistic Local Matching Engine
pub struct PessimisticMatchingEngine {
    order_book_cache: HashMap<String, OrderBookSnapshot>,
    virtual_orders: HashMap<String, VirtualOrder>,
    queue_positions: HashMap<String, QueuePosition>,
    is_initialized: bool,
}

/// Virtual order for shadow trading
#[derive(Debug, Clone)]
pub struct VirtualOrder {
    pub intent: TradeIntent,
    pub queue_position: QueuePosition,
    pub status: VirtualOrderStatus,
}

/// Queue position tracking
#[derive(Debug, Clone)]
pub struct QueuePosition {
    pub symbol: String,
    pub price_level: f64,
    pub cumulative_volume_ahead: f64, // Volume ahead of our order
    pub order_volume: f64,           // Our order volume
    pub executed_volume: f64,        // Volume already executed
}

/// Virtual order status
#[derive(Debug, Clone, PartialEq)]
pub enum VirtualOrderStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
}

impl PessimisticMatchingEngine {
    /// Create a new pessimistic matching engine
    pub fn new() -> Self {
        Self {
            order_book_cache: HashMap::new(),
            virtual_orders: HashMap::new(),
            queue_positions: HashMap::new(),
            is_initialized: false,
        }
    }
    
    /// Initialize the matching engine
    pub fn initialize(&mut self) -> Result<(), SimulationError> {
        self.is_initialized = true;
        info!("Pessimistic Matching Engine initialized");
        Ok(())
    }
    
    /// Update order book cache with live data
    pub fn update_order_book(&mut self, order_book: OrderBookSnapshot) -> Result<(), SimulationError> {
        if !self.is_initialized {
            return Err(SimulationError::ExecutionError(
                "Matching engine not initialized".to_string()
            ));
        }
        
        self.order_book_cache.insert(order_book.symbol.clone(), order_book);
        Ok(())
    }
    
    /// Submit a virtual order for matching
    pub fn submit_virtual_order(&mut self, intent: TradeIntent) -> Result<String, SimulationError> {
        if !self.is_initialized {
            return Err(SimulationError::ExecutionError(
                "Matching engine not initialized".to_string()
            ));
        }
        
        // Calculate queue position for this order
        let queue_position = self.calculate_queue_position(&intent)?;
        
        let order_id = format!("shadow_{}", uuid::Uuid::new_v4());
        
        let virtual_order = VirtualOrder {
            intent: intent.clone(),
            queue_position: queue_position.clone(),
            status: VirtualOrderStatus::Pending,
        };
        
        self.virtual_orders.insert(order_id.clone(), virtual_order);
        self.queue_positions.insert(order_id.clone(), queue_position);
        
        info!("Submitted virtual order: {}", order_id);
        Ok(order_id)
    }
    
    /// Calculate queue position for an order
    fn calculate_queue_position(&self, intent: &TradeIntent) -> Result<QueuePosition, SimulationError> {
        let order_book = self.order_book_cache.get(&intent.symbol)
            .ok_or_else(|| SimulationError::ExecutionError(
                format!("No order book data for symbol: {}", intent.symbol)
            ))?;
        
        let mut cumulative_volume_ahead = 0.0;
        
        // Calculate cumulative volume ahead based on order side
        match intent.side {
            TradeSide::Buy => {
                // For buy orders, we look at asks at or below our price
                for ask in &order_book.asks {
                    if ask.price <= intent.price {
                        cumulative_volume_ahead += ask.quantity;
                    } else {
                        break;
                    }
                }
            }
            TradeSide::Sell => {
                // For sell orders, we look at bids at or above our price
                for bid in &order_book.bids {
                    if bid.price >= intent.price {
                        cumulative_volume_ahead += bid.quantity;
                    } else {
                        break;
                    }
                }
            }
        }
        
        Ok(QueuePosition {
            symbol: intent.symbol.clone(),
            price_level: intent.price,
            cumulative_volume_ahead,
            order_volume: intent.size,
            executed_volume: 0.0,
        })
    }
    
    /// Process tick data and update virtual order statuses
    pub fn process_tick_data(&mut self, tick_data: &crate::core::TradeEvent) -> Result<Vec<String>, SimulationError> {
        if !self.is_initialized {
            return Ok(vec![]);
        }
        
        let mut filled_orders = Vec::new();
        
        // Process each virtual order
        for (order_id, virtual_order) in self.virtual_orders.iter_mut() {
            if virtual_order.status == VirtualOrderStatus::Filled || 
               virtual_order.status == VirtualOrderStatus::Cancelled {
                continue;
            }
            
            // Check if this tick affects our order
            if virtual_order.intent.symbol != tick_data.symbol {
                continue;
            }
            
            let queue_pos = self.queue_positions.get_mut(order_id)
                .ok_or_else(|| SimulationError::ExecutionError(
                    format!("Missing queue position for order: {}", order_id)
                ))?;
            
            // Check if tick price matches our order price
            if (tick_data.price - queue_pos.price_level).abs() < f64::EPSILON {
                // Update executed volume
                queue_pos.executed_volume += tick_data.quantity;
                
                // Check if order should be filled
                if queue_pos.executed_volume >= queue_pos.cumulative_volume_ahead {
                    virtual_order.status = VirtualOrderStatus::Filled;
                    filled_orders.push(order_id.clone());
                    info!("Virtual order filled: {}", order_id);
                } else if queue_pos.executed_volume > 0.0 {
                    virtual_order.status = VirtualOrderStatus::PartiallyFilled;
                    debug!("Virtual order partially filled: {} ({}/{})", 
                           order_id, queue_pos.executed_volume, queue_pos.cumulative_volume_ahead);
                }
            }
        }
        
        Ok(filled_orders)
    }
    
    /// Cancel a virtual order
    pub fn cancel_virtual_order(&mut self, order_id: &str) -> Result<(), SimulationError> {
        if let Some(virtual_order) = self.virtual_orders.get_mut(order_id) {
            virtual_order.status = VirtualOrderStatus::Cancelled;
            info!("Cancelled virtual order: {}", order_id);
            Ok(())
        } else {
            Err(SimulationError::ExecutionError(
                format!("Virtual order not found: {}", order_id)
            ))
        }
    }
    
    /// Get virtual order status
    pub fn get_order_status(&self, order_id: &str) -> Option<&VirtualOrderStatus> {
        self.virtual_orders.get(order_id).map(|o| &o.status)
    }
    
    /// Check if engine is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}