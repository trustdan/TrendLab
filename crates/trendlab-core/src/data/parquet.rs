//! Parquet I/O helpers for normalized market data.
//!
//! Provides functions for writing bars to partitioned Parquet files
//! and reading them back with lazy scans.

use crate::bar::Bar;
use crate::data::ProviderError;
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use polars::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// Convert bars to a Polars DataFrame.
pub fn bars_to_dataframe(bars: &[Bar]) -> Result<DataFrame, ProviderError> {
    if bars.is_empty() {
        // Return empty DataFrame with correct schema
        return DataFrame::new(vec![
            Series::new("ts".into(), Vec::<i64>::new()).into(),
            Series::new("open".into(), Vec::<f64>::new()).into(),
            Series::new("high".into(), Vec::<f64>::new()).into(),
            Series::new("low".into(), Vec::<f64>::new()).into(),
            Series::new("close".into(), Vec::<f64>::new()).into(),
            Series::new("volume".into(), Vec::<f64>::new()).into(),
            Series::new("symbol".into(), Vec::<String>::new()).into(),
            Series::new("timeframe".into(), Vec::<String>::new()).into(),
        ])
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        });
    }

    let ts: Vec<i64> = bars.iter().map(|b| b.ts.timestamp_millis()).collect();
    let open: Vec<f64> = bars.iter().map(|b| b.open).collect();
    let high: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let low: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let close: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let volume: Vec<f64> = bars.iter().map(|b| b.volume).collect();
    let symbol: Vec<String> = bars.iter().map(|b| b.symbol.clone()).collect();
    let timeframe: Vec<String> = bars.iter().map(|b| b.timeframe.clone()).collect();

    // Create timestamp column as datetime
    let ts_series = Series::new("ts".into(), ts)
        .cast(&DataType::Datetime(
            TimeUnit::Milliseconds,
            Some("UTC".into()),
        ))
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;

    DataFrame::new(vec![
        ts_series.into(),
        Series::new("open".into(), open).into(),
        Series::new("high".into(), high).into(),
        Series::new("low".into(), low).into(),
        Series::new("close".into(), close).into(),
        Series::new("volume".into(), volume).into(),
        Series::new("symbol".into(), symbol).into(),
        Series::new("timeframe".into(), timeframe).into(),
    ])
    .map_err(|e| ProviderError::ParseError {
        message: e.to_string(),
    })
}

/// Convert a DataFrame back to bars.
pub fn dataframe_to_bars(df: &DataFrame) -> Result<Vec<Bar>, ProviderError> {
    let n = df.height();
    let mut bars = Vec::with_capacity(n);

    let ts_col = df
        .column("ts")
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?
        .datetime()
        .map_err(|e| ProviderError::ParseError {
            message: format!("ts column is not datetime: {}", e),
        })?;

    let open_col = df
        .column("open")
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?
        .f64()
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;

    let high_col = df
        .column("high")
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?
        .f64()
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;

    let low_col = df
        .column("low")
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?
        .f64()
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;

    let close_col = df
        .column("close")
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?
        .f64()
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;

    let volume_col = df
        .column("volume")
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?
        .f64()
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;

    let symbol_col = df
        .column("symbol")
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?
        .str()
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;

    let timeframe_col = df
        .column("timeframe")
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?
        .str()
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;

    for i in 0..n {
        let ts_ms = ts_col.get(i).ok_or_else(|| ProviderError::ParseError {
            message: format!("Null timestamp at row {}", i),
        })?;

        let ts =
            Utc.timestamp_millis_opt(ts_ms)
                .single()
                .ok_or_else(|| ProviderError::ParseError {
                    message: format!("Invalid timestamp {} at row {}", ts_ms, i),
                })?;

        bars.push(Bar::new(
            ts,
            open_col.get(i).unwrap_or(0.0),
            high_col.get(i).unwrap_or(0.0),
            low_col.get(i).unwrap_or(0.0),
            close_col.get(i).unwrap_or(0.0),
            volume_col.get(i).unwrap_or(0.0),
            symbol_col.get(i).unwrap_or(""),
            timeframe_col.get(i).unwrap_or(""),
        ));
    }

    Ok(bars)
}

/// Group bars by year for partitioned storage.
pub fn partition_by_year(bars: &[Bar]) -> HashMap<i32, Vec<Bar>> {
    let mut partitions: HashMap<i32, Vec<Bar>> = HashMap::new();

    for bar in bars {
        let year = bar.ts.year();
        partitions.entry(year).or_default().push(bar.clone());
    }

    partitions
}

/// Generate the Parquet file path for a given symbol, timeframe, and year.
///
/// Format: `{timeframe}/symbol={symbol}/year={year}/data.parquet`
pub fn parquet_path(timeframe: &str, symbol: &str, year: i32) -> String {
    format!("{}/symbol={}/year={}/data.parquet", timeframe, symbol, year)
}

/// Write bars to a Parquet file.
///
/// # Arguments
/// * `bars` - Bars to write
/// * `path` - Output file path
///
/// # Note
/// Creates parent directories if they don't exist.
pub fn write_parquet(bars: &[Bar], path: &Path) -> Result<(), ProviderError> {
    if bars.is_empty() {
        return Ok(());
    }

    // Create parent directories
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut df = bars_to_dataframe(bars)?;

    let file = std::fs::File::create(path)?;
    ParquetWriter::new(file)
        .finish(&mut df)
        .map_err(|e| ProviderError::IoError {
            message: format!("Failed to write Parquet: {}", e),
        })?;

    Ok(())
}

/// Read bars from a Parquet file.
pub fn read_parquet(path: &Path) -> Result<Vec<Bar>, ProviderError> {
    let df = LazyFrame::scan_parquet(path, ScanArgsParquet::default())
        .map_err(|e| ProviderError::IoError {
            message: format!("Failed to scan Parquet: {}", e),
        })?
        .collect()
        .map_err(|e| ProviderError::IoError {
            message: format!("Failed to collect Parquet: {}", e),
        })?;

    dataframe_to_bars(&df)
}

/// Scan a Parquet file as a LazyFrame (for direct Polars pipeline).
///
/// This is the preferred method for Phase 4 - skips the Vec<Bar> intermediate.
/// Uses `scan_parquet` for lazy evaluation with predicate pushdown.
pub fn scan_parquet_lazy(path: &Path) -> Result<LazyFrame, ProviderError> {
    LazyFrame::scan_parquet(path, ScanArgsParquet::default()).map_err(|e| ProviderError::IoError {
        message: format!("Failed to scan Parquet: {}", e),
    })
}

/// Scan multiple Parquet files as a single LazyFrame.
///
/// This is optimal for loading data across multiple year partitions.
/// Uses vertical concatenation of lazy scans for maximum performance.
///
/// # Arguments
/// * `paths` - List of Parquet file paths to scan
///
/// # Returns
/// A LazyFrame containing all data, sorted by timestamp.
pub fn scan_multiple_parquet_lazy(
    paths: &[std::path::PathBuf],
) -> Result<LazyFrame, ProviderError> {
    if paths.is_empty() {
        // Return empty LazyFrame with correct schema
        let empty_df = DataFrame::new(vec![
            Series::new("ts".into(), Vec::<i64>::new())
                .cast(&DataType::Datetime(
                    TimeUnit::Milliseconds,
                    Some("UTC".into()),
                ))
                .unwrap()
                .into(),
            Series::new("open".into(), Vec::<f64>::new()).into(),
            Series::new("high".into(), Vec::<f64>::new()).into(),
            Series::new("low".into(), Vec::<f64>::new()).into(),
            Series::new("close".into(), Vec::<f64>::new()).into(),
            Series::new("volume".into(), Vec::<f64>::new()).into(),
            Series::new("symbol".into(), Vec::<String>::new()).into(),
            Series::new("timeframe".into(), Vec::<String>::new()).into(),
        ])
        .map_err(|e| ProviderError::ParseError {
            message: e.to_string(),
        })?;
        return Ok(empty_df.lazy());
    }

    // Scan all files lazily
    let lazy_frames: Vec<LazyFrame> = paths
        .iter()
        .map(|p| {
            LazyFrame::scan_parquet(p, ScanArgsParquet::default()).map_err(|e| {
                ProviderError::IoError {
                    message: format!("Failed to scan Parquet {}: {}", p.display(), e),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Concatenate all LazyFrames
    let combined =
        concat(lazy_frames, UnionArgs::default()).map_err(|e| ProviderError::IoError {
            message: format!("Failed to concatenate Parquet files: {}", e),
        })?;

    // Sort by timestamp for consistent ordering
    let sort_opts = SortMultipleOptions::new().with_order_descending(false);
    Ok(combined.sort(["ts"], sort_opts))
}

/// Scan Parquet files for a symbol with optional date filtering.
///
/// This is the high-level function for Phase 4 direct Parquet pipeline.
/// It scans the appropriate year partitions and applies predicate pushdown
/// for date filtering when possible.
///
/// # Arguments
/// * `base_dir` - Base Parquet directory (e.g., "data/parquet")
/// * `symbol` - Ticker symbol
/// * `timeframe` - Timeframe (e.g., "1d")
/// * `start_date` - Optional start date filter (inclusive)
/// * `end_date` - Optional end date filter (inclusive)
///
/// # Returns
/// A LazyFrame containing the filtered data, sorted by timestamp.
pub fn scan_symbol_parquet_lazy(
    base_dir: &Path,
    symbol: &str,
    timeframe: &str,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
) -> Result<LazyFrame, ProviderError> {
    let symbol_dir = base_dir.join(format!("{}/symbol={}", timeframe, symbol));

    if !symbol_dir.exists() {
        return Err(ProviderError::IoError {
            message: format!(
                "No data found for symbol {} at {}",
                symbol,
                symbol_dir.display()
            ),
        });
    }

    // Determine which year directories to scan based on date range
    let years_to_scan: Vec<i32> = if let (Some(start), Some(end)) = (start_date, end_date) {
        (start.year()..=end.year()).collect()
    } else {
        // Scan all available years
        std::fs::read_dir(&symbol_dir)
            .map_err(|e| ProviderError::IoError {
                message: format!("Failed to read symbol directory: {}", e),
            })?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("year=") {
                    name.strip_prefix("year=")?.parse::<i32>().ok()
                } else {
                    None
                }
            })
            .collect()
    };

    // Build list of parquet file paths
    let parquet_paths: Vec<std::path::PathBuf> = years_to_scan
        .iter()
        .map(|year| symbol_dir.join(format!("year={}/data.parquet", year)))
        .filter(|p| p.exists())
        .collect();

    if parquet_paths.is_empty() {
        return Err(ProviderError::IoError {
            message: format!(
                "No Parquet files found for {} in years {:?}",
                symbol, years_to_scan
            ),
        });
    }

    // Scan all partitions
    let mut lf = scan_multiple_parquet_lazy(&parquet_paths)?;

    // Apply date filtering with predicate pushdown
    if let Some(start) = start_date {
        let start_ts = Utc
            .with_ymd_and_hms(start.year(), start.month(), start.day(), 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        lf = lf.filter(col("ts").gt_eq(lit(start_ts)));
    }

    if let Some(end) = end_date {
        let end_ts = Utc
            .with_ymd_and_hms(end.year(), end.month(), end.day(), 23, 59, 59)
            .single()
            .unwrap()
            .timestamp_millis();
        lf = lf.filter(col("ts").lt_eq(lit(end_ts)));
    }

    Ok(lf)
}

/// Check the date coverage of cached Parquet data for a symbol.
///
/// Returns `Some((min_date, max_date))` if data exists, or `None` if no data found.
/// This is used to determine if we need to fetch additional data from Yahoo.
///
/// # Arguments
/// * `base_dir` - Base Parquet directory (e.g., "data/parquet")
/// * `symbol` - Ticker symbol
/// * `timeframe` - Timeframe (e.g., "1d")
pub fn get_parquet_date_range(
    base_dir: &Path,
    symbol: &str,
    timeframe: &str,
) -> Option<(NaiveDate, NaiveDate)> {
    let symbol_dir = base_dir.join(format!("{}/symbol={}", timeframe, symbol));

    if !symbol_dir.exists() {
        return None;
    }

    // Find all year directories and determine min/max years
    let years: Vec<i32> = std::fs::read_dir(&symbol_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("year=") {
                name.strip_prefix("year=")?.parse::<i32>().ok()
            } else {
                None
            }
        })
        .collect();

    if years.is_empty() {
        return None;
    }

    let min_year = *years.iter().min()?;
    let max_year = *years.iter().max()?;

    // Scan the data to get actual min/max dates
    // We scan all available data (no date filter) to find the true range
    let lf = scan_symbol_parquet_lazy(base_dir, symbol, timeframe, None, None).ok()?;

    // Get min and max timestamps
    let agg = lf
        .select([
            col("ts").min().alias("min_ts"),
            col("ts").max().alias("max_ts"),
        ])
        .collect()
        .ok()?;

    if agg.height() == 0 {
        return None;
    }

    let min_ts = agg.column("min_ts").ok()?.datetime().ok()?.get(0)?;
    let max_ts = agg.column("max_ts").ok()?.datetime().ok()?.get(0)?;

    // Convert milliseconds to NaiveDate
    let min_date = Utc.timestamp_millis_opt(min_ts).single()?.date_naive();
    let max_date = Utc.timestamp_millis_opt(max_ts).single()?.date_naive();

    // Suppress unused variable warnings
    let _ = (min_year, max_year);

    Some((min_date, max_date))
}

/// Write bars to partitioned Parquet files.
///
/// # Arguments
/// * `bars` - Bars to write (can span multiple years)
/// * `base_dir` - Base directory for Parquet files (e.g., "data/parquet")
///
/// # Returns
/// List of paths that were written.
pub fn write_partitioned_parquet(
    bars: &[Bar],
    base_dir: &Path,
) -> Result<Vec<std::path::PathBuf>, ProviderError> {
    if bars.is_empty() {
        return Ok(Vec::new());
    }

    // Get timeframe and symbol from first bar (assumes all bars have same values)
    let timeframe = &bars[0].timeframe;
    let symbol = &bars[0].symbol;

    let partitions = partition_by_year(bars);
    let mut written_paths = Vec::new();

    for (year, year_bars) in partitions {
        let rel_path = parquet_path(timeframe, symbol, year);
        let full_path = base_dir.join(&rel_path);

        write_parquet(&year_bars, &full_path)?;
        written_paths.push(full_path);
    }

    Ok(written_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_test_bars() -> Vec<Bar> {
        vec![
            Bar::new(
                Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
                100.0,
                102.0,
                99.0,
                101.0,
                1000.0,
                "TEST",
                "1d",
            ),
            Bar::new(
                Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap(),
                101.0,
                103.0,
                100.0,
                102.0,
                1200.0,
                "TEST",
                "1d",
            ),
        ]
    }

    #[test]
    fn test_bars_to_dataframe_roundtrip() {
        let bars = make_test_bars();
        let df = bars_to_dataframe(&bars).unwrap();

        assert_eq!(df.height(), 2);
        assert_eq!(df.width(), 8);

        let recovered = dataframe_to_bars(&df).unwrap();
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered[0].open, 100.0);
        assert_eq!(recovered[1].close, 102.0);
    }

    #[test]
    fn test_partition_by_year() {
        let bars = vec![
            Bar::new(
                Utc.with_ymd_and_hms(2023, 12, 29, 0, 0, 0).unwrap(),
                98.0,
                100.0,
                97.0,
                99.0,
                1000.0,
                "TEST",
                "1d",
            ),
            Bar::new(
                Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
                100.0,
                102.0,
                99.0,
                101.0,
                1000.0,
                "TEST",
                "1d",
            ),
            Bar::new(
                Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap(),
                101.0,
                103.0,
                100.0,
                102.0,
                1000.0,
                "TEST",
                "1d",
            ),
        ];

        let partitions = partition_by_year(&bars);

        assert_eq!(partitions.len(), 2);
        assert_eq!(partitions.get(&2023).unwrap().len(), 1);
        assert_eq!(partitions.get(&2024).unwrap().len(), 2);
    }

    #[test]
    fn test_parquet_path() {
        let path = parquet_path("1d", "SPY", 2024);
        assert_eq!(path, "1d/symbol=SPY/year=2024/data.parquet");
    }
}
