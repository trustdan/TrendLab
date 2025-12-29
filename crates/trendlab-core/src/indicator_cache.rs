//! Indicator caching system for optimized parameter sweeps.
//!
//! This module provides a caching layer that computes each unique indicator only once,
//! then reuses the cached values across multiple strategy configurations.
//!
//! The key insight is that many sweep configurations share the same indicator parameters.
//! For example, 100 MA crossover configs with fast_period=10 (varying slow periods)
//! would normally compute SMA(10) 100 times. With caching, it's computed once.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use trendlab_core::indicator_cache::{IndicatorCache, IndicatorKey};
//!
//! // Create cache with base data
//! let mut cache = IndicatorCache::new(df);
//!
//! // Request indicators (computed on demand, cached)
//! cache.ensure_sma(10);
//! cache.ensure_sma(50);
//! cache.ensure_donchian(20);
//!
//! // Get DataFrame with all cached indicators
//! let df_with_indicators = cache.get_dataframe()?;
//! ```

use polars::prelude::*;
use std::collections::HashSet;

/// Key for cached indicators, used for deduplication.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum IndicatorKey {
    /// Simple Moving Average of close
    SMA { window: usize },
    /// Exponential Moving Average of close
    EMA { window: usize },
    /// Donchian Channel (entry lookback, exit lookback)
    Donchian { lookback: usize },
    /// True Range (no parameters)
    TrueRange,
    /// ATR with SMA smoothing
    ATR { window: usize },
    /// ATR with Wilder smoothing
    ATRWilder { window: usize },
    /// Bollinger Bands
    Bollinger { period: usize, multiplier_x100: i32 },
    /// DMI/ADX indicator set
    DMI { period: usize },
    /// Aroon indicator set
    Aroon { period: usize },
    /// RSI
    RSI { period: usize },
    /// MACD
    MACD {
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
    },
    /// Keltner Channel
    Keltner {
        ema_period: usize,
        atr_period: usize,
        multiplier_x100: i32,
    },
    /// STARC Bands
    Starc {
        sma_period: usize,
        atr_period: usize,
        multiplier_x100: i32,
    },
    /// Supertrend
    Supertrend {
        atr_period: usize,
        multiplier_x100: i32,
    },
    /// Heikin-Ashi candles
    HeikinAshi,
    /// TSMOM lookback
    Tsmom { lookback: usize },
    /// 52-Week High
    FiftyTwoWeekHigh { period: usize },
    /// Rolling Max of High
    RollingMaxHigh { period: usize },
}

impl IndicatorKey {
    /// Generate column name prefix for this indicator.
    pub fn column_prefix(&self) -> String {
        match self {
            IndicatorKey::SMA { window } => format!("sma_{}", window),
            IndicatorKey::EMA { window } => format!("ema_{}", window),
            IndicatorKey::Donchian { lookback } => format!("dc_{}", lookback),
            IndicatorKey::TrueRange => "true_range".to_string(),
            IndicatorKey::ATR { window } => format!("atr_{}", window),
            IndicatorKey::ATRWilder { window } => format!("atr_wilder_{}", window),
            IndicatorKey::Bollinger { period, .. } => format!("bb_{}", period),
            IndicatorKey::DMI { period } => format!("dmi_{}", period),
            IndicatorKey::Aroon { period } => format!("aroon_{}", period),
            IndicatorKey::RSI { period } => format!("rsi_{}", period),
            IndicatorKey::MACD {
                fast_period,
                slow_period,
                ..
            } => {
                format!("macd_{}_{}", fast_period, slow_period)
            }
            IndicatorKey::Keltner { ema_period, .. } => format!("kc_{}", ema_period),
            IndicatorKey::Starc { sma_period, .. } => format!("starc_{}", sma_period),
            IndicatorKey::Supertrend { atr_period, .. } => format!("st_{}", atr_period),
            IndicatorKey::HeikinAshi => "ha".to_string(),
            IndicatorKey::Tsmom { lookback } => format!("tsmom_{}", lookback),
            IndicatorKey::FiftyTwoWeekHigh { period } => format!("52wh_{}", period),
            IndicatorKey::RollingMaxHigh { period } => format!("rmh_{}", period),
        }
    }
}

/// Indicator cache for optimized sweep execution.
///
/// The cache stores computed indicator columns and provides methods to
/// ensure specific indicators exist, computing them on demand if needed.
#[derive(Debug)]
pub struct IndicatorCache {
    /// The base DataFrame with OHLCV data
    df: DataFrame,
    /// Set of indicators that have been computed
    computed: HashSet<IndicatorKey>,
    /// Track columns added to df
    added_columns: Vec<String>,
}

impl IndicatorCache {
    /// Create a new indicator cache from a DataFrame.
    ///
    /// The DataFrame must have columns: ts, open, high, low, close, volume
    pub fn new(df: DataFrame) -> Self {
        Self {
            df,
            computed: HashSet::new(),
            added_columns: Vec::new(),
        }
    }

    /// Get a reference to the current DataFrame.
    pub fn dataframe(&self) -> &DataFrame {
        &self.df
    }

    /// Get a clone of the DataFrame (for backtest execution).
    pub fn clone_dataframe(&self) -> DataFrame {
        self.df.clone()
    }

    /// Check if an indicator has been computed.
    pub fn has_indicator(&self, key: &IndicatorKey) -> bool {
        self.computed.contains(key)
    }

    /// Get the list of added indicator columns.
    pub fn added_columns(&self) -> &[String] {
        &self.added_columns
    }

    /// Ensure True Range is computed.
    pub fn ensure_true_range(&mut self) -> PolarsResult<()> {
        let key = IndicatorKey::TrueRange;
        if self.computed.contains(&key) {
            return Ok(());
        }

        use crate::indicators_polars::true_range_expr;
        let lf = self.df.clone().lazy().with_column(true_range_expr());
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push("true_range".to_string());
        Ok(())
    }

    /// Ensure SMA is computed for the specified window.
    pub fn ensure_sma(&mut self, window: usize) -> PolarsResult<()> {
        let key = IndicatorKey::SMA { window };
        if self.computed.contains(&key) {
            return Ok(());
        }

        use crate::indicators_polars::sma_close_expr;
        let col_name = format!("sma_{}", window);
        let lf = self
            .df
            .clone()
            .lazy()
            .with_column(sma_close_expr(window).alias(&col_name));
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(col_name);
        Ok(())
    }

    /// Ensure EMA is computed for the specified window.
    pub fn ensure_ema(&mut self, window: usize) -> PolarsResult<()> {
        let key = IndicatorKey::EMA { window };
        if self.computed.contains(&key) {
            return Ok(());
        }

        use crate::indicators_polars::ema_close_expr;
        let col_name = format!("ema_{}", window);
        let lf = self
            .df
            .clone()
            .lazy()
            .with_column(ema_close_expr(window).alias(&col_name));
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(col_name);
        Ok(())
    }

    /// Ensure Donchian channel is computed for the specified lookback.
    pub fn ensure_donchian(&mut self, lookback: usize) -> PolarsResult<()> {
        let key = IndicatorKey::Donchian { lookback };
        if self.computed.contains(&key) {
            return Ok(());
        }

        use crate::indicators_polars::donchian_channel_exprs;
        let (upper, lower) = donchian_channel_exprs(lookback);
        let upper_name = format!("dc_{}_upper", lookback);
        let lower_name = format!("dc_{}_lower", lookback);

        let lf = self
            .df
            .clone()
            .lazy()
            .with_columns([upper.alias(&upper_name), lower.alias(&lower_name)]);
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(upper_name);
        self.added_columns.push(lower_name);
        Ok(())
    }

    /// Ensure ATR (SMA-based) is computed.
    pub fn ensure_atr(&mut self, window: usize) -> PolarsResult<()> {
        let key = IndicatorKey::ATR { window };
        if self.computed.contains(&key) {
            return Ok(());
        }

        // ATR requires true range
        self.ensure_true_range()?;

        use crate::indicators_polars::atr_sma_expr;
        let col_name = format!("atr_{}", window);
        let lf = self
            .df
            .clone()
            .lazy()
            .with_column(atr_sma_expr(window).alias(&col_name));
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(col_name);
        Ok(())
    }

    /// Ensure ATR (Wilder smoothing) is computed.
    pub fn ensure_atr_wilder(&mut self, window: usize) -> PolarsResult<()> {
        let key = IndicatorKey::ATRWilder { window };
        if self.computed.contains(&key) {
            return Ok(());
        }

        // ATR requires true range
        self.ensure_true_range()?;

        use crate::indicators_polars::atr_wilder_expr;
        let col_name = format!("atr_wilder_{}", window);
        let lf = self
            .df
            .clone()
            .lazy()
            .with_column(atr_wilder_expr(window).alias(&col_name));
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(col_name);
        Ok(())
    }

    /// Ensure TSMOM lookback column is computed (close N bars ago).
    pub fn ensure_tsmom(&mut self, lookback: usize) -> PolarsResult<()> {
        let key = IndicatorKey::Tsmom { lookback };
        if self.computed.contains(&key) {
            return Ok(());
        }

        let col_name = format!("close_lag_{}", lookback);
        let lf = self
            .df
            .clone()
            .lazy()
            .with_column(col("close").shift(lit(lookback as i64)).alias(&col_name));
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(col_name);
        Ok(())
    }

    /// Ensure Rolling Max of High is computed.
    pub fn ensure_rolling_max_high(&mut self, period: usize) -> PolarsResult<()> {
        let key = IndicatorKey::RollingMaxHigh { period };
        if self.computed.contains(&key) {
            return Ok(());
        }

        let col_name = format!("max_high_{}", period);
        let lf = self.df.clone().lazy().with_column(
            col("high")
                .rolling_max(RollingOptionsFixedWindow {
                    window_size: period,
                    min_periods: period,
                    weights: None,
                    center: false,
                    fn_params: None,
                })
                .alias(&col_name),
        );
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(col_name);
        Ok(())
    }

    /// Ensure RSI is computed.
    pub fn ensure_rsi(&mut self, period: usize) -> PolarsResult<()> {
        let key = IndicatorKey::RSI { period };
        if self.computed.contains(&key) {
            return Ok(());
        }

        use crate::indicators_polars::apply_rsi_exprs;
        let col_name = format!("rsi_{}", period);
        // RSI adds 'rsi' column, we need to rename for caching
        let lf = apply_rsi_exprs(self.df.clone().lazy(), period);
        let lf = lf.rename(["rsi"], [&col_name], false);
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(col_name);
        Ok(())
    }

    /// Ensure MACD is computed.
    pub fn ensure_macd(
        &mut self,
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
    ) -> PolarsResult<()> {
        let key = IndicatorKey::MACD {
            fast_period,
            slow_period,
            signal_period,
        };
        if self.computed.contains(&key) {
            return Ok(());
        }

        use crate::indicators_polars::apply_macd_exprs;
        let prefix = format!("macd_{}_{}_{}", fast_period, slow_period, signal_period);
        let line_name = format!("{}_line", prefix);
        let signal_name = format!("{}_signal", prefix);
        let hist_name = format!("{}_histogram", prefix);

        let lf = apply_macd_exprs(
            self.df.clone().lazy(),
            fast_period,
            slow_period,
            signal_period,
        );
        let lf = lf.rename(
            ["macd_line", "macd_signal", "macd_histogram"],
            [&line_name, &signal_name, &hist_name],
            false,
        );
        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(line_name);
        self.added_columns.push(signal_name);
        self.added_columns.push(hist_name);
        Ok(())
    }

    /// Ensure Bollinger Bands are computed.
    ///
    /// Adds columns: bb_{period}_middle, bb_{period}_upper, bb_{period}_lower, bb_{period}_bandwidth
    pub fn ensure_bollinger(&mut self, period: usize, multiplier: f64) -> PolarsResult<()> {
        let multiplier_x100 = (multiplier * 100.0) as i32;
        let key = IndicatorKey::Bollinger {
            period,
            multiplier_x100,
        };
        if self.computed.contains(&key) {
            return Ok(());
        }

        use crate::indicators_polars::bollinger_bands_exprs;
        let prefix = format!("bb_{}", period);
        let middle_name = format!("{}_middle", prefix);
        let upper_name = format!("{}_upper", prefix);
        let lower_name = format!("{}_lower", prefix);
        let bandwidth_name = format!("{}_bandwidth", prefix);

        let (middle, upper, lower, _) = bollinger_bands_exprs(period, multiplier);

        // Apply middle, upper, lower first
        let lf = self.df.clone().lazy().with_columns([
            middle.alias(&middle_name),
            upper.alias(&upper_name),
            lower.alias(&lower_name),
        ]);
        // Then compute bandwidth (depends on the columns just added)
        let lf = lf.with_column(
            ((col(&upper_name) - col(&lower_name)) / col(&middle_name)).alias(&bandwidth_name),
        );

        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(middle_name);
        self.added_columns.push(upper_name);
        self.added_columns.push(lower_name);
        self.added_columns.push(bandwidth_name);
        Ok(())
    }

    /// Ensure Keltner Channel is computed.
    ///
    /// Adds columns: kc_{ema}_{atr}_center, kc_{ema}_{atr}_upper, kc_{ema}_{atr}_lower
    /// Requires true_range as dependency.
    pub fn ensure_keltner(
        &mut self,
        ema_period: usize,
        atr_period: usize,
        multiplier: f64,
    ) -> PolarsResult<()> {
        let multiplier_x100 = (multiplier * 100.0) as i32;
        let key = IndicatorKey::Keltner {
            ema_period,
            atr_period,
            multiplier_x100,
        };
        if self.computed.contains(&key) {
            return Ok(());
        }

        // Keltner requires true_range for ATR
        self.ensure_true_range()?;

        use crate::indicators_polars::keltner_channel_exprs;
        let prefix = format!("kc_{}_{}", ema_period, atr_period);
        let center_name = format!("{}_center", prefix);
        let upper_name = format!("{}_upper", prefix);
        let lower_name = format!("{}_lower", prefix);

        let (center, upper, lower) = keltner_channel_exprs(ema_period, atr_period, multiplier);

        let lf = self.df.clone().lazy().with_columns([
            center.alias(&center_name),
            upper.alias(&upper_name),
            lower.alias(&lower_name),
        ]);

        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(center_name);
        self.added_columns.push(upper_name);
        self.added_columns.push(lower_name);
        Ok(())
    }

    /// Ensure STARC Bands are computed.
    ///
    /// Adds columns: starc_{sma}_{atr}_center, starc_{sma}_{atr}_upper, starc_{sma}_{atr}_lower
    /// Requires true_range as dependency.
    pub fn ensure_starc(
        &mut self,
        sma_period: usize,
        atr_period: usize,
        multiplier: f64,
    ) -> PolarsResult<()> {
        let multiplier_x100 = (multiplier * 100.0) as i32;
        let key = IndicatorKey::Starc {
            sma_period,
            atr_period,
            multiplier_x100,
        };
        if self.computed.contains(&key) {
            return Ok(());
        }

        // STARC requires true_range for ATR
        self.ensure_true_range()?;

        use crate::indicators_polars::starc_bands_exprs;
        let prefix = format!("starc_{}_{}", sma_period, atr_period);
        let center_name = format!("{}_center", prefix);
        let upper_name = format!("{}_upper", prefix);
        let lower_name = format!("{}_lower", prefix);

        let (center, upper, lower) = starc_bands_exprs(sma_period, atr_period, multiplier);

        let lf = self.df.clone().lazy().with_columns([
            center.alias(&center_name),
            upper.alias(&upper_name),
            lower.alias(&lower_name),
        ]);

        self.df = lf.collect()?;
        self.computed.insert(key);
        self.added_columns.push(center_name);
        self.added_columns.push(upper_name);
        self.added_columns.push(lower_name);
        Ok(())
    }

    /// Compute all requested indicators at once for a batch of configs.
    ///
    /// This collects all unique indicator requirements and computes them in
    /// an optimized order (dependencies first).
    pub fn ensure_all(&mut self, keys: &[IndicatorKey]) -> PolarsResult<()> {
        for key in keys {
            if self.computed.contains(key) {
                continue;
            }

            match key {
                IndicatorKey::TrueRange => self.ensure_true_range()?,
                IndicatorKey::SMA { window } => self.ensure_sma(*window)?,
                IndicatorKey::EMA { window } => self.ensure_ema(*window)?,
                IndicatorKey::Donchian { lookback } => self.ensure_donchian(*lookback)?,
                IndicatorKey::ATR { window } => self.ensure_atr(*window)?,
                IndicatorKey::ATRWilder { window } => self.ensure_atr_wilder(*window)?,
                IndicatorKey::Tsmom { lookback } => self.ensure_tsmom(*lookback)?,
                IndicatorKey::RollingMaxHigh { period } => self.ensure_rolling_max_high(*period)?,
                IndicatorKey::RSI { period } => self.ensure_rsi(*period)?,
                IndicatorKey::MACD {
                    fast_period,
                    slow_period,
                    signal_period,
                } => self.ensure_macd(*fast_period, *slow_period, *signal_period)?,
                IndicatorKey::Bollinger {
                    period,
                    multiplier_x100,
                } => self.ensure_bollinger(*period, *multiplier_x100 as f64 / 100.0)?,
                IndicatorKey::Keltner {
                    ema_period,
                    atr_period,
                    multiplier_x100,
                } => {
                    self.ensure_keltner(*ema_period, *atr_period, *multiplier_x100 as f64 / 100.0)?
                }
                IndicatorKey::Starc {
                    sma_period,
                    atr_period,
                    multiplier_x100,
                } => {
                    self.ensure_starc(*sma_period, *atr_period, *multiplier_x100 as f64 / 100.0)?
                }
                // For complex indicators that need special handling, compute inline
                IndicatorKey::DMI { .. }
                | IndicatorKey::Aroon { .. }
                | IndicatorKey::Supertrend { .. }
                | IndicatorKey::HeikinAshi
                | IndicatorKey::FiftyTwoWeekHigh { .. } => {
                    // These are more complex and typically less commonly shared
                    // Fall back to strategy-specific computation for now
                    // TODO: Implement caching for these if profiling shows benefit
                }
            }
        }
        Ok(())
    }

    /// Compute indicators in batches to minimize DataFrame materializations.
    ///
    /// This groups compatible indicators (those that don't have dependencies on each other)
    /// and computes them in a single pass. This is more efficient than computing each
    /// indicator separately.
    ///
    /// Batching strategy:
    /// 1. First batch: TrueRange (needed by ATR variants)
    /// 2. Second batch: All SMA, EMA, Donchian, TSMOM, RollingMaxHigh (independent)
    /// 3. Third batch: ATR, ATRWilder (depend on TrueRange)
    /// 4. Remaining: Complex indicators computed individually
    pub fn ensure_all_batched(&mut self, keys: &[IndicatorKey]) -> PolarsResult<()> {
        use crate::indicators_polars::{donchian_channel_exprs, ema_close_expr, sma_close_expr};

        // Separate keys by category
        let mut need_true_range = false;
        let mut sma_windows: Vec<usize> = Vec::new();
        let mut ema_windows: Vec<usize> = Vec::new();
        let mut donchian_lookbacks: Vec<usize> = Vec::new();
        let mut tsmom_lookbacks: Vec<usize> = Vec::new();
        let mut rolling_max_periods: Vec<usize> = Vec::new();
        let mut atr_windows: Vec<usize> = Vec::new();
        let mut atr_wilder_windows: Vec<usize> = Vec::new();
        let mut complex_keys: Vec<IndicatorKey> = Vec::new();

        for key in keys {
            if self.computed.contains(key) {
                continue;
            }

            match key {
                IndicatorKey::TrueRange => need_true_range = true,
                IndicatorKey::SMA { window } => sma_windows.push(*window),
                IndicatorKey::EMA { window } => ema_windows.push(*window),
                IndicatorKey::Donchian { lookback } => donchian_lookbacks.push(*lookback),
                IndicatorKey::Tsmom { lookback } => tsmom_lookbacks.push(*lookback),
                IndicatorKey::RollingMaxHigh { period } => rolling_max_periods.push(*period),
                IndicatorKey::ATR { window } => {
                    need_true_range = true;
                    atr_windows.push(*window);
                }
                IndicatorKey::ATRWilder { window } => {
                    need_true_range = true;
                    atr_wilder_windows.push(*window);
                }
                IndicatorKey::RSI { period } => {
                    // RSI is complex, handle individually
                    complex_keys.push(IndicatorKey::RSI { period: *period });
                }
                IndicatorKey::MACD {
                    fast_period,
                    slow_period,
                    signal_period,
                } => {
                    complex_keys.push(IndicatorKey::MACD {
                        fast_period: *fast_period,
                        slow_period: *slow_period,
                        signal_period: *signal_period,
                    });
                }
                other => complex_keys.push(other.clone()),
            }
        }

        // Batch 1: TrueRange if needed
        if need_true_range && !self.computed.contains(&IndicatorKey::TrueRange) {
            self.ensure_true_range()?;
        }

        // Batch 2: All independent indicators in one LazyFrame pass
        let mut exprs: Vec<Expr> = Vec::new();
        let mut batch_columns: Vec<String> = Vec::new();
        let mut batch_keys: Vec<IndicatorKey> = Vec::new();

        // Deduplicate
        sma_windows.sort();
        sma_windows.dedup();
        ema_windows.sort();
        ema_windows.dedup();
        donchian_lookbacks.sort();
        donchian_lookbacks.dedup();
        tsmom_lookbacks.sort();
        tsmom_lookbacks.dedup();
        rolling_max_periods.sort();
        rolling_max_periods.dedup();

        for window in sma_windows {
            let col_name = format!("sma_{}", window);
            exprs.push(sma_close_expr(window).alias(&col_name));
            batch_columns.push(col_name);
            batch_keys.push(IndicatorKey::SMA { window });
        }

        for window in ema_windows {
            let col_name = format!("ema_{}", window);
            exprs.push(ema_close_expr(window).alias(&col_name));
            batch_columns.push(col_name);
            batch_keys.push(IndicatorKey::EMA { window });
        }

        for lookback in donchian_lookbacks {
            let (upper, lower) = donchian_channel_exprs(lookback);
            let upper_name = format!("dc_{}_upper", lookback);
            let lower_name = format!("dc_{}_lower", lookback);
            exprs.push(upper.alias(&upper_name));
            exprs.push(lower.alias(&lower_name));
            batch_columns.push(upper_name);
            batch_columns.push(lower_name);
            batch_keys.push(IndicatorKey::Donchian { lookback });
        }

        for lookback in tsmom_lookbacks {
            let col_name = format!("close_lag_{}", lookback);
            exprs.push(col("close").shift(lit(lookback as i64)).alias(&col_name));
            batch_columns.push(col_name);
            batch_keys.push(IndicatorKey::Tsmom { lookback });
        }

        for period in rolling_max_periods {
            let col_name = format!("max_high_{}", period);
            exprs.push(
                col("high")
                    .rolling_max(RollingOptionsFixedWindow {
                        window_size: period,
                        min_periods: period,
                        weights: None,
                        center: false,
                        fn_params: None,
                    })
                    .alias(&col_name),
            );
            batch_columns.push(col_name);
            batch_keys.push(IndicatorKey::RollingMaxHigh { period });
        }

        // Apply batch 2 in single pass
        if !exprs.is_empty() {
            let lf = self.df.clone().lazy().with_columns(exprs);
            self.df = lf.collect()?;
            for col_name in batch_columns {
                self.added_columns.push(col_name);
            }
            for key in batch_keys {
                self.computed.insert(key);
            }
        }

        // Batch 3: ATR variants (depend on true_range)
        atr_windows.sort();
        atr_windows.dedup();
        atr_wilder_windows.sort();
        atr_wilder_windows.dedup();

        let mut atr_exprs: Vec<Expr> = Vec::new();
        let mut atr_columns: Vec<String> = Vec::new();
        let mut atr_keys: Vec<IndicatorKey> = Vec::new();

        for window in atr_windows {
            use crate::indicators_polars::atr_sma_expr;
            let col_name = format!("atr_{}", window);
            atr_exprs.push(atr_sma_expr(window).alias(&col_name));
            atr_columns.push(col_name);
            atr_keys.push(IndicatorKey::ATR { window });
        }

        for window in atr_wilder_windows {
            use crate::indicators_polars::atr_wilder_expr;
            let col_name = format!("atr_wilder_{}", window);
            atr_exprs.push(atr_wilder_expr(window).alias(&col_name));
            atr_columns.push(col_name);
            atr_keys.push(IndicatorKey::ATRWilder { window });
        }

        if !atr_exprs.is_empty() {
            let lf = self.df.clone().lazy().with_columns(atr_exprs);
            self.df = lf.collect()?;
            for col_name in atr_columns {
                self.added_columns.push(col_name);
            }
            for key in atr_keys {
                self.computed.insert(key);
            }
        }

        // Batch 4: Complex indicators (computed individually)
        for key in complex_keys {
            match key {
                IndicatorKey::RSI { period } => self.ensure_rsi(period)?,
                IndicatorKey::MACD {
                    fast_period,
                    slow_period,
                    signal_period,
                } => self.ensure_macd(fast_period, slow_period, signal_period)?,
                IndicatorKey::Bollinger {
                    period,
                    multiplier_x100,
                } => self.ensure_bollinger(period, multiplier_x100 as f64 / 100.0)?,
                IndicatorKey::Keltner {
                    ema_period,
                    atr_period,
                    multiplier_x100,
                } => self.ensure_keltner(ema_period, atr_period, multiplier_x100 as f64 / 100.0)?,
                IndicatorKey::Starc {
                    sma_period,
                    atr_period,
                    multiplier_x100,
                } => self.ensure_starc(sma_period, atr_period, multiplier_x100 as f64 / 100.0)?,
                _ => {} // Skip unsupported complex indicators
            }
        }

        Ok(())
    }

    /// Get statistics about cache usage.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            num_indicators_cached: self.computed.len(),
            num_columns_added: self.added_columns.len(),
            dataframe_height: self.df.height(),
            dataframe_width: self.df.width(),
        }
    }
}

/// Statistics about indicator cache usage.
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of unique indicators cached
    pub num_indicators_cached: usize,
    /// Number of columns added to the DataFrame
    pub num_columns_added: usize,
    /// Number of rows in the DataFrame
    pub dataframe_height: usize,
    /// Number of columns in the DataFrame (including original + cached)
    pub dataframe_width: usize,
}

/// Lazy indicator cache that defers computation until collection.
///
/// Unlike `IndicatorCache` which materializes after each batch,
/// `LazyIndicatorCache` builds up expressions and only collects once.
/// This is more efficient for large sweeps where all indicators are known upfront.
///
/// ## Usage
///
/// ```rust,ignore
/// use trendlab_core::indicator_cache::{LazyIndicatorCache, IndicatorKey};
///
/// let cache = LazyIndicatorCache::new(lf);
/// let cache = cache
///     .with_sma(10)
///     .with_sma(50)
///     .with_donchian(20)
///     .with_atr(14);
///
/// // Single collect at the end
/// let df = cache.collect()?;
/// ```
pub struct LazyIndicatorCache {
    lf: LazyFrame,
    pending_keys: HashSet<IndicatorKey>,
    has_true_range: bool,
}

impl LazyIndicatorCache {
    /// Create a new lazy indicator cache from a LazyFrame.
    pub fn new(lf: LazyFrame) -> Self {
        Self {
            lf,
            pending_keys: HashSet::new(),
            has_true_range: false,
        }
    }

    /// Add SMA to the pending indicators.
    pub fn with_sma(mut self, window: usize) -> Self {
        self.pending_keys.insert(IndicatorKey::SMA { window });
        self
    }

    /// Add EMA to the pending indicators.
    pub fn with_ema(mut self, window: usize) -> Self {
        self.pending_keys.insert(IndicatorKey::EMA { window });
        self
    }

    /// Add Donchian channel to the pending indicators.
    pub fn with_donchian(mut self, lookback: usize) -> Self {
        self.pending_keys
            .insert(IndicatorKey::Donchian { lookback });
        self
    }

    /// Add ATR to the pending indicators.
    pub fn with_atr(mut self, window: usize) -> Self {
        self.pending_keys.insert(IndicatorKey::ATR { window });
        self.has_true_range = true; // ATR depends on true range
        self
    }

    /// Add ATR with Wilder smoothing to the pending indicators.
    pub fn with_atr_wilder(mut self, window: usize) -> Self {
        self.pending_keys.insert(IndicatorKey::ATRWilder { window });
        self.has_true_range = true;
        self
    }

    /// Add TSMOM lookback to the pending indicators.
    pub fn with_tsmom(mut self, lookback: usize) -> Self {
        self.pending_keys.insert(IndicatorKey::Tsmom { lookback });
        self
    }

    /// Add rolling max of high to the pending indicators.
    pub fn with_rolling_max_high(mut self, period: usize) -> Self {
        self.pending_keys
            .insert(IndicatorKey::RollingMaxHigh { period });
        self
    }

    /// Add Bollinger Bands to the pending indicators.
    pub fn with_bollinger(mut self, period: usize, multiplier: f64) -> Self {
        let multiplier_x100 = (multiplier * 100.0) as i32;
        self.pending_keys.insert(IndicatorKey::Bollinger {
            period,
            multiplier_x100,
        });
        self
    }

    /// Add Keltner Channel to the pending indicators.
    pub fn with_keltner(mut self, ema_period: usize, atr_period: usize, multiplier: f64) -> Self {
        let multiplier_x100 = (multiplier * 100.0) as i32;
        self.pending_keys.insert(IndicatorKey::Keltner {
            ema_period,
            atr_period,
            multiplier_x100,
        });
        self.has_true_range = true; // Keltner depends on ATR which needs true_range
        self
    }

    /// Add STARC Bands to the pending indicators.
    pub fn with_starc(mut self, sma_period: usize, atr_period: usize, multiplier: f64) -> Self {
        let multiplier_x100 = (multiplier * 100.0) as i32;
        self.pending_keys.insert(IndicatorKey::Starc {
            sma_period,
            atr_period,
            multiplier_x100,
        });
        self.has_true_range = true; // STARC depends on ATR which needs true_range
        self
    }

    /// Add all indicators from a list of keys.
    pub fn with_all(mut self, keys: &[IndicatorKey]) -> Self {
        for key in keys {
            self.pending_keys.insert(key.clone());
            // Track true range dependency
            if matches!(
                key,
                IndicatorKey::ATR { .. }
                    | IndicatorKey::ATRWilder { .. }
                    | IndicatorKey::Keltner { .. }
                    | IndicatorKey::Starc { .. }
            ) {
                self.has_true_range = true;
            }
        }
        self
    }

    /// Build all pending indicators as expressions and return a new LazyFrame.
    ///
    /// This does NOT collect - it returns a LazyFrame with all indicator columns added.
    /// Call `.collect()` on the result when you need a DataFrame.
    pub fn build(self) -> LazyFrame {
        use crate::indicators_polars::{
            atr_sma_expr, atr_wilder_expr, donchian_channel_exprs, ema_close_expr, sma_close_expr,
            true_range_expr,
        };

        let mut lf = self.lf;
        let mut exprs: Vec<Expr> = Vec::new();

        // Add true range first if needed
        if self.has_true_range {
            lf = lf.with_column(true_range_expr());
        }

        // Collect simple indicators (no dependencies after true_range)
        for key in &self.pending_keys {
            match key {
                IndicatorKey::SMA { window } => {
                    let col_name = format!("sma_{}", window);
                    exprs.push(sma_close_expr(*window).alias(&col_name));
                }
                IndicatorKey::EMA { window } => {
                    let col_name = format!("ema_{}", window);
                    exprs.push(ema_close_expr(*window).alias(&col_name));
                }
                IndicatorKey::Donchian { lookback } => {
                    let (upper, lower) = donchian_channel_exprs(*lookback);
                    exprs.push(upper.alias(format!("dc_{}_upper", lookback)));
                    exprs.push(lower.alias(format!("dc_{}_lower", lookback)));
                }
                IndicatorKey::Tsmom { lookback } => {
                    let col_name = format!("close_lag_{}", lookback);
                    exprs.push(col("close").shift(lit(*lookback as i64)).alias(&col_name));
                }
                IndicatorKey::RollingMaxHigh { period } => {
                    let col_name = format!("max_high_{}", period);
                    exprs.push(
                        col("high")
                            .rolling_max(RollingOptionsFixedWindow {
                                window_size: *period,
                                min_periods: *period,
                                weights: None,
                                center: false,
                                fn_params: None,
                            })
                            .alias(&col_name),
                    );
                }
                // Skip ATR variants and others for this pass
                _ => {}
            }
        }

        // Apply simple indicators in one pass
        if !exprs.is_empty() {
            lf = lf.with_columns(exprs);
        }

        // Collect ATR variants (depend on true_range)
        let mut atr_exprs: Vec<Expr> = Vec::new();
        for key in &self.pending_keys {
            match key {
                IndicatorKey::ATR { window } => {
                    let col_name = format!("atr_{}", window);
                    atr_exprs.push(atr_sma_expr(*window).alias(&col_name));
                }
                IndicatorKey::ATRWilder { window } => {
                    let col_name = format!("atr_wilder_{}", window);
                    atr_exprs.push(atr_wilder_expr(*window).alias(&col_name));
                }
                _ => {}
            }
        }

        if !atr_exprs.is_empty() {
            lf = lf.with_columns(atr_exprs);
        }

        // Collect channel indicators (Bollinger, Keltner, STARC)
        // These need to be computed individually due to complex dependencies
        for key in &self.pending_keys {
            match key {
                IndicatorKey::Bollinger {
                    period,
                    multiplier_x100,
                } => {
                    use crate::indicators_polars::bollinger_bands_exprs;
                    let multiplier = *multiplier_x100 as f64 / 100.0;
                    let prefix = format!("bb_{}", period);
                    let (middle, upper, lower, _) = bollinger_bands_exprs(*period, multiplier);

                    lf = lf.with_columns([
                        middle.alias(format!("{}_middle", prefix)),
                        upper.alias(format!("{}_upper", prefix)),
                        lower.alias(format!("{}_lower", prefix)),
                    ]);
                    // Bandwidth computed in a second pass
                    lf = lf.with_column(
                        ((col(format!("{}_upper", prefix)) - col(format!("{}_lower", prefix)))
                            / col(format!("{}_middle", prefix)))
                        .alias(format!("{}_bandwidth", prefix)),
                    );
                }
                IndicatorKey::Keltner {
                    ema_period,
                    atr_period,
                    multiplier_x100,
                } => {
                    use crate::indicators_polars::keltner_channel_exprs;
                    let multiplier = *multiplier_x100 as f64 / 100.0;
                    let prefix = format!("kc_{}_{}", ema_period, atr_period);
                    let (center, upper, lower) =
                        keltner_channel_exprs(*ema_period, *atr_period, multiplier);

                    lf = lf.with_columns([
                        center.alias(format!("{}_center", prefix)),
                        upper.alias(format!("{}_upper", prefix)),
                        lower.alias(format!("{}_lower", prefix)),
                    ]);
                }
                IndicatorKey::Starc {
                    sma_period,
                    atr_period,
                    multiplier_x100,
                } => {
                    use crate::indicators_polars::starc_bands_exprs;
                    let multiplier = *multiplier_x100 as f64 / 100.0;
                    let prefix = format!("starc_{}_{}", sma_period, atr_period);
                    let (center, upper, lower) =
                        starc_bands_exprs(*sma_period, *atr_period, multiplier);

                    lf = lf.with_columns([
                        center.alias(format!("{}_center", prefix)),
                        upper.alias(format!("{}_upper", prefix)),
                        lower.alias(format!("{}_lower", prefix)),
                    ]);
                }
                _ => {}
            }
        }

        lf
    }

    /// Build and collect the LazyFrame into a DataFrame.
    pub fn collect(self) -> PolarsResult<DataFrame> {
        self.build().collect()
    }

    /// Get the number of pending indicators.
    pub fn pending_count(&self) -> usize {
        self.pending_keys.len()
    }
}

/// Extract indicator requirements from a strategy specification.
///
/// Returns the set of indicators needed to compute signals for this strategy.
pub fn extract_indicator_requirements(
    spec: &crate::strategy_v2::StrategySpec,
) -> Vec<IndicatorKey> {
    use crate::strategy_v2::StrategySpec;

    match spec {
        StrategySpec::DonchianBreakout {
            entry_lookback,
            exit_lookback,
        } => {
            vec![
                IndicatorKey::Donchian {
                    lookback: *entry_lookback,
                },
                IndicatorKey::Donchian {
                    lookback: *exit_lookback,
                },
            ]
        }
        StrategySpec::MACrossover {
            fast_period,
            slow_period,
            ma_type,
        } => {
            use crate::indicators::MAType;
            match ma_type {
                MAType::SMA => vec![
                    IndicatorKey::SMA {
                        window: *fast_period,
                    },
                    IndicatorKey::SMA {
                        window: *slow_period,
                    },
                ],
                MAType::EMA => vec![
                    IndicatorKey::EMA {
                        window: *fast_period,
                    },
                    IndicatorKey::EMA {
                        window: *slow_period,
                    },
                ],
            }
        }
        StrategySpec::Tsmom { lookback } => {
            vec![IndicatorKey::Tsmom {
                lookback: *lookback,
            }]
        }
        StrategySpec::FiftyTwoWeekHigh { period, .. } => {
            vec![IndicatorKey::RollingMaxHigh { period: *period }]
        }
        StrategySpec::DmiAdx { di_period, .. } => {
            vec![IndicatorKey::DMI { period: *di_period }]
        }
        StrategySpec::BollingerSqueeze {
            period, std_mult, ..
        } => {
            vec![IndicatorKey::Bollinger {
                period: *period,
                multiplier_x100: (*std_mult * 100.0) as i32,
            }]
        }
        StrategySpec::Aroon { period } => {
            vec![IndicatorKey::Aroon { period: *period }]
        }
        StrategySpec::Keltner {
            ema_period,
            atr_period,
            multiplier,
        } => {
            vec![IndicatorKey::Keltner {
                ema_period: *ema_period,
                atr_period: *atr_period,
                multiplier_x100: (*multiplier * 100.0) as i32,
            }]
        }
        StrategySpec::Starc {
            sma_period,
            atr_period,
            multiplier,
        } => {
            vec![IndicatorKey::Starc {
                sma_period: *sma_period,
                atr_period: *atr_period,
                multiplier_x100: (*multiplier * 100.0) as i32,
            }]
        }
        StrategySpec::Supertrend {
            atr_period,
            multiplier,
        } => {
            vec![IndicatorKey::Supertrend {
                atr_period: *atr_period,
                multiplier_x100: (*multiplier * 100.0) as i32,
            }]
        }
        StrategySpec::HeikinAshi { .. } => {
            vec![IndicatorKey::HeikinAshi]
        }
        StrategySpec::Rsi { period, .. } => {
            vec![IndicatorKey::RSI { period: *period }]
        }
        StrategySpec::Macd {
            fast_period,
            slow_period,
            signal_period,
            ..
        } => {
            vec![IndicatorKey::MACD {
                fast_period: *fast_period,
                slow_period: *slow_period,
                signal_period: *signal_period,
            }]
        }
        StrategySpec::Ensemble { children, .. } => {
            // Collect requirements from all children
            let mut requirements = Vec::new();
            for child in children {
                requirements.extend(extract_indicator_requirements(child));
            }
            requirements
        }
        // For strategies not yet optimized, return empty
        _ => Vec::new(),
    }
}

/// Collect all unique indicator requirements from a set of strategy specifications.
pub fn collect_indicator_requirements(
    specs: &[crate::strategy_v2::StrategySpec],
) -> Vec<IndicatorKey> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for spec in specs {
        for key in extract_indicator_requirements(spec) {
            if seen.insert(key.clone()) {
                result.push(key);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::Bar;
    use crate::data::bars_to_dataframe;
    use chrono::{TimeZone, Utc};

    fn make_test_bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
                    + chrono::Duration::days(i as i64);
                let base = 100.0 + i as f64;
                Bar::new(
                    ts,
                    base,
                    base + 2.0,
                    base - 1.0,
                    base + 1.0,
                    1000.0,
                    "TEST",
                    "1d",
                )
            })
            .collect()
    }

    #[test]
    fn test_indicator_cache_sma() {
        let bars = make_test_bars(20);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        // Compute SMA(10)
        cache.ensure_sma(10).unwrap();
        assert!(cache.has_indicator(&IndicatorKey::SMA { window: 10 }));
        assert!(cache.dataframe().column("sma_10").is_ok());

        // Second call should be no-op
        cache.ensure_sma(10).unwrap();
        assert_eq!(cache.added_columns().len(), 1);

        // Different window creates new column
        cache.ensure_sma(5).unwrap();
        assert!(cache.has_indicator(&IndicatorKey::SMA { window: 5 }));
        assert_eq!(cache.added_columns().len(), 2);
    }

    #[test]
    fn test_indicator_cache_donchian() {
        let bars = make_test_bars(30);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        cache.ensure_donchian(10).unwrap();
        assert!(cache.has_indicator(&IndicatorKey::Donchian { lookback: 10 }));
        assert!(cache.dataframe().column("dc_10_upper").is_ok());
        assert!(cache.dataframe().column("dc_10_lower").is_ok());
    }

    #[test]
    fn test_indicator_cache_atr_deps() {
        let bars = make_test_bars(30);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        // ATR should automatically compute true range as dependency
        cache.ensure_atr(14).unwrap();
        assert!(cache.has_indicator(&IndicatorKey::TrueRange));
        assert!(cache.has_indicator(&IndicatorKey::ATR { window: 14 }));
        assert!(cache.dataframe().column("true_range").is_ok());
        assert!(cache.dataframe().column("atr_14").is_ok());
    }

    #[test]
    fn test_extract_requirements() {
        use crate::indicators::MAType;
        use crate::strategy_v2::StrategySpec;

        let spec = StrategySpec::MACrossover {
            fast_period: 10,
            slow_period: 50,
            ma_type: MAType::SMA,
        };

        let reqs = extract_indicator_requirements(&spec);
        assert_eq!(reqs.len(), 2);
        assert!(reqs.contains(&IndicatorKey::SMA { window: 10 }));
        assert!(reqs.contains(&IndicatorKey::SMA { window: 50 }));
    }

    #[test]
    fn test_collect_requirements_dedup() {
        use crate::indicators::MAType;
        use crate::strategy_v2::StrategySpec;

        let specs = vec![
            StrategySpec::MACrossover {
                fast_period: 10,
                slow_period: 50,
                ma_type: MAType::SMA,
            },
            StrategySpec::MACrossover {
                fast_period: 10,
                slow_period: 100,
                ma_type: MAType::SMA,
            },
            StrategySpec::MACrossover {
                fast_period: 20,
                slow_period: 50,
                ma_type: MAType::SMA,
            },
        ];

        let reqs = collect_indicator_requirements(&specs);
        // Should have 4 unique SMAs: 10, 50, 100, 20
        assert_eq!(reqs.len(), 4);
    }

    #[test]
    fn test_ensure_all_batched() {
        let bars = make_test_bars(50);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        // Request multiple indicators at once
        let keys = vec![
            IndicatorKey::SMA { window: 10 },
            IndicatorKey::SMA { window: 20 },
            IndicatorKey::EMA { window: 12 },
            IndicatorKey::Donchian { lookback: 20 },
            IndicatorKey::ATR { window: 14 },
            IndicatorKey::Tsmom { lookback: 21 },
        ];

        cache.ensure_all_batched(&keys).unwrap();

        // Verify all indicators were computed
        assert!(cache.has_indicator(&IndicatorKey::SMA { window: 10 }));
        assert!(cache.has_indicator(&IndicatorKey::SMA { window: 20 }));
        assert!(cache.has_indicator(&IndicatorKey::EMA { window: 12 }));
        assert!(cache.has_indicator(&IndicatorKey::Donchian { lookback: 20 }));
        assert!(cache.has_indicator(&IndicatorKey::ATR { window: 14 }));
        assert!(cache.has_indicator(&IndicatorKey::Tsmom { lookback: 21 }));
        // TrueRange should be auto-computed as dependency of ATR
        assert!(cache.has_indicator(&IndicatorKey::TrueRange));

        // Verify columns exist
        assert!(cache.dataframe().column("sma_10").is_ok());
        assert!(cache.dataframe().column("sma_20").is_ok());
        assert!(cache.dataframe().column("ema_12").is_ok());
        assert!(cache.dataframe().column("dc_20_upper").is_ok());
        assert!(cache.dataframe().column("dc_20_lower").is_ok());
        assert!(cache.dataframe().column("atr_14").is_ok());
        assert!(cache.dataframe().column("true_range").is_ok());
        assert!(cache.dataframe().column("close_lag_21").is_ok());

        // Check stats
        let stats = cache.stats();
        assert_eq!(stats.num_indicators_cached, 7); // 6 requested + TrueRange
    }

    #[test]
    fn test_ensure_all_batched_dedup() {
        let bars = make_test_bars(50);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        // Request with duplicates - should deduplicate
        let keys = vec![
            IndicatorKey::SMA { window: 10 },
            IndicatorKey::SMA { window: 10 }, // duplicate
            IndicatorKey::EMA { window: 12 },
            IndicatorKey::Donchian { lookback: 20 },
            IndicatorKey::Donchian { lookback: 20 }, // duplicate
        ];

        cache.ensure_all_batched(&keys).unwrap();

        // Verify unique indicators
        assert!(cache.has_indicator(&IndicatorKey::SMA { window: 10 }));
        assert!(cache.has_indicator(&IndicatorKey::EMA { window: 12 }));
        assert!(cache.has_indicator(&IndicatorKey::Donchian { lookback: 20 }));

        // Check stats - should have 3 unique (not 5)
        let stats = cache.stats();
        assert_eq!(stats.num_indicators_cached, 3);
    }

    #[test]
    fn test_cache_stats() {
        let bars = make_test_bars(30);
        let df = bars_to_dataframe(&bars).unwrap();
        let original_width = df.width();
        let mut cache = IndicatorCache::new(df);

        cache.ensure_sma(10).unwrap();
        cache.ensure_donchian(20).unwrap();

        let stats = cache.stats();
        assert_eq!(stats.num_indicators_cached, 2);
        assert_eq!(stats.num_columns_added, 3); // 1 SMA + 2 Donchian (upper/lower)
        assert_eq!(stats.dataframe_height, 30);
        assert_eq!(stats.dataframe_width, original_width + 3);
    }

    #[test]
    fn test_lazy_indicator_cache_basic() {
        let bars = make_test_bars(50);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        // Build cache with multiple indicators
        let cache = LazyIndicatorCache::new(lf)
            .with_sma(10)
            .with_sma(20)
            .with_ema(12)
            .with_donchian(15);

        assert_eq!(cache.pending_count(), 4);

        // Collect and verify
        let result = cache.collect().unwrap();
        assert!(result.column("sma_10").is_ok());
        assert!(result.column("sma_20").is_ok());
        assert!(result.column("ema_12").is_ok());
        assert!(result.column("dc_15_upper").is_ok());
        assert!(result.column("dc_15_lower").is_ok());
    }

    #[test]
    fn test_lazy_indicator_cache_with_atr() {
        let bars = make_test_bars(50);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        // Build cache with ATR (requires true_range)
        let cache = LazyIndicatorCache::new(lf).with_atr(14).with_atr_wilder(20);

        let result = cache.collect().unwrap();

        // Should have true_range as dependency
        assert!(result.column("true_range").is_ok());
        assert!(result.column("atr_14").is_ok());
        assert!(result.column("atr_wilder_20").is_ok());
    }

    #[test]
    fn test_lazy_indicator_cache_with_all() {
        let bars = make_test_bars(60);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let keys = vec![
            IndicatorKey::SMA { window: 10 },
            IndicatorKey::EMA { window: 21 },
            IndicatorKey::Donchian { lookback: 20 },
            IndicatorKey::Tsmom { lookback: 63 },
            IndicatorKey::RollingMaxHigh { period: 252 },
        ];

        let cache = LazyIndicatorCache::new(lf).with_all(&keys);
        assert_eq!(cache.pending_count(), 5);

        let result = cache.collect().unwrap();
        assert!(result.column("sma_10").is_ok());
        assert!(result.column("ema_21").is_ok());
        assert!(result.column("dc_20_upper").is_ok());
        assert!(result.column("close_lag_63").is_ok());
        assert!(result.column("max_high_252").is_ok());
    }

    #[test]
    fn test_lazy_indicator_cache_build_returns_lazyframe() {
        let bars = make_test_bars(30);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        // Use build() to get LazyFrame without collecting
        let cache = LazyIndicatorCache::new(lf).with_sma(10).with_ema(20);

        // build() returns LazyFrame - we can add more transformations
        let result_lf = cache.build();

        // Now collect and verify
        let result = result_lf.collect().unwrap();
        assert!(result.column("sma_10").is_ok());
        assert!(result.column("ema_20").is_ok());
    }

    #[test]
    fn test_ensure_bollinger() {
        let bars = make_test_bars(50);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        cache.ensure_bollinger(20, 2.0).unwrap();

        assert!(cache.has_indicator(&IndicatorKey::Bollinger {
            period: 20,
            multiplier_x100: 200
        }));
        assert!(cache.dataframe().column("bb_20_middle").is_ok());
        assert!(cache.dataframe().column("bb_20_upper").is_ok());
        assert!(cache.dataframe().column("bb_20_lower").is_ok());
        assert!(cache.dataframe().column("bb_20_bandwidth").is_ok());
    }

    #[test]
    fn test_ensure_keltner() {
        let bars = make_test_bars(50);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        cache.ensure_keltner(20, 14, 1.5).unwrap();

        assert!(cache.has_indicator(&IndicatorKey::Keltner {
            ema_period: 20,
            atr_period: 14,
            multiplier_x100: 150
        }));
        // Keltner requires true_range as dependency
        assert!(cache.has_indicator(&IndicatorKey::TrueRange));
        assert!(cache.dataframe().column("kc_20_14_center").is_ok());
        assert!(cache.dataframe().column("kc_20_14_upper").is_ok());
        assert!(cache.dataframe().column("kc_20_14_lower").is_ok());
    }

    #[test]
    fn test_ensure_starc() {
        let bars = make_test_bars(50);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        cache.ensure_starc(20, 14, 2.0).unwrap();

        assert!(cache.has_indicator(&IndicatorKey::Starc {
            sma_period: 20,
            atr_period: 14,
            multiplier_x100: 200
        }));
        // STARC requires true_range as dependency
        assert!(cache.has_indicator(&IndicatorKey::TrueRange));
        assert!(cache.dataframe().column("starc_20_14_center").is_ok());
        assert!(cache.dataframe().column("starc_20_14_upper").is_ok());
        assert!(cache.dataframe().column("starc_20_14_lower").is_ok());
    }

    #[test]
    fn test_ensure_all_batched_with_channels() {
        let bars = make_test_bars(100);
        let df = bars_to_dataframe(&bars).unwrap();
        let mut cache = IndicatorCache::new(df);

        let keys = vec![
            IndicatorKey::Bollinger {
                period: 20,
                multiplier_x100: 200,
            },
            IndicatorKey::Keltner {
                ema_period: 20,
                atr_period: 14,
                multiplier_x100: 150,
            },
            IndicatorKey::Starc {
                sma_period: 20,
                atr_period: 14,
                multiplier_x100: 200,
            },
        ];

        cache.ensure_all_batched(&keys).unwrap();

        // All should be computed
        for key in &keys {
            assert!(cache.has_indicator(key), "Missing {:?}", key);
        }

        // TrueRange should be auto-computed (shared dependency)
        assert!(cache.has_indicator(&IndicatorKey::TrueRange));
    }

    #[test]
    fn test_lazy_cache_with_channels() {
        let bars = make_test_bars(60);
        let df = bars_to_dataframe(&bars).unwrap();
        let lf = df.lazy();

        let cache = LazyIndicatorCache::new(lf)
            .with_bollinger(20, 2.0)
            .with_keltner(20, 14, 1.5)
            .with_starc(20, 14, 2.0);

        let result = cache.collect().unwrap();

        // Bollinger columns
        assert!(result.column("bb_20_middle").is_ok());
        assert!(result.column("bb_20_upper").is_ok());
        assert!(result.column("bb_20_lower").is_ok());
        assert!(result.column("bb_20_bandwidth").is_ok());

        // Keltner columns
        assert!(result.column("kc_20_14_center").is_ok());
        assert!(result.column("kc_20_14_upper").is_ok());
        assert!(result.column("kc_20_14_lower").is_ok());

        // STARC columns
        assert!(result.column("starc_20_14_center").is_ok());
        assert!(result.column("starc_20_14_upper").is_ok());
        assert!(result.column("starc_20_14_lower").is_ok());

        // True range (dependency)
        assert!(result.column("true_range").is_ok());
    }
}
