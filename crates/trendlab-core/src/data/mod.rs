//! Data layer: providers, caching, normalization, and quality checks.
//!
//! This module handles:
//! - Fetching raw OHLCV data from providers (Yahoo Finance, etc.)
//! - Caching raw responses with metadata
//! - Normalizing to canonical Parquet format
//! - Data quality validation and reporting

mod parquet;
mod provider;
mod quality;
mod yahoo;

pub use parquet::{
    bars_to_dataframe, dataframe_to_bars, parquet_path, partition_by_year, read_parquet,
    scan_multiple_parquet_lazy, scan_parquet_lazy, scan_symbol_parquet_lazy, write_parquet,
    write_partitioned_parquet,
};
pub use provider::{CacheMetadata, DataSource, FetchRequest, FetchResult, ProviderError};
pub use quality::{DataQualityChecker, DataQualityReport, QualityIssue};
pub use yahoo::{build_yahoo_chart_url, build_yahoo_url, parse_yahoo_chart_json, parse_yahoo_csv};
