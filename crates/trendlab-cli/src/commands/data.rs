//! Data management commands.
//!
//! Handles fetching, caching, and normalizing market data from external providers.

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use trendlab_core::data::{
    parse_yahoo_csv, write_partitioned_parquet, CacheMetadata, DataQualityChecker,
    DataQualityReport, FetchRequest,
};

/// Configuration for the data layer.
pub struct DataConfig {
    /// Base directory for all data (typically "data")
    pub data_dir: PathBuf,
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
        }
    }
}

impl DataConfig {
    /// Path to raw cache directory.
    pub fn raw_dir(&self) -> PathBuf {
        self.data_dir.join("raw")
    }

    /// Path to normalized Parquet directory.
    pub fn parquet_dir(&self) -> PathBuf {
        self.data_dir.join("parquet")
    }

    /// Path to quality reports directory.
    pub fn reports_dir(&self) -> PathBuf {
        self.data_dir.join("reports")
    }
}

/// Result of refreshing data for a single symbol.
#[derive(Debug)]
pub struct RefreshResult {
    pub symbol: String,
    pub bars_count: usize,
    pub source: RefreshSource,
    pub quality_report: DataQualityReport,
    pub parquet_paths: Vec<PathBuf>,
}

/// Whether data was fetched fresh or loaded from cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshSource {
    Cache,
    Fresh,
}

/// Refresh Yahoo Finance data for multiple symbols.
pub async fn refresh_yahoo(
    tickers: &[String],
    start: NaiveDate,
    end: NaiveDate,
    force: bool,
    config: &DataConfig,
) -> Result<Vec<RefreshResult>> {
    let mut results = Vec::new();

    for symbol in tickers {
        let result = refresh_symbol(symbol, start, end, force, config).await?;
        results.push(result);
    }

    Ok(results)
}

/// Refresh data for a single symbol.
async fn refresh_symbol(
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
    force: bool,
    config: &DataConfig,
) -> Result<RefreshResult> {
    let _request = FetchRequest::daily(symbol, start, end).with_force(force);
    let raw_dir = config.raw_dir();
    let parquet_dir = config.parquet_dir();

    // Check cache first
    let cache_path = raw_dir.join(format!("yahoo/{}/{}_{}.csv", symbol, start, end));
    let meta_path = raw_dir.join(format!("yahoo/{}/{}_{}.meta.json", symbol, start, end));

    let (csv_text, source) = if !force && cache_path.exists() && meta_path.exists() {
        // Load from cache
        let csv = std::fs::read_to_string(&cache_path)
            .with_context(|| format!("Failed to read cache file: {}", cache_path.display()))?;
        (csv, RefreshSource::Cache)
    } else {
        // Fetch fresh data
        let csv = fetch_yahoo_csv(symbol, start, end).await?;

        // Write to cache
        write_cache(&csv, symbol, start, end, &raw_dir)?;

        (csv, RefreshSource::Fresh)
    };

    // Parse CSV to bars
    let bars = parse_yahoo_csv(&csv_text, symbol, "1d")
        .with_context(|| format!("Failed to parse Yahoo CSV for {}", symbol))?;

    // Run quality checks
    let checker = DataQualityChecker::new().with_timeframe("1d");
    let quality_report = checker.check(&bars);

    // Write normalized Parquet
    let parquet_paths = if !bars.is_empty() {
        write_partitioned_parquet(&bars, &parquet_dir)
            .with_context(|| format!("Failed to write Parquet for {}", symbol))?
    } else {
        Vec::new()
    };

    // Write quality report
    write_quality_report(symbol, start, end, &quality_report, config)?;

    Ok(RefreshResult {
        symbol: symbol.to_string(),
        bars_count: bars.len(),
        source,
        quality_report,
        parquet_paths,
    })
}

/// Fetch OHLCV data from Yahoo Finance using the chart API.
///
/// Yahoo Finance's download endpoint requires authentication, but the chart API
/// is more accessible. We fetch JSON data and convert it to CSV format.
async fn fetch_yahoo_csv(symbol: &str, start: NaiveDate, end: NaiveDate) -> Result<String> {
    // Build chart API URL
    let start_ts = Utc
        .with_ymd_and_hms(start.year(), start.month(), start.day(), 0, 0, 0)
        .single()
        .unwrap()
        .timestamp();

    let end_ts = Utc
        .with_ymd_and_hms(end.year(), end.month(), end.day(), 23, 59, 59)
        .single()
        .unwrap()
        .timestamp();

    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?period1={}&period2={}&interval=1d&events=history",
        symbol, start_ts, end_ts
    );

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch data for {}", symbol))?;

    let status = response.status();
    if !status.is_success() {
        if status.as_u16() == 404 {
            anyhow::bail!("Symbol not found: {}", symbol);
        }
        anyhow::bail!("HTTP error {}: {}", status.as_u16(), status.as_str());
    }

    let json: serde_json::Value = response
        .json()
        .await
        .with_context(|| format!("Failed to parse JSON response for {}", symbol))?;

    // Extract data from the chart API response
    let chart = json
        .get("chart")
        .and_then(|c| c.get("result"))
        .and_then(|r| r.get(0))
        .ok_or_else(|| anyhow::anyhow!("Invalid chart response structure"))?;

    // Check for errors in the response
    if let Some(error) = json.get("chart").and_then(|c| c.get("error")) {
        if !error.is_null() {
            let error_msg = error
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Yahoo Finance error: {}", error_msg);
        }
    }

    let timestamps = chart
        .get("timestamp")
        .and_then(|t| t.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing timestamp array"))?;

    let indicators = chart
        .get("indicators")
        .and_then(|i| i.get("quote"))
        .and_then(|q| q.get(0))
        .ok_or_else(|| anyhow::anyhow!("Missing quote indicators"))?;

    let adj_close = chart
        .get("indicators")
        .and_then(|i| i.get("adjclose"))
        .and_then(|a| a.get(0))
        .and_then(|a| a.get("adjclose"))
        .and_then(|a| a.as_array());

    let opens = indicators.get("open").and_then(|o| o.as_array());
    let highs = indicators.get("high").and_then(|h| h.as_array());
    let lows = indicators.get("low").and_then(|l| l.as_array());
    let closes = indicators.get("close").and_then(|c| c.as_array());
    let volumes = indicators.get("volume").and_then(|v| v.as_array());

    // Convert to CSV format
    let mut csv = String::from("Date,Open,High,Low,Close,Adj Close,Volume\n");

    for (i, ts) in timestamps.iter().enumerate() {
        let ts_val = ts.as_i64().unwrap_or(0);
        let date = Utc.timestamp_opt(ts_val, 0).single();
        let date_str = match date {
            Some(d) => d.format("%Y-%m-%d").to_string(),
            None => continue,
        };

        let open = opens
            .and_then(|o| o.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "null".to_string());

        let high = highs
            .and_then(|h| h.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "null".to_string());

        let low = lows
            .and_then(|l| l.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "null".to_string());

        let close = closes
            .and_then(|c| c.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "null".to_string());

        let adj = adj_close
            .and_then(|a| a.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| close.clone());

        let volume = volumes
            .and_then(|v| v.get(i))
            .and_then(|v| v.as_f64())
            .map(|v| format!("{:.0}", v))
            .unwrap_or_else(|| "0".to_string());

        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            date_str, open, high, low, close, adj, volume
        ));
    }

    Ok(csv)
}

/// Write raw CSV and metadata to cache.
fn write_cache(
    csv_text: &str,
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
    raw_dir: &Path,
) -> Result<()> {
    let cache_dir = raw_dir.join(format!("yahoo/{}", symbol));
    std::fs::create_dir_all(&cache_dir)?;

    let cache_path = cache_dir.join(format!("{}_{}.csv", start, end));
    let meta_path = cache_dir.join(format!("{}_{}.meta.json", start, end));

    // Write CSV
    std::fs::write(&cache_path, csv_text)?;

    // Calculate checksum
    let mut hasher = Sha256::new();
    hasher.update(csv_text.as_bytes());
    let checksum = format!("{:x}", hasher.finalize());

    // Count rows (excluding header)
    let row_count = csv_text.lines().skip(1).filter(|l| !l.is_empty()).count();

    // Write metadata
    let metadata = CacheMetadata::new("yahoo", symbol, start, end, "1d", row_count, checksum);
    let meta_json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(&meta_path, meta_json)?;

    Ok(())
}

/// Write data quality report to disk.
fn write_quality_report(
    symbol: &str,
    start: NaiveDate,
    end: NaiveDate,
    report: &DataQualityReport,
    config: &DataConfig,
) -> Result<()> {
    let reports_dir = config.reports_dir().join("quality");
    std::fs::create_dir_all(&reports_dir)?;

    let report_path = reports_dir.join(format!("{}_{}_{}.json", symbol, start, end));
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(&report_path, json)?;

    Ok(())
}

/// Parse ticker string (comma-separated or file path).
pub fn parse_tickers(input: &str) -> Result<Vec<String>> {
    // Check if it's a file path
    let path = Path::new(input);
    if path.exists() && path.is_file() {
        let content = std::fs::read_to_string(path)?;
        return Ok(content
            .lines()
            .map(|l| l.trim().to_uppercase())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect());
    }

    // Parse as comma-separated
    Ok(input
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect())
}

/// Parse date string (YYYY-MM-DD).
pub fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("Invalid date format: '{}'. Expected YYYY-MM-DD", s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_tickers_comma_separated() {
        let tickers = parse_tickers("spy, qqq, iwm").unwrap();
        assert_eq!(tickers, vec!["SPY", "QQQ", "IWM"]);
    }

    #[test]
    fn test_parse_tickers_single() {
        let tickers = parse_tickers("AAPL").unwrap();
        assert_eq!(tickers, vec!["AAPL"]);
    }

    #[test]
    fn test_parse_date_valid() {
        let date = parse_date("2024-01-15").unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), 1);
        assert_eq!(date.day(), 15);
    }

    #[test]
    fn test_parse_date_invalid() {
        let result = parse_date("not-a-date");
        assert!(result.is_err());
    }
}
