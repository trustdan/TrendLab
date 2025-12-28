//! Walk-forward validation and cross-validation for trading strategies.
//!
//! Provides validation techniques to detect overfitting and estimate
//! realistic out-of-sample performance:
//! - Walk-forward analysis (rolling optimization/test windows)
//! - Time-series cross-validation
//! - Train/test splitting with gap periods

use chrono::{DateTime, Duration, Utc};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during validation operations.
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Insufficient data for validation: need {needed}, have {available}")]
    InsufficientData { needed: usize, available: usize },

    #[error("Invalid split configuration: {0}")]
    InvalidConfig(String),

    #[error("Missing required column: {0}")]
    MissingColumn(String),

    #[error("Polars error: {0}")]
    PolarsError(#[from] PolarsError),

    #[error("Date parsing error: {0}")]
    DateError(String),
}

/// Configuration for walk-forward analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkForwardConfig {
    /// In-sample window size (number of bars)
    pub in_sample_bars: usize,
    /// Out-of-sample window size (number of bars)
    pub out_of_sample_bars: usize,
    /// Gap between IS and OOS (number of bars, to avoid lookahead)
    pub gap_bars: usize,
    /// Step size for rolling forward (number of bars)
    pub step_bars: usize,
    /// Minimum number of folds required
    pub min_folds: usize,
}

impl Default for WalkForwardConfig {
    fn default() -> Self {
        Self {
            in_sample_bars: 252,       // ~1 year daily
            out_of_sample_bars: 63,    // ~3 months daily
            gap_bars: 5,               // Small gap to avoid edge effects
            step_bars: 63,             // Roll quarterly
            min_folds: 3,              // At least 3 folds
        }
    }
}

impl WalkForwardConfig {
    /// Create a new config with yearly IS and quarterly OOS.
    pub fn yearly_quarterly() -> Self {
        Self::default()
    }

    /// Create a config with 6-month IS and 1-month OOS.
    pub fn half_year_monthly() -> Self {
        Self {
            in_sample_bars: 126,
            out_of_sample_bars: 21,
            gap_bars: 3,
            step_bars: 21,
            min_folds: 5,
        }
    }

    /// Create a config with 2-year IS and 6-month OOS.
    pub fn two_year_half_year() -> Self {
        Self {
            in_sample_bars: 504,
            out_of_sample_bars: 126,
            gap_bars: 5,
            step_bars: 126,
            min_folds: 2,
        }
    }

    /// Validate the configuration.
    pub fn validate(&self, total_bars: usize) -> Result<(), ValidationError> {
        let min_bars = self.in_sample_bars + self.gap_bars + self.out_of_sample_bars;
        if total_bars < min_bars {
            return Err(ValidationError::InsufficientData {
                needed: min_bars,
                available: total_bars,
            });
        }

        if self.in_sample_bars == 0 {
            return Err(ValidationError::InvalidConfig(
                "in_sample_bars must be > 0".to_string(),
            ));
        }

        if self.out_of_sample_bars == 0 {
            return Err(ValidationError::InvalidConfig(
                "out_of_sample_bars must be > 0".to_string(),
            ));
        }

        if self.step_bars == 0 {
            return Err(ValidationError::InvalidConfig(
                "step_bars must be > 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// A single fold in walk-forward analysis.
#[derive(Debug, Clone)]
pub struct WalkForwardFold {
    /// Fold number (0-indexed)
    pub fold_idx: usize,
    /// In-sample start index (inclusive)
    pub is_start: usize,
    /// In-sample end index (exclusive)
    pub is_end: usize,
    /// Out-of-sample start index (inclusive)
    pub oos_start: usize,
    /// Out-of-sample end index (exclusive)
    pub oos_end: usize,
}

impl WalkForwardFold {
    /// Get the in-sample bar range.
    pub fn in_sample_range(&self) -> std::ops::Range<usize> {
        self.is_start..self.is_end
    }

    /// Get the out-of-sample bar range.
    pub fn out_of_sample_range(&self) -> std::ops::Range<usize> {
        self.oos_start..self.oos_end
    }

    /// Number of in-sample bars.
    pub fn is_bars(&self) -> usize {
        self.is_end - self.is_start
    }

    /// Number of out-of-sample bars.
    pub fn oos_bars(&self) -> usize {
        self.oos_end - self.oos_start
    }
}

/// Generate walk-forward folds for a given data length.
///
/// Creates a series of train/test splits where the training window
/// slides forward through time, simulating realistic strategy development.
///
/// # Arguments
/// * `total_bars` - Total number of data points
/// * `config` - Walk-forward configuration
///
/// # Returns
/// Vector of WalkForwardFold structs defining each fold's boundaries
pub fn generate_walk_forward_folds(
    total_bars: usize,
    config: &WalkForwardConfig,
) -> Result<Vec<WalkForwardFold>, ValidationError> {
    config.validate(total_bars)?;

    let mut folds = Vec::new();
    let mut is_start = 0;
    let mut fold_idx = 0;

    loop {
        let is_end = is_start + config.in_sample_bars;
        let oos_start = is_end + config.gap_bars;
        let oos_end = oos_start + config.out_of_sample_bars;

        // Stop if OOS would exceed data
        if oos_end > total_bars {
            break;
        }

        folds.push(WalkForwardFold {
            fold_idx,
            is_start,
            is_end,
            oos_start,
            oos_end,
        });

        fold_idx += 1;
        is_start += config.step_bars;
    }

    if folds.len() < config.min_folds {
        return Err(ValidationError::InsufficientData {
            needed: config.min_folds,
            available: folds.len(),
        });
    }

    Ok(folds)
}

/// Result from a single walk-forward fold evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldResult {
    /// Fold index
    pub fold_idx: usize,
    /// Best in-sample configuration (if multiple were tested)
    pub best_is_config: String,
    /// In-sample Sharpe ratio
    pub is_sharpe: f64,
    /// Out-of-sample Sharpe ratio
    pub oos_sharpe: f64,
    /// In-sample CAGR
    pub is_cagr: f64,
    /// Out-of-sample CAGR
    pub oos_cagr: f64,
    /// In-sample max drawdown
    pub is_max_drawdown: f64,
    /// Out-of-sample max drawdown
    pub oos_max_drawdown: f64,
    /// Number of OOS trades
    pub oos_trades: u32,
}

impl FoldResult {
    /// Compute degradation from IS to OOS.
    ///
    /// Returns the ratio of OOS/IS performance. Values < 1 indicate degradation.
    pub fn sharpe_degradation(&self) -> f64 {
        if self.is_sharpe.abs() < 1e-10 {
            return 1.0;
        }
        self.oos_sharpe / self.is_sharpe
    }

    /// Check if OOS Sharpe is positive.
    pub fn is_oos_profitable(&self) -> bool {
        self.oos_sharpe > 0.0
    }
}

/// Aggregated results from walk-forward analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalkForwardResult {
    /// Configuration used
    pub config: WalkForwardConfig,
    /// Individual fold results
    pub folds: Vec<FoldResult>,
    /// Aggregated OOS Sharpe (mean across folds)
    pub mean_oos_sharpe: f64,
    /// OOS Sharpe standard deviation
    pub std_oos_sharpe: f64,
    /// Aggregated OOS CAGR
    pub mean_oos_cagr: f64,
    /// Aggregated OOS max drawdown
    pub mean_oos_drawdown: f64,
    /// Average Sharpe degradation
    pub mean_degradation: f64,
    /// Percentage of folds with positive OOS Sharpe
    pub pct_profitable_folds: f64,
    /// Total OOS trades across all folds
    pub total_oos_trades: u32,
}

impl WalkForwardResult {
    /// Compute aggregated result from fold results.
    pub fn from_folds(folds: Vec<FoldResult>, config: WalkForwardConfig) -> Self {
        let n = folds.len() as f64;
        if folds.is_empty() {
            return Self {
                config,
                folds: vec![],
                mean_oos_sharpe: 0.0,
                std_oos_sharpe: 0.0,
                mean_oos_cagr: 0.0,
                mean_oos_drawdown: 0.0,
                mean_degradation: 0.0,
                pct_profitable_folds: 0.0,
                total_oos_trades: 0,
            };
        }

        let mean_oos_sharpe = folds.iter().map(|f| f.oos_sharpe).sum::<f64>() / n;
        let mean_oos_cagr = folds.iter().map(|f| f.oos_cagr).sum::<f64>() / n;
        let mean_oos_drawdown = folds.iter().map(|f| f.oos_max_drawdown).sum::<f64>() / n;
        let mean_degradation = folds.iter().map(|f| f.sharpe_degradation()).sum::<f64>() / n;

        let profitable_count = folds.iter().filter(|f| f.is_oos_profitable()).count();
        let pct_profitable_folds = profitable_count as f64 / n;

        let total_oos_trades = folds.iter().map(|f| f.oos_trades).sum();

        // Compute std
        let variance = folds
            .iter()
            .map(|f| (f.oos_sharpe - mean_oos_sharpe).powi(2))
            .sum::<f64>()
            / n;
        let std_oos_sharpe = variance.sqrt();

        Self {
            config,
            folds,
            mean_oos_sharpe,
            std_oos_sharpe,
            mean_oos_cagr,
            mean_oos_drawdown,
            mean_degradation,
            pct_profitable_folds,
            total_oos_trades,
        }
    }

    /// Check if the strategy passes basic walk-forward tests.
    ///
    /// Returns true if:
    /// - Mean OOS Sharpe > 0.3
    /// - At least 60% of folds are profitable
    /// - Average degradation > 0.5 (not losing more than half performance)
    pub fn passes_basic_test(&self) -> bool {
        self.mean_oos_sharpe > 0.3
            && self.pct_profitable_folds >= 0.6
            && self.mean_degradation > 0.5
    }

    /// Check if the strategy passes strict walk-forward tests.
    ///
    /// Returns true if:
    /// - Mean OOS Sharpe > 0.5
    /// - At least 75% of folds are profitable
    /// - Average degradation > 0.7
    /// - OOS Sharpe std < 0.5 (consistent performance)
    pub fn passes_strict_test(&self) -> bool {
        self.mean_oos_sharpe > 0.5
            && self.pct_profitable_folds >= 0.75
            && self.mean_degradation > 0.7
            && self.std_oos_sharpe < 0.5
    }

    /// Get a summary grade (A, B, C, D, F) based on walk-forward results.
    pub fn grade(&self) -> char {
        if self.passes_strict_test() && self.mean_oos_sharpe > 0.8 {
            'A'
        } else if self.passes_strict_test() {
            'B'
        } else if self.passes_basic_test() {
            'C'
        } else if self.mean_oos_sharpe > 0.0 && self.pct_profitable_folds > 0.5 {
            'D'
        } else {
            'F'
        }
    }
}

/// Split a DataFrame into train and test sets based on date.
///
/// # Arguments
/// * `df` - DataFrame with a "ts" or "date" column
/// * `train_end` - End date for training period (exclusive)
/// * `gap_days` - Gap between train and test to avoid lookahead
///
/// # Returns
/// Tuple of (train_df, test_df)
pub fn train_test_split_by_date(
    df: &DataFrame,
    train_end: DateTime<Utc>,
    gap_days: i64,
) -> Result<(DataFrame, DataFrame), ValidationError> {
    // Find the date column
    let date_col = if df.column("ts").is_ok() {
        "ts"
    } else if df.column("date").is_ok() {
        "date"
    } else {
        return Err(ValidationError::MissingColumn(
            "ts or date column required".to_string(),
        ));
    };

    let test_start = train_end + Duration::days(gap_days);

    let train_df = df
        .clone()
        .lazy()
        .filter(col(date_col).lt(lit(train_end.timestamp_millis())))
        .collect()
        .map_err(ValidationError::PolarsError)?;

    let test_df = df
        .clone()
        .lazy()
        .filter(col(date_col).gt_eq(lit(test_start.timestamp_millis())))
        .collect()
        .map_err(ValidationError::PolarsError)?;

    Ok((train_df, test_df))
}

/// Split a DataFrame by row indices.
///
/// # Arguments
/// * `df` - DataFrame to split
/// * `start` - Start row index (inclusive)
/// * `end` - End row index (exclusive)
///
/// # Returns
/// Sliced DataFrame
pub fn slice_by_index(
    df: &DataFrame,
    start: usize,
    end: usize,
) -> Result<DataFrame, ValidationError> {
    let length = end.saturating_sub(start);
    Ok(df.slice(start as i64, length))
}

/// Time-series cross-validation configuration.
#[derive(Debug, Clone)]
pub struct TimeSeriesCVConfig {
    /// Number of folds
    pub n_splits: usize,
    /// Minimum training size (number of bars)
    pub min_train_size: usize,
    /// Maximum training size (None = expanding window)
    pub max_train_size: Option<usize>,
    /// Gap between train and test (number of bars)
    pub gap: usize,
}

impl Default for TimeSeriesCVConfig {
    fn default() -> Self {
        Self {
            n_splits: 5,
            min_train_size: 252,
            max_train_size: None, // Expanding window
            gap: 5,
        }
    }
}

/// A single time-series cross-validation split.
#[derive(Debug, Clone)]
pub struct CVSplit {
    /// Split index
    pub split_idx: usize,
    /// Training indices
    pub train_start: usize,
    pub train_end: usize,
    /// Test indices
    pub test_start: usize,
    pub test_end: usize,
}

/// Generate time-series cross-validation splits.
///
/// Uses expanding or rolling windows for training, ensuring
/// temporal ordering is preserved (no future leakage).
pub fn generate_ts_cv_splits(
    total_bars: usize,
    config: &TimeSeriesCVConfig,
) -> Result<Vec<CVSplit>, ValidationError> {
    if total_bars < config.min_train_size + config.gap + config.n_splits {
        return Err(ValidationError::InsufficientData {
            needed: config.min_train_size + config.gap + config.n_splits,
            available: total_bars,
        });
    }

    let test_size = (total_bars - config.min_train_size - config.gap) / config.n_splits;
    if test_size == 0 {
        return Err(ValidationError::InvalidConfig(
            "Test size would be 0".to_string(),
        ));
    }

    let mut splits = Vec::new();

    for i in 0..config.n_splits {
        let test_start = config.min_train_size + config.gap + i * test_size;
        let test_end = test_start + test_size;

        if test_end > total_bars {
            break;
        }

        let train_end = test_start - config.gap;
        let train_start = match config.max_train_size {
            Some(max) if train_end > max => train_end - max,
            _ => 0, // Expanding window
        };

        splits.push(CVSplit {
            split_idx: i,
            train_start,
            train_end,
            test_start,
            test_end,
        });
    }

    Ok(splits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walk_forward_folds_basic() {
        let config = WalkForwardConfig {
            in_sample_bars: 100,
            out_of_sample_bars: 20,
            gap_bars: 5,
            step_bars: 20,
            min_folds: 2,
        };

        // Need at least 100 + 5 + 20 = 125 bars for first fold
        // With step of 20, second fold needs 125 + 20 = 145 bars
        let folds = generate_walk_forward_folds(200, &config).unwrap();

        assert!(folds.len() >= 2);

        // Check first fold boundaries
        assert_eq!(folds[0].is_start, 0);
        assert_eq!(folds[0].is_end, 100);
        assert_eq!(folds[0].oos_start, 105);
        assert_eq!(folds[0].oos_end, 125);

        // Check second fold
        assert_eq!(folds[1].is_start, 20);
        assert_eq!(folds[1].is_end, 120);
        assert_eq!(folds[1].oos_start, 125);
        assert_eq!(folds[1].oos_end, 145);
    }

    #[test]
    fn test_walk_forward_insufficient_data() {
        let config = WalkForwardConfig {
            in_sample_bars: 100,
            out_of_sample_bars: 50,
            gap_bars: 5,
            step_bars: 20,
            min_folds: 3,
        };

        // Only 120 bars, need 155 for first fold
        let result = generate_walk_forward_folds(120, &config);
        assert!(matches!(result, Err(ValidationError::InsufficientData { .. })));
    }

    #[test]
    fn test_fold_result_degradation() {
        let fold = FoldResult {
            fold_idx: 0,
            best_is_config: "test".to_string(),
            is_sharpe: 1.5,
            oos_sharpe: 0.75,
            is_cagr: 0.15,
            oos_cagr: 0.08,
            is_max_drawdown: 0.10,
            oos_max_drawdown: 0.15,
            oos_trades: 10,
        };

        assert!((fold.sharpe_degradation() - 0.5).abs() < 0.01);
        assert!(fold.is_oos_profitable());
    }

    #[test]
    fn test_walk_forward_result_aggregation() {
        let folds = vec![
            FoldResult {
                fold_idx: 0,
                best_is_config: "c1".to_string(),
                is_sharpe: 1.5,
                oos_sharpe: 0.8,
                is_cagr: 0.15,
                oos_cagr: 0.10,
                is_max_drawdown: 0.10,
                oos_max_drawdown: 0.12,
                oos_trades: 10,
            },
            FoldResult {
                fold_idx: 1,
                best_is_config: "c1".to_string(),
                is_sharpe: 1.4,
                oos_sharpe: 0.6,
                is_cagr: 0.12,
                oos_cagr: 0.08,
                is_max_drawdown: 0.08,
                oos_max_drawdown: 0.10,
                oos_trades: 12,
            },
        ];

        let result = WalkForwardResult::from_folds(folds, WalkForwardConfig::default());

        assert!((result.mean_oos_sharpe - 0.7).abs() < 0.01);
        assert!((result.pct_profitable_folds - 1.0).abs() < 0.01);
        assert_eq!(result.total_oos_trades, 22);
    }

    #[test]
    fn test_walk_forward_grading() {
        let folds = vec![
            FoldResult {
                fold_idx: 0,
                best_is_config: "c1".to_string(),
                is_sharpe: 1.0,
                oos_sharpe: 0.9,
                is_cagr: 0.10,
                oos_cagr: 0.08,
                is_max_drawdown: 0.10,
                oos_max_drawdown: 0.12,
                oos_trades: 10,
            },
            FoldResult {
                fold_idx: 1,
                best_is_config: "c1".to_string(),
                is_sharpe: 1.0,
                oos_sharpe: 0.8,
                is_cagr: 0.10,
                oos_cagr: 0.08,
                is_max_drawdown: 0.10,
                oos_max_drawdown: 0.12,
                oos_trades: 10,
            },
            FoldResult {
                fold_idx: 2,
                best_is_config: "c1".to_string(),
                is_sharpe: 1.0,
                oos_sharpe: 0.85,
                is_cagr: 0.10,
                oos_cagr: 0.08,
                is_max_drawdown: 0.10,
                oos_max_drawdown: 0.12,
                oos_trades: 10,
            },
            FoldResult {
                fold_idx: 3,
                best_is_config: "c1".to_string(),
                is_sharpe: 1.0,
                oos_sharpe: 0.75,
                is_cagr: 0.10,
                oos_cagr: 0.08,
                is_max_drawdown: 0.10,
                oos_max_drawdown: 0.12,
                oos_trades: 10,
            },
        ];

        let result = WalkForwardResult::from_folds(folds, WalkForwardConfig::default());

        // High OOS Sharpe (0.825 avg), 100% profitable, ~82% degradation
        assert!(result.passes_strict_test());
        assert_eq!(result.grade(), 'A'); // > 0.8 and passes strict
    }

    #[test]
    fn test_ts_cv_splits() {
        let config = TimeSeriesCVConfig {
            n_splits: 3,
            min_train_size: 50,
            max_train_size: None,
            gap: 5,
        };

        let splits = generate_ts_cv_splits(200, &config).unwrap();

        assert_eq!(splits.len(), 3);

        // First split: train 0-50, test 55-103
        assert_eq!(splits[0].train_start, 0);
        assert_eq!(splits[0].train_end, 50);

        // Check no overlap between train and test
        for split in &splits {
            assert!(split.train_end + config.gap <= split.test_start);
        }
    }

    #[test]
    fn test_slice_by_index() {
        let df = DataFrame::new(vec![
            Series::new("x".into(), vec![1, 2, 3, 4, 5]).into(),
        ])
        .unwrap();

        let sliced = slice_by_index(&df, 1, 4).unwrap();
        assert_eq!(sliced.height(), 3);

        let x_col = sliced.column("x").unwrap().i32().unwrap();
        assert_eq!(x_col.get(0), Some(2));
        assert_eq!(x_col.get(2), Some(4));
    }
}
