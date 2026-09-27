//! Constants used throughout the HFT system

/// Heartbeat timeout in milliseconds
pub const HEARTBEAT_TIMEOUT_MS: u64 = 2000;

/// Maximum number of messages in ring buffer
pub const RING_BUFFER_SIZE: usize = 10_000;

/// Time to wait between snapshot requests (in seconds)
pub const SNAPSHOT_REQUEST_INTERVAL_SEC: u64 = 3;

/// Maximum depth of order book to maintain
pub const ORDER_BOOK_DEPTH: usize = 1000;

/// WebSocket URL for Binance Futures
pub const BINANCE_FUTURES_WS_URL: &str = "wss://fstream.binance.com/ws/";

/// REST API URL for Binance Futures
pub const BINANCE_FUTURES_REST_URL: &str = "https://fapi.binance.com";