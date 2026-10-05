//! Constants used throughout the Simulation Engine

/// Lower bound for a plausible millisecond epoch (2001-09-09).
///
/// Data below this is almost certainly a *seconds* epoch. That mistake is not
/// harmless: every row would land outside the requested window and the run would
/// report "the strategy found nothing" instead of "the feed is in the wrong units".
pub const MIN_EPOCH_MILLIS: i64 = 1_000_000_000_000;

/// Simulation timing constants
pub const MIN_SIMULATION_TIMESTAMP: i64 = 0;
pub const MAX_SIMULATION_TIMESTAMP: i64 = i64::MAX;
pub const DEFAULT_PLAYBACK_SPEED: f64 = 1.0; // 1x speed
pub const MIN_PLAYBACK_SPEED: f64 = 0.1; // 0.1x speed
pub const MAX_PLAYBACK_SPEED: f64 = 100.0; // 100x speed

/// Data lake constants
pub const PARQUET_FILE_EXTENSION: &str = ".parquet";
pub const DEFAULT_BATCH_SIZE: usize = 1000;
pub const MAX_BATCH_SIZE: usize = 10000;

/// Latency simulation constants
pub const MIN_LATENCY_MS: u64 = 2;
pub const MAX_LATENCY_MS: u64 = 10;
pub const DEFAULT_LATENCY_MS: u64 = 5;

/// Simulated strategy constants
///
/// These mirror the live path rather than inventing simulation-only behaviour: the
/// cooldown, TTL and stop/target geometry are what Layer 2's signal generator uses,
/// and `MIN_NOTIONAL_USDT` matches `execution_engine`'s exchange floor.
pub const SIM_SIGNAL_COOLDOWN_MS: u64 = 1000;
pub const SIM_ORDER_TTL_MS: u64 = 200;
pub const SIM_STOP_DISTANCE_PCT: f64 = 0.005; // 0.5% stop, as Layer 2 places it
pub const SIM_TAKE_PROFIT_PCT: f64 = 0.01; // 1% target
pub const MIN_NOTIONAL_USDT: f64 = 5.0;

/// Fees charged per fill by default (Binance USDⓈ-M: 0.02% maker, 0.04% taker).
pub const DEFAULT_MAKER_FEE_RATE: f64 = 0.0002;
pub const DEFAULT_TAKER_FEE_RATE: f64 = 0.0004;

/// Profit factor is wins/losses; with no losing trade the ratio is unbounded, and
/// `f64::INFINITY` cannot be serialized by `serde_json` (the dashboard reads these
/// reports), so it is reported capped at a finite, obviously-excellent value.
pub const MAX_PROFIT_FACTOR: f64 = 999.0;

/// Annualization basis for the simulated Sharpe ratio.
pub const SECONDS_PER_YEAR: f64 = 365.0 * 24.0 * 3600.0;

/// Capital adaptation test matrix
pub const TEST_CAPITAL_SMALL: f64 = 100.0; // $100
pub const TEST_CAPITAL_MEDIUM: f64 = 50000.0; // $50,000
pub const TEST_CAPITAL_LARGE: f64 = 2000000.0; // $2,000,000

/// Robustness testing thresholds
pub const OVERFITTING_TEST_RANGE: f64 = 0.05; // ±5% around optimal parameters

/// How much of its return a strategy may lose when a tuned knob is nudged and still
/// be considered genuine. Past this, the measured edge is a spike at one parameter
/// value rather than a plateau, which is what overfitting looks like in a backtest.
pub const MAX_ACCEPTABLE_PARAMETER_DEGRADATION: f64 = 0.5;
pub const MIN_ACCEPTABLE_ROBUSTNESS: f64 = 0.8; // 80% of optimal performance

/// CI/CD pipeline constants
pub const CI_CD_TIMEOUT_SECONDS: u64 = 3600; // 1 hour
pub const DEPLOYMENT_RETRY_COUNT: usize = 3;
pub const HEALTH_CHECK_INTERVAL_MS: u64 = 5000; // 5 seconds

/// AWS deployment constants
pub const AWS_REGION: &str = "ap-northeast-1"; // Tokyo
pub const EC2_INSTANCE_TYPE: &str = "c6i.xlarge";
pub const DOCKER_IMAGE_TAG: &str = "latest";
