//! Custom error types for the Simulation Engine

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SimulationError {
    #[error("Data loading error: {0}")]
    DataLoadingError(String),
    
    #[error("Parquet file error: {0}")]
    ParquetError(#[from] parquet::errors::ParquetError),
    
    #[error("Simulation timeout exceeded: {0}")]
    TimeoutError(String),
    
    #[error("Invalid simulation configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Timestamp violation: {0}")]
    TimestampViolation(String),
    
    #[error("Look-ahead bias detected: {0}")]
    LookAheadBias(String),
    
    #[error("Execution simulation error: {0}")]
    ExecutionError(String),
    
    #[error("Capital adaptation test failed: {0}")]
    CapitalAdaptationFailed(String),
    
    #[error("Robustness test failed: {0}")]
    RobustnessTestFailed(String),
    
    #[error("CI/CD pipeline error: {0}")]
    PipelineError(String),
    
    #[error("Deployment error: {0}")]
    DeploymentError(String),
    
    #[error("Other error: {0}")]
    Other(String),
}