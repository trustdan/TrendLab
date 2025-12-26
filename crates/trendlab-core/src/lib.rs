//! TrendLab Core - Domain types, strategies, and metrics for trend-following research.
//!
//! This crate provides:
//! - Bar and OHLCV data types
//! - Strategy trait and implementations
//! - Performance metrics calculations
//! - Data provider traits

pub mod bar;
pub mod error;
pub mod metrics;
pub mod strategy;

pub use bar::Bar;
pub use error::TrendLabError;

/// Re-export commonly used types
pub mod prelude {
    pub use crate::bar::Bar;
    pub use crate::error::TrendLabError;
    pub use crate::strategy::Strategy;
}
