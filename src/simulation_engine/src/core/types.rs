//! Core data types for the Simulation Engine

use crate::core::SimulationError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Simulation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// Where recorded events live (see `components::data_lake` for the accepted files).
    pub data_path: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    /// Validated pacing setting for an *external* feeder pushing events into an active
    /// run. The replay itself ignores it on purpose: a backtest consumes a fixed event
    /// sequence as fast as it can, and determinism - not wall-clock fidelity - is what
    /// makes two runs over the same files comparable.
    pub playback_speed: f64,
    pub initial_capital: f64,
    pub leverage: f64,
    pub symbols: Vec<String>,
    /// The strategy the simulated path trades. Part of the config, not a global,
    /// because a backtest is only reproducible if the rule it tested is recorded
    /// with it (and because the robustness sweep perturbs exactly these values).
    pub strategy: StrategyParams,
    /// Fee the simulation charges per fill. Binance USDⓈ-M futures quotes 0.02%
    /// maker / 0.04% taker; entries queue as makers and stop/target exits cross, so
    /// both rates matter and neither may be left out.
    pub maker_fee_rate: f64,
    pub taker_fee_rate: f64,
}

impl SimulationConfig {
    /// Replay everything the data lake holds.
    ///
    /// `default()` sets `start_time == end_time == Utc::now()`, which is the right shape
    /// for a test that overwrites both and a broken shape for a launcher that does not:
    /// a zero-length window silently selects no events, and the replay would then report
    /// "no events in window" about data that is sitting on disk. So a run without an
    /// explicit window spans the range the reader accepts - from `MIN_EPOCH_MILLIS` (the
    /// sanity floor that separates milliseconds from seconds) to the year 2100, which is
    /// the far future the lake's own tests use and still inside chrono's range.
    pub fn whole_lake(data_path: impl Into<String>) -> Self {
        // `TimeZone` supplies `timestamp_millis_opt`; `Utc` is already imported above.
        use chrono::TimeZone;

        let to_millis = |value: i64, constant: &str| {
            Utc.timestamp_millis_opt(value)
                .single()
                .unwrap_or_else(|| panic!("{} is outside chrono's timestamp range", constant))
        };

        Self {
            data_path: data_path.into(),
            start_time: to_millis(crate::core::constants::MIN_EPOCH_MILLIS, "MIN_EPOCH_MILLIS"),
            end_time: to_millis(FAR_FUTURE_MILLIS, "FAR_FUTURE_MILLIS"),
            ..Self::default()
        }
    }
}

/// The end of the window `SimulationConfig::whole_lake` uses: far enough to be "no upper
/// bound" inside chrono's range, and the same year 2100 the data lake tests assume.
const FAR_FUTURE_MILLIS: i64 = 4_102_444_800_000;

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            data_path: "./data".to_string(),
            start_time: chrono::Utc::now(),
            end_time: chrono::Utc::now(),
            playback_speed: crate::core::constants::DEFAULT_PLAYBACK_SPEED,
            initial_capital: 10000.0,
            leverage: 10.0,
            symbols: vec!["BTCUSDT".to_string()],
            strategy: StrategyParams::default(),
            maker_fee_rate: crate::core::constants::DEFAULT_MAKER_FEE_RATE,
            taker_fee_rate: crate::core::constants::DEFAULT_TAKER_FEE_RATE,
        }
    }
}

/// Strategy knobs the simulated path trades on.
///
/// The decision rule and these defaults mirror Layer 2
/// (`alpha_engine::logic::signal_generator` / `position_sizing`) on purpose: a
/// backtest of a *different* rule than the one that trades live tells you nothing
/// about your live PnL. Keeping them here (rather than importing Layer 2) is what
/// lets a robustness run perturb them one at a time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParams {
    /// |OBI| required to act. Layer 2 uses 0.4.
    pub obi_threshold: f64,
    /// Top-of-book levels in the imbalance ratio. Layer 2 uses 5.
    pub obi_levels: usize,
    /// Share of equity risked between entry and stop. Layer 2 uses 0.01.
    pub risk_percentage: f64,
    /// Trade prints sampled to estimate realized volatility.
    pub volatility_window: usize,
    /// Refuse new entries while sampled volatility exceeds this (0 disables the gate).
    pub max_volatility: f64,
    /// Quantity step the exchange accepts (BTCUSDT: 0.001).
    pub size_step: f64,
    /// Absolute cap on position notional, as Layer 2 caps it.
    pub max_position_value: f64,
}

impl Default for StrategyParams {
    fn default() -> Self {
        Self {
            obi_threshold: 0.4,
            obi_levels: 5,
            risk_percentage: 0.01,
            volatility_window: 100,
            max_volatility: 0.0,
            size_step: 0.001,
            max_position_value: 10000.0,
        }
    }
}

impl StrategyParams {
    /// The knobs a robustness or overfitting sweep may name.
    pub const SWEPT_KNOBS: &'static [&'static str] = &[
        "obi_threshold",
        "obi_levels",
        "volatility_window",
        "risk_percentage",
        "max_position_value",
        "size_step",
    ];

    /// Read a knob by name, for tests that need the current value to perturb around it.
    pub fn knob(&self, name: &str) -> Option<f64> {
        let value = match name {
            "obi_threshold" => self.obi_threshold,
            "obi_levels" => self.obi_levels as f64,
            "volatility_window" => self.volatility_window as f64,
            "risk_percentage" => self.risk_percentage,
            "max_volatility" => self.max_volatility,
            "max_position_value" => self.max_position_value,
            "size_step" => self.size_step,
            _ => return None,
        };
        Some(value)
    }

    /// Write a knob by name.
    ///
    /// Unknown names are an error, not a warning to be logged and ignored: re-running
    /// the unchanged config and counting it as a test point is how a sweep reports a
    /// pass it never measured. Note `max_volatility` is readable but not listed as
    /// sweepable: it is a gate, and sweeping a gate that defaults to "disabled" only
    /// produces identical runs.
    pub fn set_knob(&mut self, name: &str, value: f64) -> Result<(), SimulationError> {
        match name {
            "obi_threshold" => self.obi_threshold = value,
            // Count-valued knobs are rounded up to at least one: a zero-level OBI or a
            // zero-sample volatility window is not a parameter, it is a division by
            // nothing.
            "obi_levels" => self.obi_levels = value.max(1.0).round() as usize,
            "volatility_window" => self.volatility_window = value.max(1.0).round() as usize,
            "risk_percentage" => self.risk_percentage = value,
            "max_position_value" => self.max_position_value = value,
            "size_step" => self.size_step = value,
            other => {
                return Err(SimulationError::InvalidConfig(format!(
                    "The simulation applies no strategy parameter named '{}'; sweep one of {}",
                    other,
                    Self::SWEPT_KNOBS.join(", ")
                )))
            }
        }
        Ok(())
    }
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestReport {
    pub config: SimulationConfig,
    pub results: SimulationResult,
    pub equity_curve: Vec<(i64, f64)>,
    pub trade_log: Vec<TradeLogEntry>,
    pub timestamp: DateTime<Utc>,
}

/// Trade log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
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
