//! Pessimistic Local Matching Engine for Shadow Trading

use crate::core::{OrderBookSnapshot, TradeIntent, TradeSide, SimulationError};
use tracing::{debug, info};
use std::collections::HashMap;

/// What the matcher saw while orders were live, reported through the backtest so a run
/// that filled nothing says *which* of the three reasons it was: no print arrived at all,
/// none reached our level, or the level traded and the queue in front of us never cleared.
/// Only the last one is a reading of the market.
#[derive(Debug, Clone, Copy, Default)]
pub struct MatcherStatistics {
    pub prints_while_orders_live: usize,
    pub prints_at_or_through_level: usize,
    pub nearest_print_pct: Option<f64>,
}

/// Prices live on a venue's tick grid, and the two sides of a tape arrive on different
/// streams, so a price has to be compared with a tolerance that is wide enough to absorb
/// parsing noise and far narrower than one tick. `f64::EPSILON` is 2.2e-16 *absolute*: at
/// 100000.0 two readings of the same printed price can differ by more than that, which is
/// how a matcher can watch thousands of prints and report not one fill. One part in 1e9 is
/// ~1e-4 of BTCUSDT's smallest tick, so it accepts a level and nothing else.
fn price_tolerance(price: f64) -> f64 {
    price.abs() * 1e-9
}

fn prices_equal(a: f64, b: f64) -> bool {
    (a - b).abs() <= price_tolerance(b)
}

/// Pessimistic Local Matching Engine
pub struct PessimisticMatchingEngine {
    order_book_cache: HashMap<String, OrderBookSnapshot>,
    virtual_orders: HashMap<String, VirtualOrder>,
    queue_positions: HashMap<String, QueuePosition>,
    statistics: MatcherStatistics,
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
            statistics: MatcherStatistics::default(),
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
    /// The volume standing in front of a resting order.
    ///
    /// Price-time priority puts us behind whatever the book already displayed *at our own
    /// price on our own side*. The opposite side's size at or through our price - what this
    /// used to add up - is zero for any book that is not crossed, which made the
    /// "pessimistic" queue exclude nothing and left a raw price comparison to decide fills.
    fn calculate_queue_position(&self, intent: &TradeIntent) -> Result<QueuePosition, SimulationError> {
        let order_book = self.order_book_cache.get(&intent.symbol)
            .ok_or_else(|| SimulationError::ExecutionError(
                format!("No order book data for symbol: {}", intent.symbol)
            ))?;

        let levels = match intent.side {
            TradeSide::Buy => &order_book.bids,
            TradeSide::Sell => &order_book.asks,
        };

        let mut cumulative_volume_ahead = 0.0;
        for level in levels {
            if prices_equal(level.price, intent.price) {
                cumulative_volume_ahead += level.quantity;
                continue;
            }
            // The snapshot is best-first, so once the levels have moved past our price no
            // later one can be at it - and a level priced *worse* than ours is not in front
            // of us, it is behind us.
            let past_our_level = match intent.side {
                TradeSide::Buy => level.price < intent.price,
                TradeSide::Sell => level.price > intent.price,
            };
            if past_our_level {
                break;
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
        let mut reached_levels = 0usize;
        let mut live_orders = 0usize;
        let mut nearest: Option<f64> = None;

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

            live_orders += 1;
            let quoted = virtual_order.intent.price;
            let distance = (tick_data.price - quoted).abs() / quoted.max(f64::MIN_POSITIVE);
            nearest = Some(match nearest {
                Some(best) if best <= distance => best,
                _ => distance,
            });

            let queue_pos = self.queue_positions.get_mut(order_id)
                .ok_or_else(|| SimulationError::ExecutionError(
                    format!("Missing queue position for order: {}", order_id)
                ))?;

            // A print at or *through* the quoted level means that level traded. The old test
            // was price equality, so a seller printing through our bid - which on a venue
            // executes us at our own better price - was reported as no fill at all.
            let reached = match virtual_order.intent.side {
                TradeSide::Buy => tick_data.price <= quoted + price_tolerance(quoted),
                TradeSide::Sell => tick_data.price >= quoted - price_tolerance(quoted),
            };
            if !reached {
                continue;
            }
            reached_levels += 1;

            // Pessimistic where it should be: the volume displayed in front of us has to
            // clear before our size is reached, and we are filled at our quoted price.
            queue_pos.executed_volume += tick_data.quantity;

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

        if live_orders > 0 {
            self.statistics.prints_while_orders_live += 1;
        }
        self.statistics.prints_at_or_through_level += reached_levels;
        if let Some(distance) = nearest {
            self.statistics.nearest_print_pct = Some(match self.statistics.nearest_print_pct {
                Some(best) if best <= distance => best,
                _ => distance,
            });
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
    
    /// The order behind an id, so a caller can act on a fill.
    ///
    /// `process_tick_data` reports which orders filled; booking the trade needs the
    /// intent (size, side, protection levels) that the id was created from.
    pub fn get_virtual_order(&self, order_id: &str) -> Option<&VirtualOrder> {
        self.virtual_orders.get(order_id)
    }
    
    /// Check if engine is initialized
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// What the matcher watched while orders were live, for the replay's census. Read
    /// rather than accumulated by the caller: only the matcher knows which prints it
    /// compared against which quoted level.
    pub fn statistics(&self) -> MatcherStatistics {
        self.statistics
    }
}
