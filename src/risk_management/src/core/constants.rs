//! Constants used throughout the Risk Management System

/// Circuit breaker thresholds
pub const YELLOW_ALERT_LATENCY_MS: u64 = 50;
pub const YELLOW_ALERT_REJECTION_RATE: f64 = 0.05; // 5%
pub const ORANGE_HALT_DAILY_DRAWDOWN: f64 = 0.02; // 2%
pub const RED_KILLSWITCH_FLASH_CRASH_THRESHOLD: f64 = 0.03; // 3%
pub const RED_KILLSWITCH_USER_DATA_TIMEOUT_MS: u64 = 5000; // 5 seconds
pub const RED_KILLSWITCH_WEEKLY_DRAWDOWN: f64 = 0.05; // 5%

/// Risk management parameters
pub const DEFAULT_RISK_PERCENTAGE: f64 = 0.01; // 1%
pub const CONSECUTIVE_LOSS_DAMPENER: f64 = 0.8; // Reduce risk by 20% after each loss
pub const RECOVERY_RISK_PERCENTAGE: f64 = 0.0025; // 0.25% during cautious recovery

/// Telemetry sampling
pub const TELEMETRY_SAMPLING_INTERVAL_MS: u64 = 100; // Sample every 100ms
pub const METRICS_PUSH_INTERVAL_MS: u64 = 1000; // Push metrics every second

/// Funding rate thresholds
pub const MAX_NEGATIVE_FUNDING_RATE: f64 = -0.0005; // -0.05%
pub const FUNDING_COST_MULTIPLIER: f64 = 3.0; // 3x multiplier for alpha to cover funding

/// Volatility anomaly detection
pub const VOLATILITY_ANOMALY_THRESHOLD: f64 = 4.0; // 400% increase
pub const VOLATILITY_WINDOW_SECONDS: u64 = 10;

/// Recovery protocol
pub const COOLDOWN_PERIOD_SECONDS: u64 = 900; // 15 minutes
pub const CAUTIOUS_RESUMPTION_RISK: f64 = 0.0025; // 0.25%
pub const SUCCESSFUL_TRADES_FOR_FULL_RECOVERY: usize = 5;

/// Stop loss logic
pub const STOP_LOSS_CONFIRMATION_TIME_MS: u64 = 200; // 200ms confirmation