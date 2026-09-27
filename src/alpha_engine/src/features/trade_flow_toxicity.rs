//! Trade Flow Toxicity (VPIN) calculator

use crate::core::AggTrade;
use std::collections::VecDeque;

/// Volume Bucket for VPIN calculation
#[derive(Debug, Clone)]
pub struct VolumeBucket {
    pub buys: f64,
    pub sells: f64,
    pub total_volume: f64,
    pub timestamp: u64,
}

impl VolumeBucket {
    pub fn new() -> Self {
        Self {
            buys: 0.0,
            sells: 0.0,
            total_volume: 0.0,
            timestamp: 0,
        }
    }
    
    pub fn vpin(&self) -> f64 {
        if self.total_volume == 0.0 {
            return 0.0;
        }
        
        ((self.buys - self.sells).abs()) / self.total_volume
    }
    
    pub fn is_toxic(&self, threshold: f64) -> bool {
        self.vpin() > threshold
    }
}

/// Trade Flow Toxicity calculator
pub struct TradeFlowToxicity {
    buckets: VecDeque<VolumeBucket>,
    current_bucket: VolumeBucket,
    bucket_volume_threshold: f64,
    toxicity_threshold: f64,
}

impl TradeFlowToxicity {
    /// Create a new TradeFlowToxicity calculator
    /// 
    /// # Arguments
    /// * `bucket_volume_threshold` - Volume threshold to close a bucket
    /// * `toxicity_threshold` - VPIN threshold to consider flow toxic (typically 0.8-0.9)
    pub fn new(bucket_volume_threshold: f64, toxicity_threshold: f64) -> Self {
        Self {
            buckets: VecDeque::new(),
            current_bucket: VolumeBucket::new(),
            bucket_volume_threshold,
            toxicity_threshold,
        }
    }
    
    /// Process an aggregated trade
    pub fn process_trade(&mut self, trade: &AggTrade) {
        if trade.is_buyer_maker {
            // Taker buy (market buy order filled against limit sell)
            self.current_bucket.buys += trade.quantity;
        } else {
            // Taker sell (market sell order filled against limit buy)
            self.current_bucket.sells += trade.quantity;
        }
        
        self.current_bucket.total_volume += trade.quantity;
        
        // Check if we need to close the current bucket
        if self.current_bucket.total_volume >= self.bucket_volume_threshold {
            self.close_current_bucket(trade.timestamp);
        }
    }
    
    /// Close the current bucket and start a new one
    fn close_current_bucket(&mut self, timestamp: u64) {
        self.current_bucket.timestamp = timestamp;
        self.buckets.push_back(self.current_bucket.clone());
        
        // Keep only the last N buckets (e.g., 50)
        if self.buckets.len() > 50 {
            self.buckets.pop_front();
        }
        
        // Start new bucket
        self.current_bucket = VolumeBucket::new();
    }
    
    /// Calculate current VPIN across all buckets
    pub fn calculate_vpin(&self) -> f64 {
        if self.buckets.is_empty() {
            return 0.0;
        }
        
        let sum_vpin: f64 = self.buckets.iter().map(|bucket| bucket.vpin()).sum();
        sum_vpin / self.buckets.len() as f64
    }
    
    /// Check if current flow is toxic
    pub fn is_toxic(&self) -> bool {
        self.calculate_vpin() > self.toxicity_threshold
    }
    
    /// Check if flow is toxic in a specific direction
    /// 
    /// # Arguments
    /// * `side` - Direction to check (Buy = bullish, Sell = bearish)
    pub fn is_toxic_direction(&self, side: crate::core::TradeSide) -> bool {
        if self.buckets.is_empty() {
            return false;
        }
        
        // Calculate average buys and sells
        let avg_buys: f64 = self.buckets.iter().map(|bucket| bucket.buys).sum::<f64>() / self.buckets.len() as f64;
        let avg_sells: f64 = self.buckets.iter().map(|bucket| bucket.sells).sum::<f64>() / self.buckets.len() as f64;
        
        let dominance = if side == crate::core::TradeSide::Buy {
            avg_buys / (avg_buys + avg_sells)
        } else {
            avg_sells / (avg_buys + avg_sells)
        };
        
        // Consider toxic if one side dominates and overall VPIN is high
        dominance > 0.7 && self.is_toxic()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::AggTrade;
    
    #[test]
    fn test_vpin_calculation() {
        let mut toxicity = TradeFlowToxicity::new(100.0, 0.8);
        
        // Add trades heavily skewed toward sells
        for i in 0..10 {
            let trade = AggTrade {
                symbol: "BTCUSDT".to_string(),
                price: 50000.0,
                quantity: 15.0, // Total 150, above threshold of 100
                is_buyer_maker: false, // Taker sell
                timestamp: i,
            };
            toxicity.process_trade(&trade);
        }
        
        let vpin = toxicity.calculate_vpin();
        assert!(vpin > 0.0);
    }
}