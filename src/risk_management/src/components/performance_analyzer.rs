//! Performance Analyzer implementation

use crate::core::{TradeRecord, PerformanceMetrics};
use dashmap::DashMap;
use tracing::{info, debug};
use std::sync::Arc;

/// Performance Analyzer
#[derive(Clone)]
pub struct PerformanceAnalyzer {
    trade_records: Arc<DashMap<String, TradeRecord>>,
    last_analysis_time: Arc<std::sync::RwLock<std::time::Instant>>,
}

impl PerformanceAnalyzer {
    /// Create a new Performance Analyzer
    pub fn new() -> Self {
        Self {
            trade_records: Arc::new(DashMap::new()),
            last_analysis_time: Arc::new(std::sync::RwLock::new(std::time::Instant::now())),
        }
    }
    
    /// Update with a new trade record
    pub fn update_with_trade(&self, record: &TradeRecord) {
        self.trade_records.insert(format!("{}_{}", record.symbol, record.entry_time), record.clone());
    }
    
    /// Calculate performance metrics
    pub fn calculate_metrics(&self, trade_records: &DashMap<String, TradeRecord>) -> PerformanceMetrics {
        let records: Vec<TradeRecord> = trade_records.iter().map(|entry| entry.value().clone()).collect();
        
        if records.is_empty() {
            return PerformanceMetrics {
                win_rate: 0.0,
                average_win: 0.0,
                average_loss: 0.0,
                profit_factor: 0.0,
                max_drawdown: 0.0,
                sharpe_ratio: 0.0,
                rolling_net_ev: 0.0,
            };
        }
        
        // Calculate basic statistics
        let total_trades = records.len() as f64;
        let winning_trades: Vec<&TradeRecord> = records.iter().filter(|t| t.is_winner).collect();
        let losing_trades: Vec<&TradeRecord> = records.iter().filter(|t| !t.is_winner).collect();
        
        let win_rate = winning_trades.len() as f64 / total_trades;
        
        let average_win = if !winning_trades.is_empty() {
            winning_trades.iter().map(|t| t.net_pnl).sum::<f64>() / winning_trades.len() as f64
        } else {
            0.0
        };
        
        let average_loss = if !losing_trades.is_empty() {
            losing_trades.iter().map(|t| t.net_pnl.abs()).sum::<f64>() / losing_trades.len() as f64
        } else {
            0.0
        };
        
        // Calculate profit factor
        let total_wins: f64 = winning_trades.iter().map(|t| t.net_pnl).sum();
        let total_losses: f64 = losing_trades.iter().map(|t| t.net_pnl.abs()).sum();
        let profit_factor = if total_losses > 0.0 {
            total_wins / total_losses
        } else if total_wins > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };
        
        // Calculate rolling net EV (last 100 trades)
        let rolling_net_ev = self.calculate_rolling_net_ev(&records, 100);
        
        // Calculate max drawdown
        let max_drawdown = self.calculate_max_drawdown(&records);
        
        // Calculate Sharpe ratio (simplified)
        let sharpe_ratio = self.calculate_sharpe_ratio(&records);
        
        debug!("Performance metrics calculated: Win Rate={:.2}%, Profit Factor={:.2}, Rolling NEV={:.4}", 
               win_rate * 100.0, profit_factor, rolling_net_ev);
        
        PerformanceMetrics {
            win_rate,
            average_win,
            average_loss,
            profit_factor,
            max_drawdown,
            sharpe_ratio,
            rolling_net_ev,
        }
    }
    
    /// Calculate rolling Net Expected Value
    fn calculate_rolling_net_ev(&self, records: &[TradeRecord], window_size: usize) -> f64 {
        let window_size = window_size.min(records.len());
        if window_size == 0 {
            return 0.0;
        }
        
        let recent_records: Vec<&TradeRecord> = records.iter().rev().take(window_size).collect();
        let total_ev: f64 = recent_records.iter()
            .map(|trade| trade.net_pnl) // Simplified - in reality we'd recalculate NEV
            .sum();
            
        total_ev / window_size as f64
    }
    
    /// Calculate maximum drawdown
    fn calculate_max_drawdown(&self, records: &[TradeRecord]) -> f64 {
        if records.is_empty() {
            return 0.0;
        }
        
        let mut peak = records[0].net_pnl;
        let mut max_dd = 0.0;
        
        for record in records.iter() {
            if record.net_pnl > peak {
                peak = record.net_pnl;
            }
            
            let drawdown = peak - record.net_pnl;
            if drawdown > max_dd {
                max_dd = drawdown;
            }
        }
        
        max_dd
    }
    
    /// Calculate Sharpe ratio (simplified)
    fn calculate_sharpe_ratio(&self, records: &[TradeRecord]) -> f64 {
        if records.len() < 2 {
            return 0.0;
        }
        
        let returns: Vec<f64> = records.iter().map(|t| t.net_pnl).collect();
        let mean_return: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
        
        let variance: f64 = returns.iter()
            .map(|r| (r - mean_return).powi(2))
            .sum::<f64>() / (returns.len() - 1) as f64;
            
        let std_dev = variance.sqrt();
        
        if std_dev > 0.0 {
            mean_return / std_dev
        } else {
            0.0
        }
    }
    
    /// Check for silent bleed condition
    pub fn check_silent_bleed(&self, metrics: &PerformanceMetrics) -> bool {
        // Check if win rate is low but system appears profitable
        // Or if rolling NEV is negative
        metrics.win_rate < 0.55 || metrics.rolling_net_ev < 0.0
    }
    
    /// Get trade records count
    pub fn get_trade_count(&self) -> usize {
        self.trade_records.len()
    }
}