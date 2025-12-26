//! Error types for TrendLab.

use thiserror::Error;

/// Core error type for TrendLab operations.
#[derive(Error, Debug)]
pub enum TrendLabError {
    #[error("Data error: {0}")]
    Data(String),

    #[error("Strategy error: {0}")]
    Strategy(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Polars error: {0}")]
    Polars(#[from] polars::error::PolarsError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid configuration: {0}")]
    Config(String),
}

/// Result type alias for TrendLab operations.
pub type Result<T> = std::result::Result<T, TrendLabError>;
