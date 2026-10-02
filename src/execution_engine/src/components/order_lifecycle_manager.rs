//! Order Lifecycle Manager implementation

use crate::core::{Order, OrderStatus, ExecutionError};
use dashmap::DashMap;
use tracing::{info, warn, error, debug};
use std::sync::Arc;

/// Order Lifecycle Manager
pub struct OrderLifecycleManager {
    active_orders: Arc<DashMap<String, Order>>, // client_order_id -> Order
}

impl OrderLifecycleManager {
    /// Create a new Order Lifecycle Manager
    pub fn new() -> Self {
        Self {
            active_orders: Arc::new(DashMap::new()),
        }
    }
    
    /// Track a new order
    pub fn track_order(&self, order: Order) {
        info!("Tracking order: {}", order.client_order_id);
        self.active_orders.insert(order.client_order_id.clone(), order);
    }
    
    /// Update order status based on execution report
    pub fn update_order_status(&self, client_order_id: &str, status: OrderStatus, filled_qty: f64, avg_price: f64) {
        if let Some(mut order) = self.active_orders.get_mut(client_order_id) {
            info!("Updating order {} status to {:?}", client_order_id, status);
            
            order.status = status;
            order.filled_quantity = filled_qty;
            order.avg_price = avg_price;
            order.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
                
            debug!("Order {} updated: {:?}, filled: {}, avg_price: {}", 
                   client_order_id, status, filled_qty, avg_price);
        } else {
            warn!("Order not found for update: {}", client_order_id);
        }
    }
    
    /// Handle partial fill
    pub fn handle_partial_fill(&self, client_order_id: &str, filled_qty: f64, avg_price: f64) -> Result<(), ExecutionError> {
        if let Some(mut order) = self.active_orders.get_mut(client_order_id) {
            info!("Handling partial fill for order {}: {} filled at avg price {}", 
                  client_order_id, filled_qty, avg_price);
            
            order.status = OrderStatus::PartiallyFilled;
            order.filled_quantity = filled_qty;
            order.avg_price = avg_price;
            order.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
                
            // In hyper-scalping, partial fills are problematic
            // If we have a signal reversal, we should cancel the remaining quantity
            // This would be handled by the strategy layer
            
            Ok(())
        } else {
            error!("Order not found for partial fill: {}", client_order_id);
            Err(ExecutionError::OrderNotFound(client_order_id.to_string()))
        }
    }
    
    /// Mark order as filled
    pub fn mark_order_filled(&self, client_order_id: &str, filled_qty: f64, avg_price: f64) {
        if let Some(mut order) = self.active_orders.get_mut(client_order_id) {
            info!("Marking order {} as filled: {} at avg price {}", 
                  client_order_id, filled_qty, avg_price);
            
            order.status = OrderStatus::Filled;
            order.filled_quantity = filled_qty;
            order.avg_price = avg_price;
            order.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
                
            // Remove from active orders since it's fully filled
            self.active_orders.remove(client_order_id);
        } else {
            warn!("Order not found for fill completion: {}", client_order_id);
        }
    }
    
    /// Mark order as canceled
    pub fn mark_order_canceled(&self, client_order_id: &str) {
        if let Some(mut order) = self.active_orders.get_mut(client_order_id) {
            info!("Marking order {} as canceled", client_order_id);
            
            order.status = OrderStatus::Canceled;
            order.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
                
            // Remove from active orders
            self.active_orders.remove(client_order_id);
        } else {
            warn!("Order not found for cancellation: {}", client_order_id);
        }
    }
    
    /// Get timed out orders
    pub fn get_timed_out_orders(&self) -> Vec<Order> {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
            
        let mut timed_out_orders = Vec::new();
        
        for entry in self.active_orders.iter() {
            let order = entry.value();
            // time_to_live == 0 means no TTL (protective GTC-style orders);
            // only enforce expiry for orders that actually carry one.
            if order.time_to_live > 0 && current_time > (order.created_at + order.time_to_live) {
                timed_out_orders.push(order.clone());
            }
        }
        
        timed_out_orders
    }
    
    /// Get active order count
    pub fn get_active_order_count(&self) -> usize {
        self.active_orders.len()
    }
    
    /// Get order by client order ID
    pub fn get_order(&self, client_order_id: &str) -> Option<Order> {
        self.active_orders.get(client_order_id).map(|r| r.clone())
    }
    
    /// Cancel all active orders (emergency function)
    pub fn cancel_all_orders(&self) -> Vec<String> {
        let mut canceled_orders = Vec::new();
        
        for entry in self.active_orders.iter() {
            let order = entry.value();
            canceled_orders.push(order.client_order_id.clone());
            info!("Canceling order due to emergency: {}", order.client_order_id);
        }
        
        // Clear all active orders
        self.active_orders.clear();
        
        canceled_orders
    }
}