//! Core data types for the Simulation Engine

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Simulation configuration
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub data_path: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub playback_speed: f64,
    pub initial_capital: f64,
    pub leverage: f64,
    pub symbols: Vec<String>,
}

/// Market event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    OrderBookSnapshot(OrderBookSnapshot),
    Trade(TradeEvent),
    FundingRate(FundingRateEvent),
}

/// Order book snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub symbol: String,
    pub timestamp: i64,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
}

/// Price level in order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: f64,
    pub quantity: f64,
}

/// Trade event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    pub symbol: String,
    pub timestamp: i64,
    pub price: f64,
    pub quantity: f64,
    pub side: TradeSide,
}

/// Trade side enumeration
///
/// `Copy` because it is a fieldless tag that every fill record needs in order to
/// *keep* a value rather than borrow one: the shadow ledger reads
/// `intent.side` out of a borrowed [`TradeIntent`] while recording a trade, and
/// without `Copy` that access is a move out of a shared reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    Buy,
    Sell,
}

/// A signal to open (or flip) a position: the unit the shadow engine consumes.
///
/// This is the same struct the execution layer passes around
/// (`alpha_engine::core::TradeIntent`), re-declared here because the layers are
/// independent crates - the simulated path has to accept exactly what the live
/// path produces. It is deliberately *not* [`TradeEvent`]: a `TradeEvent` is a
/// trade printed by the market in the replay feed and carries no client size,
/// no stop-loss and no time-to-live, so it cannot drive queue-position or PnL
/// math.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeIntent {
    pub symbol: String,
    pub side: TradeSide,
    pub price: f64,
    pub size: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub time_to_live: u64, // milliseconds
    /// Signal time in milliseconds since the epoch, as produced by the alpha
    /// engine. The shadow ledger keeps this unit instead of narrowing it to the
    /// seconds used by the market events above.
    pub timestamp: u64,
}

/// Funding rate event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateEvent {
    pub symbol: String,
    pub timestamp: i64,
    pub rate: f64,
}

/// Simulation result
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub total_return: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub total_trades: usize,
    pub avg_trade_duration: f64,
    pub slippage_impact: f64,
    pub execution_quality: f64,
}

/// Backtest report
#[derive(Debug, Clone)]
pub struct BacktestReport {
    pub config: SimulationConfig,
    pub results: SimulationResult,
    pub equity_curve: Vec<(i64, f64)>,
    pub trade_log: Vec<TradeLogEntry>,
    pub timestamp: DateTime<Utc>,
}

/// Trade log entry
#[derive(Debug, Clone)]
pub struct TradeLogEntry {
    pub timestamp: i64,
    pub symbol: String,
    pub action: TradeAction,
    pub price: f64,
    pub quantity: f64,
    pub slippage: f64,
    pub fees: f64,
    pub pnl: f64,
}

/// Trade action enumeration
#[derive(Debug, Clone)]
pub enum TradeAction {
    Entry,
    Exit,
    StopLoss,
    TakeProfit,
}

/// Data feeder status
#[derive(Debug, Clone, PartialEq)]
pub enum DataFeederStatus {
    Idle,
    Running,
    Paused,
    Completed,
    Error,
}

/// Execution simulation parameters
#[derive(Debug, Clone)]
pub struct ExecutionSimParams {
    pub min_latency_ms: u64,
    pub max_latency_ms: u64,
    pub queue_position_modeling: bool,
    pub volume_through_fill_logic: bool,
    pub market_impact_modeling: bool,
}

/// Capital adaptation test case
#[derive(Debug, Clone)]
pub struct CapitalAdaptationTestCase {
    pub capital: f64,
    pub leverage: f64,
    pub expected_min_notional_check: bool,
    pub market_impact_threshold: f64,
}

/// Robustness test parameters
#[derive(Debug, Clone)]
pub struct RobustnessTestParams {
    pub parameter_ranges: Vec<(String, f64, f64)>, // (param_name, min, max)
    pub test_points: usize,
    pub min_acceptable_performance: f64,
}
