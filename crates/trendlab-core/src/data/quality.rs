//! Data quality validation and reporting.
//!
//! Checks for:
//! - Duplicate timestamps
//! - Gaps in time series
//! - Out-of-order timestamps
//! - Invalid OHLC relationships (e.g., high < low)

use crate::bar::Bar;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A specific quality issue found in the data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QualityIssue {
    /// Duplicate bar at this timestamp
    Duplicate {
        ts: DateTime<Utc>,
        symbol: String,
        count: usize,
    },
    /// Gap detected between two timestamps
    Gap {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        symbol: String,
        expected_bars: usize,
    },
    /// Bar is out of chronological order
    OutOfOrder {
        ts: DateTime<Utc>,
        symbol: String,
        previous_ts: DateTime<Utc>,
    },
    /// Invalid OHLC relationship
    InvalidOhlc {
        ts: DateTime<Utc>,
        symbol: String,
        reason: String,
    },
}

/// Summary report of data quality checks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DataQualityReport {
    /// Number of duplicate (symbol, timestamp) pairs
    pub duplicate_count: usize,
    /// Number of gaps detected
    pub gap_count: usize,
    /// Number of out-of-order bars
    pub out_of_order_count: usize,
    /// Number of bars with invalid OHLC relationships
    pub invalid_ohlc_count: usize,
    /// Total bars analyzed
    pub total_bars: usize,
    /// Detailed list of issues
    pub issues: Vec<QualityIssue>,
}

impl DataQualityReport {
    /// Returns true if no issues were found.
    pub fn is_clean(&self) -> bool {
        self.duplicate_count == 0
            && self.gap_count == 0
            && self.out_of_order_count == 0
            && self.invalid_ohlc_count == 0
    }

    /// Get all duplicate timestamps.
    pub fn duplicate_timestamps(&self) -> Vec<DateTime<Utc>> {
        self.issues
            .iter()
            .filter_map(|issue| match issue {
                QualityIssue::Duplicate { ts, .. } => Some(*ts),
                _ => None,
            })
            .collect()
    }

    /// Get invalid OHLC issues at a specific timestamp.
    pub fn invalid_ohlc_at(&self, ts: DateTime<Utc>) -> Option<&QualityIssue> {
        self.issues.iter().find(|issue| match issue {
            QualityIssue::InvalidOhlc { ts: issue_ts, .. } => *issue_ts == ts,
            _ => false,
        })
    }
}

/// Checker for data quality issues.
#[derive(Debug, Default)]
pub struct DataQualityChecker {
    /// Expected timeframe for gap detection (e.g., "1d")
    timeframe: Option<String>,
}

impl DataQualityChecker {
    /// Create a new checker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the expected timeframe for gap detection.
    pub fn with_timeframe(mut self, timeframe: impl Into<String>) -> Self {
        self.timeframe = Some(timeframe.into());
        self
    }

    /// Check a slice of bars for quality issues.
    pub fn check(&self, bars: &[Bar]) -> DataQualityReport {
        let mut report = DataQualityReport {
            total_bars: bars.len(),
            ..Default::default()
        };

        if bars.is_empty() {
            return report;
        }

        // Check for duplicates
        self.check_duplicates(bars, &mut report);

        // Check for out-of-order timestamps
        self.check_ordering(bars, &mut report);

        // Check for gaps (only if timeframe is set)
        if self.timeframe.is_some() {
            self.check_gaps(bars, &mut report);
        }

        // Check OHLC validity
        self.check_ohlc_validity(bars, &mut report);

        report
    }

    fn check_duplicates(&self, bars: &[Bar], report: &mut DataQualityReport) {
        let mut seen: HashSet<(String, DateTime<Utc>)> = HashSet::new();
        let mut dup_keys: HashSet<(String, DateTime<Utc>)> = HashSet::new();

        for bar in bars {
            let key = (bar.symbol.clone(), bar.ts);
            if !seen.insert(key.clone()) {
                dup_keys.insert(key);
            }
        }

        for (symbol, ts) in dup_keys {
            let count = bars
                .iter()
                .filter(|b| b.symbol == symbol && b.ts == ts)
                .count();
            report.duplicate_count += 1;
            report
                .issues
                .push(QualityIssue::Duplicate { ts, symbol, count });
        }
    }

    fn check_ordering(&self, bars: &[Bar], report: &mut DataQualityReport) {
        // Group by symbol first
        let mut by_symbol: std::collections::HashMap<&str, Vec<&Bar>> =
            std::collections::HashMap::new();
        for bar in bars {
            by_symbol.entry(&bar.symbol).or_default().push(bar);
        }

        for (symbol, symbol_bars) in by_symbol {
            let mut prev_ts: Option<DateTime<Utc>> = None;
            for bar in symbol_bars {
                if let Some(prev) = prev_ts {
                    if bar.ts < prev {
                        report.out_of_order_count += 1;
                        report.issues.push(QualityIssue::OutOfOrder {
                            ts: bar.ts,
                            symbol: symbol.to_string(),
                            previous_ts: prev,
                        });
                    }
                }
                prev_ts = Some(bar.ts);
            }
        }
    }

    fn check_gaps(&self, bars: &[Bar], report: &mut DataQualityReport) {
        let timeframe = match &self.timeframe {
            Some(tf) => tf,
            None => return,
        };

        // Only check daily gaps for now
        if timeframe != "1d" {
            return;
        }

        // Group by symbol and sort
        let mut by_symbol: std::collections::HashMap<&str, Vec<&Bar>> =
            std::collections::HashMap::new();
        for bar in bars {
            by_symbol.entry(&bar.symbol).or_default().push(bar);
        }

        for (symbol, mut symbol_bars) in by_symbol {
            symbol_bars.sort_by_key(|b| b.ts);

            for window in symbol_bars.windows(2) {
                let (prev, curr) = (window[0], window[1]);
                let days_diff = (curr.ts - prev.ts).num_days();

                // For daily data, any gap > 1 day is flagged
                // Note: This is a simple heuristic. A proper implementation would
                // use a trading calendar to distinguish gaps from weekends/holidays.
                if days_diff > 1 {
                    report.gap_count += 1;
                    report.issues.push(QualityIssue::Gap {
                        from: prev.ts,
                        to: curr.ts,
                        symbol: symbol.to_string(),
                        expected_bars: (days_diff - 1) as usize,
                    });
                }
            }
        }
    }

    fn check_ohlc_validity(&self, bars: &[Bar], report: &mut DataQualityReport) {
        for bar in bars {
            // Check: high >= open, high >= close, high >= low
            // Check: low <= open, low <= close
            let mut reasons = Vec::new();

            if bar.high < bar.open {
                reasons.push("high < open");
            }
            if bar.high < bar.close {
                reasons.push("high < close");
            }
            if bar.high < bar.low {
                reasons.push("high < low");
            }
            if bar.low > bar.open {
                reasons.push("low > open");
            }
            if bar.low > bar.close {
                reasons.push("low > close");
            }

            if !reasons.is_empty() {
                report.invalid_ohlc_count += 1;
                report.issues.push(QualityIssue::InvalidOhlc {
                    ts: bar.ts,
                    symbol: bar.symbol.clone(),
                    reason: reasons.join(", "),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn mk_bar(day: u32, open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar::new(
            Utc.with_ymd_and_hms(2024, 1, day, 0, 0, 0).unwrap(),
            open,
            high,
            low,
            close,
            1000.0,
            "TEST",
            "1d",
        )
    }

    #[test]
    fn detects_duplicates() {
        let bars = vec![
            mk_bar(2, 100.0, 101.0, 99.0, 100.5),
            mk_bar(2, 100.0, 101.0, 99.0, 100.5), // duplicate
            mk_bar(3, 101.0, 102.0, 100.0, 101.5),
        ];

        let checker = DataQualityChecker::new();
        let report = checker.check(&bars);

        assert_eq!(report.duplicate_count, 1);
    }

    #[test]
    fn detects_out_of_order() {
        let bars = vec![
            mk_bar(3, 102.0, 103.0, 101.0, 102.5),
            mk_bar(2, 100.0, 101.0, 99.0, 100.5), // out of order
            mk_bar(4, 103.0, 104.0, 102.0, 103.5),
        ];

        let checker = DataQualityChecker::new();
        let report = checker.check(&bars);

        assert_eq!(report.out_of_order_count, 1);
    }

    #[test]
    fn detects_invalid_ohlc() {
        let bars = vec![
            mk_bar(2, 100.0, 105.0, 99.0, 103.0), // valid
            mk_bar(3, 101.0, 100.0, 99.0, 102.0), // invalid: high < open
        ];

        let checker = DataQualityChecker::new();
        let report = checker.check(&bars);

        assert_eq!(report.invalid_ohlc_count, 1);
    }

    #[test]
    fn clean_data_reports_clean() {
        let bars = vec![
            mk_bar(2, 100.0, 105.0, 99.0, 103.0),
            mk_bar(3, 103.0, 108.0, 102.0, 106.0),
            mk_bar(4, 106.0, 110.0, 105.0, 109.0),
        ];

        let checker = DataQualityChecker::new();
        let report = checker.check(&bars);

        assert!(report.is_clean());
    }
}
