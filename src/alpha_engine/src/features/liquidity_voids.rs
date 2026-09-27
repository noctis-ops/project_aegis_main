//! Liquidity Voids detector

use crate::core::{OrderBookSnapshot, OrderBookLevel};

/// Liquidity void information
#[derive(Debug, Clone)]
pub struct LiquidityVoid {
    pub price_level: f64,
    pub gap_size: f64,
    pub direction: LiquidityDirection,
}

/// Direction of liquidity void
#[derive(Debug, Clone, PartialEq)]
pub enum LiquidityDirection {
    Up,   // Bullish void - price likely to move up
    Down, // Bearish void - price likely to move down
}

/// Detect liquidity voids in the order book
/// 
/// # Arguments
/// * `order_book` - Order book snapshot
/// * `tick_size` - Minimum price increment for the symbol
/// * `min_gap_multiplier` - Minimum gap size as multiple of tick_size (default: 3.0)
/// 
/// # Returns
/// Vector of detected liquidity voids
pub fn detect_liquidity_voids(
    order_book: &OrderBookSnapshot,
    tick_size: f64,
    min_gap_multiplier: f64,
) -> Vec<LiquidityVoid> {
    let mut voids = Vec::new();
    let min_gap_size = tick_size * min_gap_multiplier;
    
    // Check bids for downward voids
    voids.extend(detect_voids_in_levels(
        &order_book.bids,
        LiquidityDirection::Down,
        min_gap_size,
    ));
    
    // Check asks for upward voids
    voids.extend(detect_voids_in_levels(
        &order_book.asks,
        LiquidityDirection::Up,
        min_gap_size,
    ));
    
    voids
}

/// Detect voids in a sorted list of order book levels
fn detect_voids_in_levels(
    levels: &[OrderBookLevel],
    direction: LiquidityDirection,
    min_gap_size: f64,
) -> Vec<LiquidityVoid> {
    let mut voids = Vec::new();
    
    if levels.len() < 2 {
        return voids;
    }
    
    // Sort levels by price (they should already be sorted, but just in case)
    let mut sorted_levels = levels.to_vec();
    if direction == LiquidityDirection::Down {
        sorted_levels.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());
    } else {
        sorted_levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
    }
    
    // Check gaps between consecutive levels
    for i in 0..sorted_levels.len() - 1 {
        let current = &sorted_levels[i];
        let next = &sorted_levels[i + 1];
        
        let gap_size = (next.price - current.price).abs();
        
        // Check if gap is significant and current level has low volume
        if gap_size > min_gap_size && current.quantity < next.quantity * 0.1 {
            voids.push(LiquidityVoid {
                price_level: if direction == LiquidityDirection::Up {
                    current.price
                } else {
                    next.price
                },
                gap_size,
                direction: direction.clone(),
            });
        }
    }
    
    voids
}

/// Check if a price is near a liquidity void
/// 
/// # Arguments
/// * `price` - Current price to check
/// * `voids` - List of detected liquidity voids
/// * `tolerance` - How close price needs to be to void (as multiple of tick_size)
/// 
/// # Returns
/// Option with the nearest liquidity void if price is near it
pub fn is_near_liquidity_void(
    price: f64,
    voids: &[LiquidityVoid],
    tolerance: f64,
) -> Option<&LiquidityVoid> {
    voids.iter().find(|void| (void.price_level - price).abs() <= tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_liquidity_void_detection() {
        let order_book = OrderBookSnapshot {
            symbol: "BTCUSDT".to_string(),
            bids: vec![
                OrderBookLevel { price: 50000.0, quantity: 1.0 },
                OrderBookLevel { price: 49990.0, quantity: 0.5 }, // Large gap here
                OrderBookLevel { price: 49500.0, quantity: 2.0 },
            ],
            asks: vec![
                OrderBookLevel { price: 50010.0, quantity: 1.5 },
                OrderBookLevel { price: 50020.0, quantity: 0.8 }, // Large gap here
                OrderBookLevel { price: 50500.0, quantity: 3.0 },
            ],
            timestamp: 0,
        };
        
        let voids = detect_liquidity_voids(&order_book, 0.1, 3.0);
        assert!(!voids.is_empty());
    }
}