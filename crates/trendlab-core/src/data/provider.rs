//! Data provider traits and types.
//!
//! Defines the contract for fetching market data from external sources.
//! Actual implementations with network I/O live in `trendlab-cli`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur when fetching or parsing provider data.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Symbol not found: {symbol}")]
    SymbolNotFound { symbol: String },

    #[error("Parse error: {message}")]
    ParseError { message: String },

    #[error("Invalid date range: {start} to {end}")]
    InvalidDateRange { start: NaiveDate, end: NaiveDate },

    #[error("Network error: {message}")]
    NetworkError { message: String },

    #[error("Cache error: {message}")]
    CacheError { message: String },

    #[error("IO error: {message}")]
    IoError { message: String },
}

impl From<std::io::Error> for ProviderError {
    fn from(e: std::io::Error) -> Self {
        ProviderError::IoError {
            message: e.to_string(),
        }
    }
}

/// Request parameters for fetching data from a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchRequest {
    /// Ticker symbol (e.g., "SPY", "AAPL")
    pub symbol: String,

    /// Start date (inclusive)
    pub start: NaiveDate,

    /// End date (inclusive)
    pub end: NaiveDate,

    /// Timeframe (e.g., "1d", "1h")
    pub timeframe: String,

    /// If true, bypass cache and fetch fresh data
    pub force: bool,
}

impl FetchRequest {
    /// Create a new fetch request for daily data.
    pub fn daily(symbol: impl Into<String>, start: NaiveDate, end: NaiveDate) -> Self {
        Self {
            symbol: symbol.into(),
            start,
            end,
            timeframe: "1d".to_string(),
            force: false,
        }
    }

    /// Set the force flag to bypass cache.
    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }
}

/// Metadata sidecar for cached raw data.
///
/// Stored alongside raw data files to track provenance and enable cache validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// Provider name (e.g., "yahoo")
    pub provider: String,

    /// Symbol fetched
    pub symbol: String,

    /// Requested start date
    pub start: NaiveDate,

    /// Requested end date
    pub end: NaiveDate,

    /// Timeframe
    pub timeframe: String,

    /// When the data was fetched (UTC)
    pub fetched_at: DateTime<Utc>,

    /// Number of rows in the data file
    pub row_count: usize,

    /// SHA256 checksum of the data file
    pub checksum: String,

    /// Schema version for forward compatibility
    pub schema_version: u32,
}

impl CacheMetadata {
    /// Current schema version for cache metadata.
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Create new cache metadata.
    pub fn new(
        provider: impl Into<String>,
        symbol: impl Into<String>,
        start: NaiveDate,
        end: NaiveDate,
        timeframe: impl Into<String>,
        row_count: usize,
        checksum: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            symbol: symbol.into(),
            start,
            end,
            timeframe: timeframe.into(),
            fetched_at: Utc::now(),
            row_count,
            checksum: checksum.into(),
            schema_version: Self::CURRENT_SCHEMA_VERSION,
        }
    }

    /// Generate the cache file path (relative to data/raw/).
    ///
    /// Format: `{provider}/{symbol}/{start}_{end}.csv`
    pub fn cache_path(&self) -> String {
        format!(
            "{}/{}/{}_{}.csv",
            self.provider, self.symbol, self.start, self.end
        )
    }

    /// Generate the metadata sidecar path (relative to data/raw/).
    ///
    /// Format: `{provider}/{symbol}/{start}_{end}.meta.json`
    pub fn metadata_path(&self) -> String {
        format!(
            "{}/{}/{}_{}.meta.json",
            self.provider, self.symbol, self.start, self.end
        )
    }
}

/// Source of data (cache hit vs fresh fetch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSource {
    /// Data was loaded from cache.
    Cache,
    /// Data was freshly fetched from the provider.
    Fresh,
}

/// Result of a fetch operation.
#[derive(Debug, Clone)]
pub struct FetchResult {
    /// The fetched bars.
    pub bars: Vec<crate::Bar>,

    /// Where the data came from.
    pub source: DataSource,

    /// Cache metadata (if data was cached).
    pub metadata: Option<CacheMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_request_daily() {
        let req = FetchRequest::daily(
            "SPY",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
        );

        assert_eq!(req.symbol, "SPY");
        assert_eq!(req.timeframe, "1d");
        assert!(!req.force);
    }

    #[test]
    fn test_cache_metadata_paths() {
        let meta = CacheMetadata::new(
            "yahoo",
            "SPY",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            "1d",
            252,
            "abc123",
        );

        assert_eq!(meta.cache_path(), "yahoo/SPY/2024-01-01_2024-12-31.csv");
        assert_eq!(
            meta.metadata_path(),
            "yahoo/SPY/2024-01-01_2024-12-31.meta.json"
        );
    }
}
