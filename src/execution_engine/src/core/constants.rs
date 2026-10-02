//! Constants used throughout the Execution Engine

/// WebSocket URL for Binance Futures User Data Stream
pub const BINANCE_USER_DATA_STREAM_URL: &str = "wss://fstream.binance.com/ws/";

/// REST API URL for Binance Futures
pub const BINANCE_FUTURES_REST_URL: &str = "https://fapi.binance.com";

/// WebSocket URL for Binance Futures Order API
pub const BINANCE_ORDER_API_URL: &str = "wss://fstream.binance.com/ws-api/v1";

/// Heartbeat timeout for user data stream in milliseconds
pub const USER_DATA_STREAM_TIMEOUT_MS: u64 = 3000;

/// Rate limit thresholds
pub const MAX_REQUESTS_PER_SECOND: u64 = 10;
pub const MAX_WEIGHT_PER_MINUTE: u64 = 2400;

/// Order constraints
pub const MIN_NOTIONAL: f64 = 5.0; // USDT
pub const DEFAULT_LEVERAGE: u32 = 50;

/// Time-to-live for orders in milliseconds
pub const ORDER_TTL_MS: u64 = 200;

/// How often the order-timeout sweep runs (orders carry ~200ms TTLs, so a
/// 100ms sweep enforces them with at most 100ms of extra latency).
pub const ORDER_TIMEOUT_CHECK_INTERVAL_MS: u64 = 100;

/// Tick size for price adjustments
pub const DEFAULT_TICK_SIZE: f64 = 0.1;