//! Core data types for the Risk Management System

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Risk management configuration
#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub default_risk_percentage: f64,
    pub max_consecutive_losses: usize,
    pub daily_drawdown_limit: f64,
    pub weekly_drawdown_limit: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            default_risk_percentage: 0.01, // 1%
            max_consecutive_losses: 3,
            daily_drawdown_limit: 0.02, // 2%
            weekly_drawdown_limit: 0.05, // 5%
        }
    }
}

/// Portfolio state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioState {
    pub equity: f64,
    pub available_balance: f64,
    pub total_exposure: f64,
    pub floating_pnl: f64,
    pub realized_pnl: f64,
    pub timestamp: u64,
}

/// Trade execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub symbol: String,
    pub side: TradeSide,
    pub entry_price: f64,
    pub exit_price: f64,
    pub quantity: f64,
    pub entry_time: u64,
    pub exit_time: u64,
    pub fees: f64,
    pub slippage: f64,
    pub net_pnl: f64,
    pub is_winner: bool,
}

/// Trade side enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TradeSide {
    Buy,
    Sell,
}

/// Performance metrics
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub win_rate: f64,
    pub average_win: f64,
    pub average_loss: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
    pub rolling_net_ev: f64,
}

/// Circuit breaker status
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerStatus {
    Normal,        // Green - Normal operation
    Warning,       // Yellow - Reduced risk
    HaltEntries,   // Orange - Halt new entries
    KillSwitch,    // Red - Full shutdown
}

/// Risk adjustment event
#[derive(Debug, Clone)]
pub struct RiskAdjustmentEvent {
    pub reason: String,
    pub old_risk_percentage: f64,
    pub new_risk_percentage: f64,
    pub timestamp: u64,
}

/// Funding rate information
#[derive(Debug, Clone)]
pub struct FundingRateInfo {
    pub symbol: String,
    pub rate: f64,
    pub next_funding_time: u64,
}

/// Volatility metrics
#[derive(Debug, Clone)]
pub struct VolatilityMetrics {
    pub current_volatility: f64,
    pub average_volatility: f64,
    pub volatility_ratio: f64, // current/average
    pub is_anomalous: bool,
}

/// Telemetry data
#[derive(Debug, Clone)]
pub struct TelemetryData {
    pub latency_ms: f64,
    pub order_rejection_rate: f64,
    pub active_positions: usize,
    pub total_exposure: f64,
    pub equity_curve: Vec<(u64, f64)>, // timestamp, equity
    pub trade_history: VecDeque<TradeRecord>,
}

/// Recovery state
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryState {
    Normal,
    Cooldown,
    CautiousResumption { successful_trades: usize },
}

/// Risk metrics for MAE/MFE tracking
#[derive(Debug, Clone)]
pub struct MaeMfeMetrics {
    pub trade_id: String,
    pub max_adverse_excursion: f64,
    pub max_favorable_excursion: f64,
    pub stop_loss_level: f64,
    pub is_stop_loss_breached: bool,
}