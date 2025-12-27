//! Yahoo Finance data parsing.
//!
//! Parses responses from Yahoo Finance APIs:
//! - CSV format from the v7 download API (requires auth)
//! - JSON format from the v8 chart API (no auth required)
//!
//! This module contains pure parsing logic with no network I/O.

use crate::bar::Bar;
use crate::data::ProviderError;
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use serde_json::Value;

/// Yahoo Finance CSV row (intermediate representation).
#[derive(Debug, Clone)]
struct YahooRow {
    date: NaiveDate,
    open: f64,
    high: f64,
    low: f64,
    #[allow(dead_code)] // Parsed for completeness but we use adj_close instead
    close: f64,
    adj_close: f64,
    volume: f64,
}

/// Parse a Yahoo Finance CSV response into bars.
///
/// # Arguments
/// * `csv_text` - Raw CSV text from Yahoo Finance
/// * `symbol` - Symbol to assign to the bars
/// * `timeframe` - Timeframe to assign (e.g., "1d")
///
/// # Returns
/// Vector of bars, sorted by timestamp ascending.
/// Rows with null/invalid values are skipped.
///
/// # CSV Format
/// ```csv
/// Date,Open,High,Low,Close,Adj Close,Volume
/// 2024-01-02,100.00,102.50,99.50,101.00,101.00,1000000
/// ```
///
/// # Notes
/// - Uses "Adj Close" for the close field (split/dividend adjusted)
/// - Rows with "null" values are filtered out
/// - Output is sorted by date ascending
pub fn parse_yahoo_csv(
    csv_text: &str,
    symbol: &str,
    timeframe: &str,
) -> Result<Vec<Bar>, ProviderError> {
    let mut rows = Vec::new();

    let lines: Vec<&str> = csv_text.lines().collect();
    if lines.is_empty() {
        return Ok(Vec::new());
    }

    // Validate header
    let header = lines[0].to_lowercase();
    if !header.contains("date") || !header.contains("close") {
        return Err(ProviderError::ParseError {
            message: "Invalid CSV header: missing required columns".to_string(),
        });
    }

    // Parse each data row
    for (line_num, line) in lines.iter().enumerate().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match parse_yahoo_row(line) {
            Ok(Some(row)) => rows.push(row),
            Ok(None) => {
                // Row had null values, skip it
            }
            Err(e) => {
                return Err(ProviderError::ParseError {
                    message: format!("Line {}: {}", line_num + 1, e),
                });
            }
        }
    }

    // Sort by date ascending
    rows.sort_by_key(|r| r.date);

    // Convert to bars with consistent adjustment
    // Apply the adjustment ratio (adj_close / close) to all OHLC values
    // This ensures all prices are consistently adjusted for splits/dividends
    let bars = rows
        .into_iter()
        .map(|row| {
            let ts = Utc
                .with_ymd_and_hms(row.date.year(), row.date.month(), row.date.day(), 0, 0, 0)
                .single()
                .expect("Valid date");

            // Calculate adjustment factor: adj_close / close
            // This accounts for stock splits and dividends
            let adj_factor = if row.close != 0.0 {
                row.adj_close / row.close
            } else {
                1.0
            };

            Bar::new(
                ts,
                row.open * adj_factor, // Adjusted open
                row.high * adj_factor, // Adjusted high
                row.low * adj_factor,  // Adjusted low
                row.adj_close,         // Already adjusted close
                row.volume,
                symbol,
                timeframe,
            )
        })
        .collect();

    Ok(bars)
}

/// Parse a single CSV row.
/// Returns None if the row contains null values.
fn parse_yahoo_row(line: &str) -> Result<Option<YahooRow>, String> {
    let fields: Vec<&str> = line.split(',').collect();

    if fields.len() < 7 {
        return Err(format!("Expected 7 columns, got {}", fields.len()));
    }

    // Check for null values
    for field in &fields[1..] {
        if field.trim().eq_ignore_ascii_case("null") {
            return Ok(None);
        }
    }

    let date = NaiveDate::parse_from_str(fields[0].trim(), "%Y-%m-%d")
        .map_err(|e| format!("Invalid date '{}': {}", fields[0], e))?;

    let open = parse_f64(fields[1], "open")?;
    let high = parse_f64(fields[2], "high")?;
    let low = parse_f64(fields[3], "low")?;
    let close = parse_f64(fields[4], "close")?;
    let adj_close = parse_f64(fields[5], "adj_close")?;
    let volume = parse_f64(fields[6], "volume")?;

    Ok(Some(YahooRow {
        date,
        open,
        high,
        low,
        close,
        adj_close,
        volume,
    }))
}

fn parse_f64(s: &str, field_name: &str) -> Result<f64, String> {
    s.trim()
        .parse()
        .map_err(|_| format!("Invalid {} value: '{}'", field_name, s))
}

/// Build a Yahoo Finance download URL for historical data (v7 API - requires auth).
///
/// # Arguments
/// * `symbol` - Ticker symbol
/// * `start` - Start date (inclusive)
/// * `end` - End date (inclusive)
///
/// # Returns
/// URL string for Yahoo Finance CSV download.
///
/// # Note
/// This API now requires authentication. Use `build_yahoo_chart_url` instead.
pub fn build_yahoo_url(symbol: &str, start: NaiveDate, end: NaiveDate) -> String {
    // Yahoo uses Unix timestamps (seconds since epoch)
    let start_ts = Utc
        .with_ymd_and_hms(start.year(), start.month(), start.day(), 0, 0, 0)
        .single()
        .unwrap()
        .timestamp();

    // End date: use 23:59:59 to include the full day
    let end_ts = Utc
        .with_ymd_and_hms(end.year(), end.month(), end.day(), 23, 59, 59)
        .single()
        .unwrap()
        .timestamp();

    format!(
        "https://query1.finance.yahoo.com/v7/finance/download/{}?period1={}&period2={}&interval=1d&events=history&includeAdjustedClose=true",
        symbol, start_ts, end_ts
    )
}

/// Build a Yahoo Finance chart URL for historical data (v8 API - no auth required).
///
/// # Arguments
/// * `symbol` - Ticker symbol
/// * `start` - Start date (inclusive)
/// * `end` - End date (inclusive)
///
/// # Returns
/// URL string for Yahoo Finance chart API (JSON).
pub fn build_yahoo_chart_url(symbol: &str, start: NaiveDate, end: NaiveDate) -> String {
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

    format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?period1={}&period2={}&interval=1d",
        symbol, start_ts, end_ts
    )
}

/// Parse Yahoo Finance chart API JSON response into bars.
///
/// # Arguments
/// * `json_text` - Raw JSON text from Yahoo Finance chart API
/// * `symbol` - Symbol to assign to the bars
/// * `timeframe` - Timeframe to assign (e.g., "1d")
///
/// # Returns
/// Vector of bars, sorted by timestamp ascending.
///
/// # JSON Structure
/// The chart API returns data in this format:
/// ```json
/// {
///   "chart": {
///     "result": [{
///       "timestamp": [1704205800, ...],
///       "indicators": {
///         "quote": [{"open": [...], "high": [...], "low": [...], "close": [...], "volume": [...]}],
///         "adjclose": [{"adjclose": [...]}]
///       }
///     }]
///   }
/// }
/// ```
pub fn parse_yahoo_chart_json(
    json_text: &str,
    symbol: &str,
    timeframe: &str,
) -> Result<Vec<Bar>, ProviderError> {
    let json: Value = serde_json::from_str(json_text).map_err(|e| ProviderError::ParseError {
        message: format!("Invalid JSON: {}", e),
    })?;

    // Check for API errors
    if let Some(error) = json.get("chart").and_then(|c| c.get("error")) {
        if !error.is_null() {
            let code = error
                .get("code")
                .and_then(|c| c.as_str())
                .unwrap_or("unknown");
            let desc = error
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("unknown error");
            return Err(ProviderError::ParseError {
                message: format!("Yahoo API error: {} - {}", code, desc),
            });
        }
    }

    // Navigate to the result
    let result = json
        .get("chart")
        .and_then(|c| c.get("result"))
        .and_then(|r| r.get(0))
        .ok_or_else(|| ProviderError::ParseError {
            message: "Missing chart.result[0] in response".to_string(),
        })?;

    // Get timestamps
    let timestamps = result
        .get("timestamp")
        .and_then(|t| t.as_array())
        .ok_or_else(|| ProviderError::ParseError {
            message: "Missing timestamp array".to_string(),
        })?;

    if timestamps.is_empty() {
        return Ok(Vec::new());
    }

    // Get quote data
    let quote = result
        .get("indicators")
        .and_then(|i| i.get("quote"))
        .and_then(|q| q.get(0))
        .ok_or_else(|| ProviderError::ParseError {
            message: "Missing indicators.quote[0]".to_string(),
        })?;

    let opens = quote.get("open").and_then(|o| o.as_array());
    let highs = quote.get("high").and_then(|h| h.as_array());
    let lows = quote.get("low").and_then(|l| l.as_array());
    let closes = quote.get("close").and_then(|c| c.as_array());
    let volumes = quote.get("volume").and_then(|v| v.as_array());

    // Get adjusted close
    let adjcloses = result
        .get("indicators")
        .and_then(|i| i.get("adjclose"))
        .and_then(|a| a.get(0))
        .and_then(|a| a.get("adjclose"))
        .and_then(|a| a.as_array());

    let mut bars = Vec::with_capacity(timestamps.len());

    for (i, ts_val) in timestamps.iter().enumerate() {
        let ts_unix = ts_val.as_i64().unwrap_or(0);
        if ts_unix == 0 {
            continue;
        }

        // Extract values, handling null (market holidays)
        let open = opens.and_then(|o| o.get(i)).and_then(|v| v.as_f64());
        let high = highs.and_then(|h| h.get(i)).and_then(|v| v.as_f64());
        let low = lows.and_then(|l| l.get(i)).and_then(|v| v.as_f64());
        let close = closes.and_then(|c| c.get(i)).and_then(|v| v.as_f64());
        let volume = volumes
            .and_then(|v| v.get(i))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let adj_close = adjcloses.and_then(|a| a.get(i)).and_then(|v| v.as_f64());

        // Skip bars with null OHLC values
        let (open, high, low, close, adj_close) = match (open, high, low, close, adj_close) {
            (Some(o), Some(h), Some(l), Some(c), Some(a)) => (o, h, l, c, a),
            _ => continue,
        };

        // Calculate adjustment factor for OHLC consistency
        let adj_factor = if close != 0.0 { adj_close / close } else { 1.0 };

        let ts =
            Utc.timestamp_opt(ts_unix, 0)
                .single()
                .ok_or_else(|| ProviderError::ParseError {
                    message: format!("Invalid timestamp: {}", ts_unix),
                })?;

        bars.push(Bar::new(
            ts,
            open * adj_factor,
            high * adj_factor,
            low * adj_factor,
            adj_close,
            volume,
            symbol,
            timeframe,
        ));
    }

    // Sort by timestamp ascending (should already be sorted, but ensure it)
    bars.sort_by_key(|b| b.ts);

    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_simple_csv() {
        let csv = r#"Date,Open,High,Low,Close,Adj Close,Volume
2024-01-02,100.00,102.50,99.50,101.00,101.00,1000000
2024-01-03,101.50,103.00,100.00,102.50,102.50,1200000"#;

        let bars = parse_yahoo_csv(csv, "TEST", "1d").unwrap();
        assert_eq!(bars.len(), 2);

        assert_eq!(bars[0].open, 100.00);
        assert_eq!(bars[0].close, 101.00); // Uses adj_close
        assert_eq!(bars[0].symbol, "TEST");
        assert_eq!(bars[0].ts.day(), 2);

        assert_eq!(bars[1].open, 101.50);
        assert_eq!(bars[1].close, 102.50);
    }

    #[test]
    fn test_parse_uses_adjusted_ohlc() {
        // Simulate a 2:1 stock split - adj_close is half of close
        let csv = r#"Date,Open,High,Low,Close,Adj Close,Volume
2024-01-02,100.00,102.50,99.50,101.00,50.50,1000000"#;

        let bars = parse_yahoo_csv(csv, "SPLIT", "1d").unwrap();
        assert_eq!(bars.len(), 1);

        // Adjustment factor = 50.50 / 101.00 = 0.5
        // All OHLC values should be adjusted by this factor
        assert_eq!(bars[0].close, 50.50); // Adj Close
        assert_eq!(bars[0].open, 50.00); // 100.00 * 0.5
        assert_eq!(bars[0].high, 51.25); // 102.50 * 0.5
        assert_eq!(bars[0].low, 49.75); // 99.50 * 0.5
    }

    #[test]
    fn test_parse_empty_csv() {
        let csv = "Date,Open,High,Low,Close,Adj Close,Volume\n";
        let bars = parse_yahoo_csv(csv, "EMPTY", "1d").unwrap();
        assert_eq!(bars.len(), 0);
    }

    #[test]
    fn test_parse_skips_null_rows() {
        let csv = r#"Date,Open,High,Low,Close,Adj Close,Volume
2024-01-02,100.00,102.50,99.50,101.00,101.00,1000000
2024-01-03,null,null,null,null,null,null
2024-01-04,102.00,104.00,101.00,103.00,103.00,800000"#;

        let bars = parse_yahoo_csv(csv, "GAPS", "1d").unwrap();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].ts.day(), 2);
        assert_eq!(bars[1].ts.day(), 4);
    }

    #[test]
    fn test_parse_sorts_by_date() {
        // Input is out of order
        let csv = r#"Date,Open,High,Low,Close,Adj Close,Volume
2024-01-04,103.00,105.00,102.00,104.00,104.00,1000000
2024-01-02,100.00,102.00,99.00,101.00,101.00,1000000
2024-01-03,101.00,103.00,100.00,102.00,102.00,1000000"#;

        let bars = parse_yahoo_csv(csv, "SORT", "1d").unwrap();
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0].ts.day(), 2);
        assert_eq!(bars[1].ts.day(), 3);
        assert_eq!(bars[2].ts.day(), 4);
    }

    #[test]
    fn test_parse_invalid_csv() {
        let csv = r#"Date,Open,High
2024-01-02,not_a_number,102.50"#;

        let result = parse_yahoo_csv(csv, "ERR", "1d");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_yahoo_url() {
        let url = build_yahoo_url(
            "SPY",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
        );

        assert!(url.contains("SPY"));
        assert!(url.contains("period1="));
        assert!(url.contains("period2="));
        assert!(url.contains("interval=1d"));
    }

    #[test]
    fn test_build_yahoo_chart_url() {
        let url = build_yahoo_chart_url(
            "GOOG",
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
        );

        assert!(url.contains("GOOG"));
        assert!(url.contains("v8/finance/chart"));
        assert!(url.contains("period1="));
        assert!(url.contains("period2="));
        assert!(url.contains("interval=1d"));
    }

    #[test]
    fn test_parse_yahoo_chart_json() {
        // Sample JSON response from Yahoo chart API
        let json = r#"{
            "chart": {
                "result": [{
                    "timestamp": [1704153600, 1704240000, 1704326400],
                    "indicators": {
                        "quote": [{
                            "open": [140.0, 141.0, 142.0],
                            "high": [142.0, 143.0, 144.0],
                            "low": [139.0, 140.0, 141.0],
                            "close": [141.0, 142.0, 143.0],
                            "volume": [1000000, 1100000, 1200000]
                        }],
                        "adjclose": [{
                            "adjclose": [141.0, 142.0, 143.0]
                        }]
                    }
                }],
                "error": null
            }
        }"#;

        let bars = parse_yahoo_chart_json(json, "TEST", "1d").unwrap();
        assert_eq!(bars.len(), 3);

        // Check first bar values
        assert_eq!(bars[0].open, 140.0);
        assert_eq!(bars[0].high, 142.0);
        assert_eq!(bars[0].low, 139.0);
        assert_eq!(bars[0].close, 141.0);
        assert_eq!(bars[0].volume, 1000000.0);
        assert_eq!(bars[0].symbol, "TEST");
    }

    #[test]
    fn test_parse_yahoo_chart_json_with_split() {
        // Simulate a 2:1 split - adj_close is half of close
        let json = r#"{
            "chart": {
                "result": [{
                    "timestamp": [1704153600],
                    "indicators": {
                        "quote": [{
                            "open": [200.0],
                            "high": [210.0],
                            "low": [195.0],
                            "close": [205.0],
                            "volume": [1000000]
                        }],
                        "adjclose": [{
                            "adjclose": [102.5]
                        }]
                    }
                }],
                "error": null
            }
        }"#;

        let bars = parse_yahoo_chart_json(json, "SPLIT", "1d").unwrap();
        assert_eq!(bars.len(), 1);

        // Adjustment factor = 102.5 / 205.0 = 0.5
        assert_eq!(bars[0].close, 102.5); // adj_close
        assert_eq!(bars[0].open, 100.0); // 200.0 * 0.5
        assert_eq!(bars[0].high, 105.0); // 210.0 * 0.5
        assert_eq!(bars[0].low, 97.5); // 195.0 * 0.5
    }

    #[test]
    fn test_parse_yahoo_chart_json_skips_nulls() {
        // Second bar has null values (market holiday)
        let json = r#"{
            "chart": {
                "result": [{
                    "timestamp": [1704153600, 1704240000, 1704326400],
                    "indicators": {
                        "quote": [{
                            "open": [140.0, null, 142.0],
                            "high": [142.0, null, 144.0],
                            "low": [139.0, null, 141.0],
                            "close": [141.0, null, 143.0],
                            "volume": [1000000, null, 1200000]
                        }],
                        "adjclose": [{
                            "adjclose": [141.0, null, 143.0]
                        }]
                    }
                }],
                "error": null
            }
        }"#;

        let bars = parse_yahoo_chart_json(json, "GAPS", "1d").unwrap();
        assert_eq!(bars.len(), 2); // Skipped the null bar
    }

    #[test]
    fn test_parse_yahoo_chart_json_error() {
        let json = r#"{
            "chart": {
                "result": null,
                "error": {
                    "code": "Not Found",
                    "description": "No data found for symbol XYZ"
                }
            }
        }"#;

        let result = parse_yahoo_chart_json(json, "XYZ", "1d");
        assert!(result.is_err());
    }
}
