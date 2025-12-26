//! Bar (OHLCV) data types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single OHLCV bar representing price action over a time period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    /// Timestamp (start of bar period, UTC)
    pub ts: DateTime<Utc>,

    /// Opening price
    pub open: f64,

    /// Highest price during period
    pub high: f64,

    /// Lowest price during period
    pub low: f64,

    /// Closing price (adjusted for splits and dividends)
    pub close: f64,

    /// Trading volume
    pub volume: f64,

    /// Ticker symbol
    pub symbol: String,

    /// Timeframe (e.g., "1d", "1h")
    pub timeframe: String,
}

impl Bar {
    /// Create a new bar with all fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ts: DateTime<Utc>,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        symbol: impl Into<String>,
        timeframe: impl Into<String>,
    ) -> Self {
        Self {
            ts,
            open,
            high,
            low,
            close,
            volume,
            symbol: symbol.into(),
            timeframe: timeframe.into(),
        }
    }

    /// Returns the bar's range (high - low).
    pub fn range(&self) -> f64 {
        self.high - self.low
    }

    /// Returns the bar's body size (absolute difference between open and close).
    pub fn body(&self) -> f64 {
        (self.close - self.open).abs()
    }

    /// Returns true if this is a bullish (green) bar.
    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }

    /// Returns true if this is a bearish (red) bar.
    pub fn is_bearish(&self) -> bool {
        self.close < self.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_bar() -> Bar {
        Bar::new(
            Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
            100.0,
            105.0,
            99.0,
            103.0,
            1_000_000.0,
            "SPY",
            "1d",
        )
    }

    #[test]
    fn test_bar_range() {
        let bar = sample_bar();
        assert_eq!(bar.range(), 6.0);
    }

    #[test]
    fn test_bar_body() {
        let bar = sample_bar();
        assert_eq!(bar.body(), 3.0);
    }

    #[test]
    fn test_bar_bullish() {
        let bar = sample_bar();
        assert!(bar.is_bullish());
        assert!(!bar.is_bearish());
    }
}
