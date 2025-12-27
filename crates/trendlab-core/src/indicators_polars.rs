//! Polars expression-based indicators for vectorized computation.
//!
//! These functions return Polars `Expr` types that can be composed in LazyFrame pipelines.
//! They provide the same semantics as the sequential indicators in `indicators.rs`.
//!
//! Key invariant: indicator values at index `t` must depend only on bars `0..=t`.

use polars::prelude::*;

/// Donchian channel as Polars expressions.
///
/// Donchian channel is defined as:
/// - Upper = highest high over the prior N bars (NOT including current bar)
/// - Lower = lowest low over the prior N bars (NOT including current bar)
///
/// Returns `(upper_expr, lower_expr)` that can be added to a LazyFrame.
/// Results are null during warmup period (first `lookback` bars).
pub fn donchian_channel_exprs(lookback: usize) -> (Expr, Expr) {
    // Shift by 1 to exclude current bar, then take rolling max/min
    // The rolling window looks at the lookback bars ending at the shifted position
    let upper = col("high")
        .shift(lit(1)) // Exclude current bar
        .rolling_max(RollingOptionsFixedWindow {
            window_size: lookback,
            min_periods: lookback,
            weights: None,
            center: false,
            fn_params: None,
        })
        .alias("dc_upper");

    let lower = col("low")
        .shift(lit(1)) // Exclude current bar
        .rolling_min(RollingOptionsFixedWindow {
            window_size: lookback,
            min_periods: lookback,
            weights: None,
            center: false,
            fn_params: None,
        })
        .alias("dc_lower");

    (upper, lower)
}

/// Simple moving average of close price as a Polars expression.
///
/// Returns null until there are enough bars to fill the window.
pub fn sma_close_expr(window: usize) -> Expr {
    col("close").rolling_mean(RollingOptionsFixedWindow {
        window_size: window,
        min_periods: window,
        weights: None,
        center: false,
        fn_params: None,
    })
}

/// Simple moving average with custom alias.
pub fn sma_close_expr_aliased(window: usize, alias: &str) -> Expr {
    sma_close_expr(window).alias(alias)
}

/// Exponential moving average of close price as a Polars expression.
///
/// Uses the standard EMA formula:
/// - Multiplier (k) = 2 / (window + 1)
/// - EMA[t] = close[t] * k + EMA[t-1] * (1 - k)
///
/// Alpha = 2/(span+1) where span = window.
pub fn ema_close_expr(window: usize) -> Expr {
    // alpha = 2 / (span + 1)
    let alpha = 2.0 / (window as f64 + 1.0);
    col("close").ewm_mean(EWMOptions {
        alpha,
        adjust: true,
        bias: false,
        min_periods: window,
        ignore_nulls: true,
    })
}

/// Exponential moving average with custom alias.
pub fn ema_close_expr_aliased(window: usize, alias: &str) -> Expr {
    ema_close_expr(window).alias(alias)
}

/// True Range as a Polars expression.
///
/// True Range is defined as the maximum of:
/// - Current high - current low
/// - |current high - previous close|
/// - |current low - previous close|
///
/// For the first bar, only (high - low) is used since there's no previous close.
pub fn true_range_expr() -> Expr {
    let hl = col("high") - col("low");
    let prev_close = col("close").shift(lit(1));
    let hc = (col("high") - prev_close.clone()).abs();
    let lc = (col("low") - prev_close).abs();

    // Compute max(hl, hc, lc) using when/then/otherwise
    // For the first bar, hc and lc are null, so we default to hl
    when(hc.clone().is_null())
        .then(hl.clone()) // First bar: just use high - low
        .otherwise(
            // max(hl, max(hc, lc))
            when(
                hl.clone()
                    .gt_eq(hc.clone())
                    .and(hl.clone().gt_eq(lc.clone())),
            )
            .then(hl.clone())
            .otherwise(when(hc.clone().gt_eq(lc.clone())).then(hc).otherwise(lc)),
        )
        .alias("true_range")
}

/// Average True Range (ATR) using simple moving average.
///
/// ATR is the simple moving average of True Range values.
/// Returns null until there are enough bars to fill the window.
pub fn atr_sma_expr(window: usize) -> Expr {
    // First compute true range, then SMA
    // We need a two-step approach: compute TR column, then rolling mean
    // This is a composite expression that assumes TR column exists
    col("true_range").rolling_mean(RollingOptionsFixedWindow {
        window_size: window,
        min_periods: window,
        weights: None,
        center: false,
        fn_params: None,
    })
}

/// Average True Range using Wilder smoothing (exponential).
///
/// This is the "classic" ATR as originally defined by Welles Wilder:
/// - First ATR = SMA of first `window` TRs
/// - Subsequent: ATR[t] = ATR[t-1] * (window-1)/window + TR[t] / window
///
/// Wilder smoothing is equivalent to EMA with alpha = 1/window.
pub fn atr_wilder_expr(window: usize) -> Expr {
    // Wilder smoothing uses alpha = 1/window
    let alpha = 1.0 / window as f64;
    col("true_range").ewm_mean(EWMOptions {
        alpha,
        adjust: true,
        bias: false,
        min_periods: window,
        ignore_nulls: true,
    })
}

/// Indicator specification for building indicator sets.
#[derive(Debug, Clone)]
pub enum IndicatorSpec {
    /// Donchian channel with specified lookback
    Donchian { lookback: usize },
    /// Simple moving average of close
    SMA { window: usize, alias: String },
    /// Exponential moving average of close
    EMA { window: usize, alias: String },
    /// Average True Range (SMA-based)
    ATR { window: usize },
    /// Average True Range (Wilder smoothing)
    ATRWilder { window: usize },
}

/// Collection of indicators to compute together.
#[derive(Debug, Clone, Default)]
pub struct IndicatorSet {
    pub indicators: Vec<IndicatorSpec>,
}

impl IndicatorSet {
    /// Create a new empty indicator set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a Donchian channel indicator.
    pub fn with_donchian(mut self, lookback: usize) -> Self {
        self.indicators.push(IndicatorSpec::Donchian { lookback });
        self
    }

    /// Add an SMA indicator.
    pub fn with_sma(mut self, window: usize, alias: impl Into<String>) -> Self {
        self.indicators.push(IndicatorSpec::SMA {
            window,
            alias: alias.into(),
        });
        self
    }

    /// Add an EMA indicator.
    pub fn with_ema(mut self, window: usize, alias: impl Into<String>) -> Self {
        self.indicators.push(IndicatorSpec::EMA {
            window,
            alias: alias.into(),
        });
        self
    }

    /// Add ATR with SMA smoothing.
    pub fn with_atr(mut self, window: usize) -> Self {
        self.indicators.push(IndicatorSpec::ATR { window });
        self
    }

    /// Add ATR with Wilder smoothing.
    pub fn with_atr_wilder(mut self, window: usize) -> Self {
        self.indicators.push(IndicatorSpec::ATRWilder { window });
        self
    }
}

/// Apply an indicator set to a LazyFrame.
///
/// The input LazyFrame must have columns: open, high, low, close, volume
/// (standard OHLCV schema).
///
/// Returns a new LazyFrame with indicator columns added.
pub fn apply_indicators(lf: LazyFrame, indicator_set: &IndicatorSet) -> LazyFrame {
    let mut lf = lf;
    let mut needs_true_range = false;

    // Check if we need true_range column
    for ind in &indicator_set.indicators {
        match ind {
            IndicatorSpec::ATR { .. } | IndicatorSpec::ATRWilder { .. } => {
                needs_true_range = true;
                break;
            }
            _ => {}
        }
    }

    // Add true_range if needed by ATR indicators
    if needs_true_range {
        lf = lf.with_column(true_range_expr());
    }

    // Apply each indicator
    for ind in &indicator_set.indicators {
        lf = match ind {
            IndicatorSpec::Donchian { lookback } => {
                let (upper, lower) = donchian_channel_exprs(*lookback);
                lf.with_columns([upper, lower])
            }
            IndicatorSpec::SMA { window, alias } => {
                lf.with_column(sma_close_expr_aliased(*window, alias))
            }
            IndicatorSpec::EMA { window, alias } => {
                lf.with_column(ema_close_expr_aliased(*window, alias))
            }
            IndicatorSpec::ATR { window } => lf.with_column(atr_sma_expr(*window).alias("atr")),
            IndicatorSpec::ATRWilder { window } => {
                lf.with_column(atr_wilder_expr(*window).alias("atr_wilder"))
            }
        };
    }

    lf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::Bar;
    use crate::data::bars_to_dataframe;
    use chrono::{TimeZone, Utc};

    fn bars_from_ohlc(ohlc: &[(f64, f64, f64, f64)]) -> Vec<Bar> {
        ohlc.iter()
            .enumerate()
            .map(|(i, &(o, h, l, c))| {
                let ts = Utc
                    .with_ymd_and_hms(2024, 1, 1 + i as u32, 0, 0, 0)
                    .unwrap();
                Bar::new(ts, o, h, l, c, 1000.0, "TEST", "1d")
            })
            .collect()
    }

    fn bars_from_closes(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let ts = Utc
                    .with_ymd_and_hms(2024, 1, 1 + i as u32, 0, 0, 0)
                    .unwrap();
                Bar::new(ts, c, c, c, c, 1000.0, "TEST", "1d")
            })
            .collect()
    }

    #[test]
    fn test_donchian_polars_matches_sequential() {
        let ohlc = vec![
            (100.0, 102.0, 98.0, 101.0),
            (101.0, 103.0, 99.0, 102.0),
            (102.0, 104.0, 100.0, 103.0),
            (103.0, 105.0, 101.0, 104.0),
            (104.0, 106.0, 102.0, 105.0),
            (105.0, 107.0, 103.0, 104.0),
        ];
        let bars = bars_from_ohlc(&ohlc);

        // Sequential implementation
        let seq_dc = crate::indicators::donchian_channel(&bars, 5);

        // Polars implementation
        let df = bars_to_dataframe(&bars).unwrap();
        let (upper_expr, lower_expr) = donchian_channel_exprs(5);
        let result = df
            .lazy()
            .with_columns([upper_expr, lower_expr])
            .collect()
            .unwrap();

        let pol_upper = result.column("dc_upper").unwrap().f64().unwrap();
        let pol_lower = result.column("dc_lower").unwrap().f64().unwrap();

        // Compare results
        for (i, seq_val) in seq_dc.iter().enumerate() {
            match seq_val {
                None => {
                    assert!(
                        pol_upper.get(i).is_none(),
                        "Expected null upper at index {}",
                        i
                    );
                    assert!(
                        pol_lower.get(i).is_none(),
                        "Expected null lower at index {}",
                        i
                    );
                }
                Some(ch) => {
                    let pu = pol_upper.get(i).unwrap();
                    let pl = pol_lower.get(i).unwrap();
                    assert!(
                        (pu - ch.upper).abs() < 1e-10,
                        "Upper mismatch at {}: {} vs {}",
                        i,
                        pu,
                        ch.upper
                    );
                    assert!(
                        (pl - ch.lower).abs() < 1e-10,
                        "Lower mismatch at {}: {} vs {}",
                        i,
                        pl,
                        ch.lower
                    );
                }
            }
        }
    }

    #[test]
    fn test_sma_polars_matches_sequential() {
        let bars = bars_from_closes(&[1.0, 2.0, 3.0, 4.0, 5.0]);

        // Sequential
        let seq_sma = crate::indicators::sma_close(&bars, 3);

        // Polars
        let df = bars_to_dataframe(&bars).unwrap();
        let result = df
            .lazy()
            .with_column(sma_close_expr(3).alias("sma"))
            .collect()
            .unwrap();

        let pol_sma = result.column("sma").unwrap().f64().unwrap();

        for (i, seq_val) in seq_sma.iter().enumerate() {
            match seq_val {
                None => {
                    assert!(pol_sma.get(i).is_none(), "Expected null at index {}", i);
                }
                Some(v) => {
                    let pv = pol_sma.get(i).unwrap();
                    assert!(
                        (pv - v).abs() < 1e-10,
                        "SMA mismatch at {}: {} vs {}",
                        i,
                        pv,
                        v
                    );
                }
            }
        }
    }

    #[test]
    fn test_true_range_polars() {
        let ohlc = vec![
            (100.0, 105.0, 95.0, 102.0),  // TR = 10
            (102.0, 108.0, 100.0, 106.0), // TR = max(8, 6, 2) = 8
            (106.0, 112.0, 104.0, 110.0), // TR = max(8, 6, 2) = 8
        ];
        let bars = bars_from_ohlc(&ohlc);

        // Sequential
        let seq_tr = crate::indicators::true_range(&bars);

        // Polars
        let df = bars_to_dataframe(&bars).unwrap();
        let result = df.lazy().with_column(true_range_expr()).collect().unwrap();

        let pol_tr = result.column("true_range").unwrap().f64().unwrap();

        for (i, &seq_val) in seq_tr.iter().enumerate() {
            let pv = pol_tr.get(i).unwrap();
            assert!(
                (pv - seq_val).abs() < 1e-10,
                "TR mismatch at {}: {} vs {}",
                i,
                pv,
                seq_val
            );
        }
    }

    #[test]
    fn test_indicator_set_builder() {
        let set = IndicatorSet::new()
            .with_donchian(20)
            .with_sma(50, "sma_50")
            .with_ema(20, "ema_20")
            .with_atr_wilder(14);

        assert_eq!(set.indicators.len(), 4);
    }

    #[test]
    fn test_apply_indicators() {
        let ohlc = vec![
            (100.0, 105.0, 95.0, 102.0),
            (102.0, 108.0, 100.0, 106.0),
            (106.0, 112.0, 104.0, 110.0),
            (110.0, 115.0, 108.0, 113.0),
            (113.0, 118.0, 111.0, 116.0),
        ];
        let bars = bars_from_ohlc(&ohlc);
        let df = bars_to_dataframe(&bars).unwrap();

        let set = IndicatorSet::new()
            .with_donchian(3)
            .with_sma(3, "sma_3")
            .with_atr_wilder(3);

        let result = apply_indicators(df.lazy(), &set).collect().unwrap();

        // Check that all expected columns exist
        assert!(result.column("dc_upper").is_ok());
        assert!(result.column("dc_lower").is_ok());
        assert!(result.column("sma_3").is_ok());
        assert!(result.column("true_range").is_ok());
        assert!(result.column("atr_wilder").is_ok());
    }
}
