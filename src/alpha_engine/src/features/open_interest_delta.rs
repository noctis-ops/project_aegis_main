//! Open Interest Delta calculator

/// Open Interest data point
#[derive(Debug, Clone)]
pub struct OpenInterestData {
    pub open_interest: f64,
    pub timestamp: u64,
}

/// Open Interest Delta analyzer
pub struct OpenInterestDelta {
    history: Vec<OpenInterestData>,
    max_history_points: usize,
}

impl OpenInterestDelta {
    /// Create a new OpenInterestDelta analyzer
    /// 
    /// # Arguments
    /// * `max_history_points` - Maximum number of historical points to keep
    pub fn new(max_history_points: usize) -> Self {
        Self {
            history: Vec::with_capacity(max_history_points),
            max_history_points,
        }
    }
    
    /// Add a new open interest data point
    pub fn add_data_point(&mut self, oi: f64, timestamp: u64) {
        self.history.push(OpenInterestData {
            open_interest: oi,
            timestamp,
        });
        
        // Maintain maximum history size
        if self.history.len() > self.max_history_points {
            self.history.remove(0);
        }
    }
    
    /// Calculate Open Interest Delta over a time period
    /// 
    /// # Arguments
    /// * `period_seconds` - Time period in seconds to calculate delta over
    /// 
    /// # Returns
    /// Delta value (positive = increasing OI, negative = decreasing OI)
    pub fn calculate_delta(&self, period_seconds: u64) -> Option<f64> {
        if self.history.len() < 2 {
            return None;
        }
        
        let current_time = self.history.last().unwrap().timestamp;
        let target_time = current_time.saturating_sub(period_seconds * 1000); // Convert to milliseconds
        
        // Find the data point closest to our target time
        let baseline_point = self.history.iter().rev().find(|data| data.timestamp <= target_time);
        
        if let Some(baseline) = baseline_point {
            let current = self.history.last().unwrap();
            Some(current.open_interest - baseline.open_interest)
        } else {
            // Not enough history, use first point
            let current = self.history.last().unwrap();
            let first = self.history.first().unwrap();
            Some(current.open_interest - first.open_interest)
        }
    }
    
    /// Analyze trend based on OI delta and price movement
    /// 
    /// # Arguments
    /// * `price_delta` - Change in price over the same period
    /// * `oi_delta` - Change in open interest over the same period
    /// 
    /// # Returns
    /// Trend interpretation
    pub fn analyze_trend(&self, price_delta: f64, oi_delta: f64) -> OITrend {
        match (price_delta > 0.0, oi_delta > 0.0) {
            (true, true) => OITrend::TrendConfirmation,   // Price up, OI up - valid trend
            (true, false) => OITrend::ShortSqueeze,       // Price up, OI down - potential squeeze
            (false, true) => OITrend::TrendContinuation,  // Price down, OI up - trend continuation
            (false, false) => OITrend::TrendExhaustion,   // Price down, OI down - trend ending
        }
    }
}

/// Open Interest trend interpretation
#[derive(Debug, Clone, PartialEq)]
pub enum OITrend {
    TrendConfirmation,  // Price and OI moving in same direction - strong trend
    ShortSqueeze,       // Price up but OI down - potential short covering
    TrendContinuation,  // Price down but OI up - trend likely to continue
    TrendExhaustion,    // Both price and OI declining - trend losing steam
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_oi_delta_calculation() {
        let mut oi_analyzer = OpenInterestDelta::new(100);
        
        // Add some data points
        oi_analyzer.add_data_point(1000.0, 1000);
        oi_analyzer.add_data_point(1100.0, 2000);
        oi_analyzer.add_data_point(1050.0, 3000);
        
        let delta = oi_analyzer.calculate_delta(2); // 2 seconds
        assert!(delta.is_some());
    }
    
    #[test]
    fn test_trend_analysis() {
        let oi_analyzer = OpenInterestDelta::new(100);
        
        // Price up, OI up - trend confirmation
        let trend1 = oi_analyzer.analyze_trend(100.0, 50.0);
        assert_eq!(trend1, OITrend::TrendConfirmation);
        
        // Price up, OI down - short squeeze
        let trend2 = oi_analyzer.analyze_trend(100.0, -50.0);
        assert_eq!(trend2, OITrend::ShortSqueeze);
    }
}