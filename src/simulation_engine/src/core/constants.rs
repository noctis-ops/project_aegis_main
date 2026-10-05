//! Constants used throughout the Simulation Engine

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

/// Capital adaptation test matrix
pub const TEST_CAPITAL_SMALL: f64 = 100.0; // $100
pub const TEST_CAPITAL_MEDIUM: f64 = 50000.0; // $50,000
pub const TEST_CAPITAL_LARGE: f64 = 2000000.0; // $2,000,000

/// Robustness testing thresholds
pub const OVERFITTING_TEST_RANGE: f64 = 0.05; // ±5% around optimal parameters
pub const MIN_ACCEPTABLE_ROBUSTNESS: f64 = 0.8; // 80% of optimal performance

/// CI/CD pipeline constants
pub const CI_CD_TIMEOUT_SECONDS: u64 = 3600; // 1 hour
pub const DEPLOYMENT_RETRY_COUNT: usize = 3;
pub const HEALTH_CHECK_INTERVAL_MS: u64 = 5000; // 5 seconds

/// AWS deployment constants
pub const AWS_REGION: &str = "ap-northeast-1"; // Tokyo
pub const EC2_INSTANCE_TYPE: &str = "c6i.xlarge";
pub const DOCKER_IMAGE_TAG: &str = "latest";
