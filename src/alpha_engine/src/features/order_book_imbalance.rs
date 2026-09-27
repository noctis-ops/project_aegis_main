//! Order Book Imbalance (OBI) calculator

use crate::core::OrderBookSnapshot;

/// Calculate Order Book Imbalance (OBI)
/// 
/// Formula: OBI = (Sum(Bid_Volumes) - Sum(Ask_Volumes)) / (Sum(Bid_Volumes) + Sum(Ask_Volumes))
/// 
/// # Arguments
/// * `order_book` - Order book snapshot with bids and asks
/// * `levels` - Number of top levels to consider (default: 5)
/// 
/// # Returns
/// OBI value between -1.0 and 1.0
pub fn calculate_obi(order_book: &OrderBookSnapshot, levels: usize) -> f64 {
    let levels = levels.min(order_book.bids.len()).min(order_book.asks.len());
    
    if levels == 0 {
        return 0.0;
    }
    
    let bid_volume: f64 = order_book.bids.iter().take(levels).map(|level| level.quantity).sum();
    let ask_volume: f64 = order_book.asks.iter().take(levels).map(|level| level.quantity).sum();
    
    let total_volume = bid_volume + ask_volume;
    
    if total_volume == 0.0 {
        return 0.0;
    }
    
    (bid_volume - ask_volume) / total_volume
}

/// Calculate incremental OBI update
/// 
/// This function updates OBI efficiently when only a small part of the order book changes
/// 
/// # Arguments
/// * `current_obi` - Current OBI value
/// * `old_bid_volume` - Volume being removed from bids
/// * `new_bid_volume` - Volume being added to bids
/// * `old_ask_volume` - Volume being removed from asks
/// * `new_ask_volume` - Volume being added to asks
/// * `total_volume` - Total volume in the order book
/// 
/// # Returns
/// Updated OBI value
pub fn update_obi_incremental(
    current_obi: f64,
    old_bid_volume: f64,
    new_bid_volume: f64,
    old_ask_volume: f64,
    new_ask_volume: f64,
    total_volume: f64,
) -> f64 {
    if total_volume == 0.0 {
        return 0.0;
    }
    
    let bid_change = new_bid_volume - old_bid_volume;
    let ask_change = new_ask_volume - old_ask_volume;
    
    let new_total_volume = total_volume + bid_change + ask_change;
    
    if new_total_volume == 0.0 {
        return 0.0;
    }
    
    // Recalculate OBI based on changes
    let numerator_change = bid_change - ask_change;
    let denominator_change = bid_change + ask_change;
    
    // This is a simplified approximation - in practice, you'd recalculate from scratch
    // for precision, but this gives good performance for high-frequency updates
    ((current_obi * total_volume) + numerator_change) / new_total_volume
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::OrderBookLevel;
    
    #[test]
    fn test_obi_calculation() {
        let order_book = OrderBookSnapshot {
            symbol: "BTCUSDT".to_string(),
            bids: vec![
                OrderBookLevel { price: 50000.0, quantity: 1.0 },
                OrderBookLevel { price: 49999.0, quantity: 2.0 },
                OrderBookLevel { price: 49998.0, quantity: 3.0 },
            ],
            asks: vec![
                OrderBookLevel { price: 50001.0, quantity: 1.5 },
                OrderBookLevel { price: 50002.0, quantity: 2.5 },
                OrderBookLevel { price: 50003.0, quantity: 3.5 },
            ],
            timestamp: 0,
        };
        
        let obi = calculate_obi(&order_book, 3);
        assert!(obi < 0.0); // More ask volume, so OBI should be negative
    }
    
    #[test]
    fn test_obi_extreme_values() {
        // All bids
        let order_book_bids = OrderBookSnapshot {
            symbol: "BTCUSDT".to_string(),
            bids: vec![OrderBookLevel { price: 50000.0, quantity: 10.0 }],
            asks: vec![],
            timestamp: 0,
        };
        
        let obi_bids = calculate_obi(&order_book_bids, 1);
        assert_eq!(obi_bids, 1.0);
        
        // All asks
        let order_book_asks = OrderBookSnapshot {
            symbol: "BTCUSDT".to_string(),
            bids: vec![],
            asks: vec![OrderBookLevel { price: 50000.0, quantity: 10.0 }],
            timestamp: 0,
        };
        
        let obi_asks = calculate_obi(&order_book_asks, 1);
        assert_eq!(obi_asks, -1.0);
    }
}