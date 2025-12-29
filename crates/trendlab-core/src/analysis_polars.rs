//! Polars-based statistical analysis computations.
//!
//! Implements vectorized computation for:
//! - Return distribution metrics (VaR, CVaR, skewness, kurtosis)
//! - Regime-based performance analysis
//! - Trade-level statistics (MAE, MFE, holding period)

use crate::analysis::{
    AnalysisConfig, DrawdownRegime, DrawdownRegimeAnalysis, DrawdownThresholds, EdgeRatioStats,
    ExcursionStats, HoldingBucket, HoldingPeriodStats, RegimeAnalysis, RegimeConcentrationScore,
    RegimeMetrics, ReturnDistribution, StatisticalAnalysis, TradeAnalysis, TradeExcursion,
    TrendRegime, TrendRegimeAnalysis, VolAtEntryStats, VolRegime,
};
use crate::backtest::{BacktestResult, EquityPoint, Trade};
use crate::bar::Bar;
use chrono::Utc;
use polars::prelude::*;

/// Compute complete statistical analysis for a backtest result.
pub fn compute_analysis(
    result: &BacktestResult,
    bars: &[Bar],
    config: &AnalysisConfig,
) -> Result<StatisticalAnalysis, PolarsError> {
    let return_distribution = compute_return_distribution(&result.equity)?;
    let regime_analysis = compute_regime_analysis(bars, &result.equity, &result.trades, config)?;
    let trade_analysis = compute_trade_analysis(&result.trades, bars, config)?;

    Ok(StatisticalAnalysis {
        return_distribution,
        regime_analysis,
        trade_analysis,
        computed_at: Utc::now(),
        config: config.clone(),
    })
}

// =============================================================================
// RETURN DISTRIBUTION
// =============================================================================

/// Compute return distribution statistics from equity curve.
pub fn compute_return_distribution(
    equity: &[EquityPoint],
) -> Result<ReturnDistribution, PolarsError> {
    if equity.len() < 2 {
        return Ok(ReturnDistribution::default());
    }

    // Extract equity values and compute daily returns
    let equities: Vec<f64> = equity.iter().map(|e| e.equity).collect();

    let df = DataFrame::new(vec![Column::new("equity".into(), equities)])?;

    // Compute daily returns
    let returns_df = df
        .lazy()
        .with_column((col("equity") / col("equity").shift(lit(1)) - lit(1.0)).alias("return"))
        .collect()?;

    // Extract returns, filtering nulls
    let returns_col = returns_df.column("return")?.f64()?;
    let returns: Vec<f64> = returns_col.into_iter().flatten().collect();

    if returns.is_empty() {
        return Ok(ReturnDistribution::default());
    }

    // Basic statistics
    let n = returns.len();
    let mean = returns.iter().sum::<f64>() / n as f64;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n as f64;
    let std = variance.sqrt();
    let min = returns.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = returns.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // Sort for quantile calculations
    let mut sorted = returns.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // VaR calculations (5th and 1st percentile - loss side)
    let var_95 = percentile(&sorted, 0.05).abs();
    let var_99 = percentile(&sorted, 0.01).abs();

    // CVaR (Expected Shortfall) - average of returns below VaR threshold
    let cvar_95 = compute_cvar(&sorted, 0.05);
    let cvar_99 = compute_cvar(&sorted, 0.01);

    // Skewness and kurtosis
    let skewness = compute_skewness(&returns, mean, std);
    let kurtosis = compute_excess_kurtosis(&returns, mean, std);

    Ok(ReturnDistribution {
        var_95,
        var_99,
        cvar_95,
        cvar_99,
        skewness,
        kurtosis,
        mean_return: mean,
        std_return: std,
        min_return: min,
        max_return: max,
        n_observations: n,
    })
}

/// Compute percentile value from sorted data.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Compute Conditional VaR (Expected Shortfall).
fn compute_cvar(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let threshold_idx = ((p * sorted.len() as f64).ceil() as usize).max(1);
    let tail: Vec<f64> = sorted.iter().take(threshold_idx).cloned().collect();
    if tail.is_empty() {
        return 0.0;
    }
    (tail.iter().sum::<f64>() / tail.len() as f64).abs()
}

/// Compute skewness of a distribution.
fn compute_skewness(data: &[f64], mean: f64, std: f64) -> f64 {
    if std == 0.0 || data.len() < 3 {
        return 0.0;
    }
    let n = data.len() as f64;
    let m3 = data.iter().map(|x| ((x - mean) / std).powi(3)).sum::<f64>() / n;
    m3
}

/// Compute excess kurtosis of a distribution.
fn compute_excess_kurtosis(data: &[f64], mean: f64, std: f64) -> f64 {
    if std == 0.0 || data.len() < 4 {
        return 0.0;
    }
    let n = data.len() as f64;
    let m4 = data.iter().map(|x| ((x - mean) / std).powi(4)).sum::<f64>() / n;
    m4 - 3.0 // Excess kurtosis (subtract 3 for normal distribution baseline)
}

// =============================================================================
// REGIME ANALYSIS
// =============================================================================

/// Compute regime-based performance analysis.
pub fn compute_regime_analysis(
    bars: &[Bar],
    equity: &[EquityPoint],
    trades: &[Trade],
    config: &AnalysisConfig,
) -> Result<RegimeAnalysis, PolarsError> {
    if bars.len() < config.atr_period + 1 {
        return Ok(RegimeAnalysis::default());
    }

    // Build bars DataFrame with ATR
    let timestamps: Vec<i64> = bars.iter().map(|b| b.ts.timestamp()).collect();
    let highs: Vec<f64> = bars.iter().map(|b| b.high).collect();
    let lows: Vec<f64> = bars.iter().map(|b| b.low).collect();
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();

    let df = DataFrame::new(vec![
        Column::new("ts".into(), timestamps),
        Column::new("high".into(), highs),
        Column::new("low".into(), lows),
        Column::new("close".into(), closes),
    ])?;

    // Compute True Range and ATR
    let with_atr = df
        .lazy()
        .with_column(crate::indicators_polars::true_range_expr().alias("tr"))
        .with_column(
            col("tr")
                .rolling_mean(RollingOptionsFixedWindow {
                    window_size: config.atr_period,
                    min_periods: config.atr_period,
                    weights: None,
                    center: false,
                    fn_params: None,
                })
                .alias("atr"),
        )
        .collect()?;

    // Calculate median ATR for regime thresholds
    let atr_col = with_atr.column("atr")?.f64()?;
    let atr_values: Vec<f64> = atr_col.into_iter().flatten().collect();

    if atr_values.is_empty() {
        return Ok(RegimeAnalysis::default());
    }

    let mut sorted_atr = atr_values.clone();
    sorted_atr.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_atr = percentile(&sorted_atr, 0.5);

    let high_threshold = median_atr * config.high_vol_threshold;
    let low_threshold = median_atr * config.low_vol_threshold;

    // Classify each bar into regime
    let ts_col = with_atr.column("ts")?.i64()?;
    let mut regimes: Vec<VolRegime> = Vec::with_capacity(bars.len());

    for i in 0..bars.len() {
        let atr = atr_col.get(i);
        let regime = match atr {
            Some(a) if a > high_threshold => VolRegime::High,
            Some(a) if a < low_threshold => VolRegime::Low,
            Some(_) => VolRegime::Neutral,
            None => VolRegime::Neutral, // Default for warmup period
        };
        regimes.push(regime);
    }

    // Build timestamp -> regime lookup
    let ts_to_regime: std::collections::HashMap<i64, VolRegime> = ts_col
        .into_iter()
        .enumerate()
        .filter_map(|(i, ts)| ts.map(|t| (t, regimes[i])))
        .collect();

    // Count days per regime
    let (high_days, neutral_days, low_days) = regimes.iter().fold((0, 0, 0), |acc, r| match r {
        VolRegime::High => (acc.0 + 1, acc.1, acc.2),
        VolRegime::Neutral => (acc.0, acc.1 + 1, acc.2),
        VolRegime::Low => (acc.0, acc.1, acc.2 + 1),
    });
    let total_days = regimes.len();

    // Classify trades by entry regime and compute per-regime metrics
    let mut high_trades: Vec<&Trade> = Vec::new();
    let mut neutral_trades: Vec<&Trade> = Vec::new();
    let mut low_trades: Vec<&Trade> = Vec::new();

    for trade in trades {
        let entry_ts = trade.entry.ts.timestamp();
        match ts_to_regime.get(&entry_ts) {
            Some(VolRegime::High) => high_trades.push(trade),
            Some(VolRegime::Neutral) => neutral_trades.push(trade),
            Some(VolRegime::Low) => low_trades.push(trade),
            None => neutral_trades.push(trade), // Default if not found
        }
    }

    // Compute equity returns per regime for Sharpe calculation
    let equity_returns = compute_equity_returns(equity);
    let (high_returns, neutral_returns, low_returns) =
        split_returns_by_regime(&equity_returns, equity, &ts_to_regime);

    Ok(RegimeAnalysis {
        high_vol: compute_regime_metrics(&high_trades, high_days, total_days, &high_returns),
        neutral_vol: compute_regime_metrics(
            &neutral_trades,
            neutral_days,
            total_days,
            &neutral_returns,
        ),
        low_vol: compute_regime_metrics(&low_trades, low_days, total_days, &low_returns),
        median_atr,
        atr_period: config.atr_period,
    })
}

/// Compute equity returns from equity points.
fn compute_equity_returns(equity: &[EquityPoint]) -> Vec<(i64, f64)> {
    equity
        .windows(2)
        .map(|w| {
            (
                w[1].ts.timestamp(),
                (w[1].equity - w[0].equity) / w[0].equity,
            )
        })
        .collect()
}

/// Split returns by regime.
fn split_returns_by_regime(
    returns: &[(i64, f64)],
    _equity: &[EquityPoint],
    ts_to_regime: &std::collections::HashMap<i64, VolRegime>,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut high = Vec::new();
    let mut neutral = Vec::new();
    let mut low = Vec::new();

    for (ts, ret) in returns {
        match ts_to_regime.get(ts) {
            Some(VolRegime::High) => high.push(*ret),
            Some(VolRegime::Neutral) => neutral.push(*ret),
            Some(VolRegime::Low) => low.push(*ret),
            None => neutral.push(*ret),
        }
    }

    (high, neutral, low)
}

/// Compute metrics for a specific regime.
fn compute_regime_metrics(
    trades: &[&Trade],
    n_days: usize,
    total_days: usize,
    returns: &[f64],
) -> RegimeMetrics {
    let n_trades = trades.len();
    let winners = trades.iter().filter(|t| t.net_pnl > 0.0).count();
    let win_rate = if n_trades > 0 {
        winners as f64 / n_trades as f64
    } else {
        0.0
    };

    let total_pnl: f64 = trades.iter().map(|t| t.net_pnl).sum();
    let avg_return = if n_trades > 0 {
        total_pnl / n_trades as f64
    } else {
        0.0
    };

    // Compute Sharpe for this regime
    let sharpe = if returns.len() > 1 {
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        let std = variance.sqrt();
        if std > 0.0 {
            (mean * 252.0) / (std * 252.0_f64.sqrt())
        } else {
            0.0
        }
    } else {
        0.0
    };

    // Total return during regime
    let total_return = returns.iter().map(|r| r + 1.0).product::<f64>() - 1.0;

    RegimeMetrics {
        n_days,
        pct_days: if total_days > 0 {
            n_days as f64 / total_days as f64
        } else {
            0.0
        },
        n_trades_entered: n_trades,
        win_rate,
        avg_trade_return: avg_return,
        total_return,
        sharpe,
    }
}

// =============================================================================
// TREND REGIME ANALYSIS
// =============================================================================

/// Configuration for trend regime classification.
#[derive(Debug, Clone)]
pub struct TrendRegimeConfig {
    /// MA period for slope calculation.
    pub ma_period: usize,
    /// Strong trend threshold (slope as % per period, e.g., 0.02 = 2%).
    pub strong_threshold: f64,
    /// Weak trend threshold (slope as % per period, e.g., 0.005 = 0.5%).
    pub weak_threshold: f64,
}

impl Default for TrendRegimeConfig {
    fn default() -> Self {
        Self {
            ma_period: 50,
            strong_threshold: 0.02,
            weak_threshold: 0.005,
        }
    }
}

/// Compute trend regime analysis based on MA slope.
///
/// Classifies each bar into a trend regime based on the slope of the moving average:
/// - StrongUp: slope > strong_threshold
/// - WeakUp: weak_threshold < slope <= strong_threshold
/// - Neutral: -weak_threshold <= slope <= weak_threshold
/// - WeakDown: -strong_threshold <= slope < -weak_threshold
/// - StrongDown: slope < -strong_threshold
pub fn compute_trend_regime_analysis(
    bars: &[Bar],
    equity: &[EquityPoint],
    trades: &[Trade],
    config: &TrendRegimeConfig,
) -> Result<TrendRegimeAnalysis, PolarsError> {
    if bars.len() < config.ma_period + 1 {
        return Ok(TrendRegimeAnalysis::default());
    }

    // Build bars DataFrame with MA and slope
    let timestamps: Vec<i64> = bars.iter().map(|b| b.ts.timestamp()).collect();
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();

    let df = DataFrame::new(vec![
        Column::new("ts".into(), timestamps),
        Column::new("close".into(), closes),
    ])?;

    // Compute MA and slope
    let with_trend = df
        .lazy()
        .with_column(
            col("close")
                .rolling_mean(RollingOptionsFixedWindow {
                    window_size: config.ma_period,
                    min_periods: config.ma_period,
                    weights: None,
                    center: false,
                    fn_params: None,
                })
                .alias("ma"),
        )
        .with_column(
            // Slope as % change in MA over last period
            ((col("ma") - col("ma").shift(lit(1))) / col("ma").shift(lit(1))).alias("ma_slope"),
        )
        .collect()?;

    // Classify each bar into trend regime
    let slope_col = with_trend.column("ma_slope")?.f64()?;
    let ts_col = with_trend.column("ts")?.i64()?;

    let mut regimes: Vec<TrendRegime> = Vec::with_capacity(bars.len());
    for i in 0..bars.len() {
        let regime = match slope_col.get(i) {
            Some(s) if s > config.strong_threshold => TrendRegime::StrongUp,
            Some(s) if s > config.weak_threshold => TrendRegime::WeakUp,
            Some(s) if s < -config.strong_threshold => TrendRegime::StrongDown,
            Some(s) if s < -config.weak_threshold => TrendRegime::WeakDown,
            Some(_) => TrendRegime::Neutral,
            None => TrendRegime::Neutral, // Default for warmup period
        };
        regimes.push(regime);
    }

    // Build timestamp -> regime lookup
    let ts_to_regime: std::collections::HashMap<i64, TrendRegime> = ts_col
        .into_iter()
        .enumerate()
        .filter_map(|(i, ts)| ts.map(|t| (t, regimes[i])))
        .collect();

    // Count days per regime
    let mut regime_days: std::collections::HashMap<TrendRegime, usize> =
        std::collections::HashMap::new();
    for regime in &regimes {
        *regime_days.entry(*regime).or_insert(0) += 1;
    }
    let total_days = regimes.len();

    // Classify trades by entry regime
    let mut regime_trades: std::collections::HashMap<TrendRegime, Vec<&Trade>> =
        std::collections::HashMap::new();
    for trade in trades {
        let entry_ts = trade.entry.ts.timestamp();
        let regime = ts_to_regime
            .get(&entry_ts)
            .copied()
            .unwrap_or(TrendRegime::Neutral);
        regime_trades.entry(regime).or_default().push(trade);
    }

    // Compute equity returns per regime
    let equity_returns = compute_equity_returns(equity);
    let regime_returns = split_returns_by_trend_regime(&equity_returns, &ts_to_regime);

    // Build results
    let mut by_regime = std::collections::HashMap::new();
    for regime in TrendRegime::all() {
        let trades = regime_trades
            .get(regime)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let n_days = *regime_days.get(regime).unwrap_or(&0);
        let returns = regime_returns
            .get(regime)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        by_regime.insert(
            *regime,
            compute_regime_metrics(trades, n_days, total_days, returns),
        );
    }

    Ok(TrendRegimeAnalysis {
        by_regime,
        ma_period: config.ma_period,
        strong_threshold: config.strong_threshold,
        weak_threshold: config.weak_threshold,
    })
}

/// Split returns by trend regime.
fn split_returns_by_trend_regime(
    returns: &[(i64, f64)],
    ts_to_regime: &std::collections::HashMap<i64, TrendRegime>,
) -> std::collections::HashMap<TrendRegime, Vec<f64>> {
    let mut by_regime: std::collections::HashMap<TrendRegime, Vec<f64>> =
        std::collections::HashMap::new();

    for (ts, ret) in returns {
        let regime = ts_to_regime
            .get(ts)
            .copied()
            .unwrap_or(TrendRegime::Neutral);
        by_regime.entry(regime).or_default().push(*ret);
    }

    by_regime
}

// =============================================================================
// DRAWDOWN REGIME ANALYSIS
// =============================================================================

/// Compute drawdown regime analysis based on current drawdown from peak.
///
/// Classifies each bar into a drawdown regime based on how far the strategy
/// equity is from its high-water mark:
/// - Normal: drawdown < 5%
/// - Shallow: 5% <= drawdown < 10%
/// - Deep: 10% <= drawdown < 20%
/// - Recovery: drawdown >= 20%
pub fn compute_drawdown_regime_analysis(
    equity: &[EquityPoint],
    trades: &[Trade],
    thresholds: &DrawdownThresholds,
) -> Result<DrawdownRegimeAnalysis, PolarsError> {
    if equity.len() < 2 {
        return Ok(DrawdownRegimeAnalysis::default());
    }

    // Compute running drawdown for each equity point
    let mut peak = equity[0].equity;
    let mut regimes: Vec<DrawdownRegime> = Vec::with_capacity(equity.len());
    let mut ts_to_regime: std::collections::HashMap<i64, DrawdownRegime> =
        std::collections::HashMap::new();

    for ep in equity {
        peak = peak.max(ep.equity);
        let dd_pct = if peak > 0.0 {
            (peak - ep.equity) / peak
        } else {
            0.0
        };

        let regime = classify_drawdown_regime(dd_pct, thresholds);
        regimes.push(regime);
        ts_to_regime.insert(ep.ts.timestamp(), regime);
    }

    // Count days per regime
    let mut regime_days: std::collections::HashMap<DrawdownRegime, usize> =
        std::collections::HashMap::new();
    for regime in &regimes {
        *regime_days.entry(*regime).or_insert(0) += 1;
    }
    let total_days = regimes.len();

    // Classify trades by entry regime
    let mut regime_trades: std::collections::HashMap<DrawdownRegime, Vec<&Trade>> =
        std::collections::HashMap::new();
    for trade in trades {
        let entry_ts = trade.entry.ts.timestamp();
        let regime = ts_to_regime
            .get(&entry_ts)
            .copied()
            .unwrap_or(DrawdownRegime::Normal);
        regime_trades.entry(regime).or_default().push(trade);
    }

    // Compute equity returns per regime
    let equity_returns = compute_equity_returns(equity);
    let regime_returns = split_returns_by_drawdown_regime(&equity_returns, &ts_to_regime);

    // Build results
    let mut by_regime = std::collections::HashMap::new();
    for regime in DrawdownRegime::all() {
        let trades = regime_trades
            .get(regime)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let n_days = *regime_days.get(regime).unwrap_or(&0);
        let returns = regime_returns
            .get(regime)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        by_regime.insert(
            *regime,
            compute_regime_metrics(trades, n_days, total_days, returns),
        );
    }

    Ok(DrawdownRegimeAnalysis {
        by_regime,
        thresholds: thresholds.clone(),
    })
}

/// Classify drawdown percentage into regime.
fn classify_drawdown_regime(dd_pct: f64, thresholds: &DrawdownThresholds) -> DrawdownRegime {
    if dd_pct < thresholds.shallow {
        DrawdownRegime::Normal
    } else if dd_pct < thresholds.deep {
        DrawdownRegime::Shallow
    } else if dd_pct < thresholds.recovery {
        DrawdownRegime::Deep
    } else {
        DrawdownRegime::Recovery
    }
}

/// Split returns by drawdown regime.
fn split_returns_by_drawdown_regime(
    returns: &[(i64, f64)],
    ts_to_regime: &std::collections::HashMap<i64, DrawdownRegime>,
) -> std::collections::HashMap<DrawdownRegime, Vec<f64>> {
    let mut by_regime: std::collections::HashMap<DrawdownRegime, Vec<f64>> =
        std::collections::HashMap::new();

    for (ts, ret) in returns {
        let regime = ts_to_regime
            .get(ts)
            .copied()
            .unwrap_or(DrawdownRegime::Normal);
        by_regime.entry(regime).or_default().push(*ret);
    }

    by_regime
}

// =============================================================================
// REGIME CONCENTRATION SCORING
// =============================================================================

/// Compute regime concentration score for a strategy.
///
/// Measures how concentrated returns are in specific regimes.
/// A perfectly balanced strategy would have equal returns across all regimes.
/// A concentrated strategy has most returns from a single regime.
///
/// The concentration score uses a Herfindahl-Hirschman-like index:
/// - Compute % of total return from each regime
/// - Square the percentages and sum
/// - Normalize to [0, 1] range
///
/// Returns a score where 0 = perfectly balanced, 1 = all returns from one regime.
pub fn compute_regime_concentration_score(
    vol_analysis: &RegimeAnalysis,
    trend_analysis: Option<&TrendRegimeAnalysis>,
    drawdown_analysis: Option<&DrawdownRegimeAnalysis>,
) -> RegimeConcentrationScore {
    // Compute volatility regime concentration
    let vol_returns = [
        vol_analysis.high_vol.total_return,
        vol_analysis.neutral_vol.total_return,
        vol_analysis.low_vol.total_return,
    ];
    let (vol_concentration, dominant_vol_idx, vol_pct) = compute_hhi_concentration(&vol_returns);
    let dominant_vol_regime = match dominant_vol_idx {
        0 => Some(VolRegime::High),
        1 => Some(VolRegime::Neutral),
        2 => Some(VolRegime::Low),
        _ => None,
    };

    // Compute trend regime concentration (if available)
    let (trend_concentration, dominant_trend_regime, trend_pct) =
        if let Some(trend) = trend_analysis {
            let returns: Vec<f64> = TrendRegime::all()
                .iter()
                .map(|r| {
                    trend
                        .by_regime
                        .get(r)
                        .map(|m| m.total_return)
                        .unwrap_or(0.0)
                })
                .collect();
            let (conc, idx, pct) = compute_hhi_concentration(&returns);
            let regime = TrendRegime::all().get(idx).copied();
            (conc, regime, pct)
        } else {
            (0.0, None, 0.0)
        };

    // Compute drawdown regime concentration (if available)
    let (drawdown_concentration, dominant_drawdown_regime, drawdown_pct) =
        if let Some(dd) = drawdown_analysis {
            let returns: Vec<f64> = DrawdownRegime::all()
                .iter()
                .map(|r| dd.by_regime.get(r).map(|m| m.total_return).unwrap_or(0.0))
                .collect();
            let (conc, idx, pct) = compute_hhi_concentration(&returns);
            let regime = DrawdownRegime::all().get(idx).copied();
            (conc, regime, pct)
        } else {
            (0.0, None, 0.0)
        };

    // Compute combined score (weighted average)
    // Weight: vol = 0.4, trend = 0.4, drawdown = 0.2
    let has_trend = trend_analysis.is_some();
    let has_drawdown = drawdown_analysis.is_some();

    let combined_score = match (has_trend, has_drawdown) {
        (true, true) => {
            0.4 * vol_concentration + 0.4 * trend_concentration + 0.2 * drawdown_concentration
        }
        (true, false) => 0.5 * vol_concentration + 0.5 * trend_concentration,
        (false, true) => 0.7 * vol_concentration + 0.3 * drawdown_concentration,
        (false, false) => vol_concentration,
    };

    RegimeConcentrationScore {
        vol_concentration,
        trend_concentration,
        drawdown_concentration,
        combined_score,
        dominant_vol_regime,
        dominant_trend_regime,
        dominant_drawdown_regime,
        vol_regime_pct: vol_pct,
        trend_regime_pct: trend_pct,
        drawdown_regime_pct: drawdown_pct,
    }
}

/// Compute Herfindahl-Hirschman-like concentration index.
///
/// Returns: (concentration score 0-1, index of dominant regime, % from dominant regime)
fn compute_hhi_concentration(returns: &[f64]) -> (f64, usize, f64) {
    if returns.is_empty() {
        return (0.0, 0, 0.0);
    }

    // Handle case where total return is zero or negative
    let total_positive: f64 = returns.iter().filter(|&&r| r > 0.0).sum();
    let total_absolute: f64 = returns.iter().map(|r| r.abs()).sum();

    if total_absolute < 1e-10 {
        return (0.0, 0, 0.0);
    }

    // Compute share of each regime (using absolute values to handle negative returns)
    let shares: Vec<f64> = returns.iter().map(|r| r.abs() / total_absolute).collect();

    // HHI = sum of squared shares
    // For n equally-sized shares, HHI = 1/n
    // For one dominant share, HHI = 1.0
    let hhi: f64 = shares.iter().map(|s| s * s).sum();

    // Normalize to [0, 1]: subtract 1/n (minimum HHI) and divide by (1 - 1/n)
    let n = returns.len() as f64;
    let min_hhi = 1.0 / n;
    let normalized = if n > 1.0 {
        (hhi - min_hhi) / (1.0 - min_hhi)
    } else {
        hhi
    };

    // Find dominant regime
    let (dominant_idx, dominant_share) = shares
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, &s)| (i, s))
        .unwrap_or((0, 0.0));

    // Compute % of positive returns from dominant regime
    let dominant_pct = if total_positive > 0.0 && returns[dominant_idx] > 0.0 {
        returns[dominant_idx] / total_positive
    } else {
        dominant_share
    };

    (normalized.clamp(0.0, 1.0), dominant_idx, dominant_pct)
}

// =============================================================================
// TRADE ANALYSIS
// =============================================================================

/// Compute trade-level analysis.
pub fn compute_trade_analysis(
    trades: &[Trade],
    bars: &[Bar],
    config: &AnalysisConfig,
) -> Result<TradeAnalysis, PolarsError> {
    if trades.is_empty() {
        return Ok(TradeAnalysis::default());
    }

    // Build timestamp -> bar lookup for MAE/MFE calculation
    let _ts_to_bar: std::collections::HashMap<i64, &Bar> =
        bars.iter().map(|b| (b.ts.timestamp(), b)).collect();

    // Build timestamp -> index lookup for range queries
    let ts_to_idx: std::collections::HashMap<i64, usize> = bars
        .iter()
        .enumerate()
        .map(|(i, b)| (b.ts.timestamp(), i))
        .collect();

    // Compute ATR for vol-at-entry
    let atr_values = compute_atr_series(bars, config.atr_period);

    // Compute excursions for each trade
    let mut excursions: Vec<TradeExcursion> = Vec::with_capacity(trades.len());

    for trade in trades {
        let entry_ts = trade.entry.ts.timestamp();
        let exit_ts = trade.exit.ts.timestamp();
        let entry_price = trade.entry.price;

        // Find bars during trade window
        let entry_idx = ts_to_idx.get(&entry_ts).copied();
        let exit_idx = ts_to_idx.get(&exit_ts).copied();

        if let (Some(start), Some(end)) = (entry_idx, exit_idx) {
            // Calculate MAE and MFE during trade
            let mut max_adverse = 0.0_f64;
            let mut max_favorable = 0.0_f64;

            #[allow(clippy::needless_range_loop)]
            for i in start..=end.min(bars.len() - 1) {
                let bar = &bars[i];
                // For long trades: adverse = how much price dropped below entry
                let adverse = (entry_price - bar.low) / entry_price;
                // Favorable = how much price rose above entry
                let favorable = (bar.high - entry_price) / entry_price;

                max_adverse = max_adverse.max(adverse.max(0.0));
                max_favorable = max_favorable.max(favorable.max(0.0));
            }

            // Get ATR at entry
            let entry_atr_pct = if start < atr_values.len() {
                atr_values[start] / entry_price
            } else {
                0.0
            };

            // Holding period in bars (approximate trading days)
            let holding_days = end.saturating_sub(start) + 1;

            // Trade return
            let return_pct = trade.net_pnl / (entry_price * trade.entry.qty);

            excursions.push(TradeExcursion {
                mae_pct: max_adverse,
                mfe_pct: max_favorable,
                holding_days,
                entry_atr_pct,
                return_pct,
                is_winner: trade.net_pnl > 0.0,
            });
        }
    }

    if excursions.is_empty() {
        return Ok(TradeAnalysis::default());
    }

    // Compute statistics from excursions
    let holding_period = compute_holding_stats(&excursions, config);
    let mae = compute_excursion_stats(&excursions, |e| e.mae_pct);
    let mfe = compute_excursion_stats(&excursions, |e| e.mfe_pct);
    let edge_ratio = compute_edge_ratio_stats(&excursions);
    let vol_at_entry = compute_vol_at_entry_stats(&excursions);

    Ok(TradeAnalysis {
        holding_period,
        mae,
        mfe,
        edge_ratio,
        vol_at_entry,
        n_trades: excursions.len(),
    })
}

/// Compute ATR series for all bars.
fn compute_atr_series(bars: &[Bar], period: usize) -> Vec<f64> {
    if bars.len() < 2 {
        return vec![0.0; bars.len()];
    }

    // Compute true range
    let mut tr: Vec<f64> = Vec::with_capacity(bars.len());
    tr.push(bars[0].high - bars[0].low); // First bar: just H-L

    for i in 1..bars.len() {
        let hl = bars[i].high - bars[i].low;
        let hc = (bars[i].high - bars[i - 1].close).abs();
        let lc = (bars[i].low - bars[i - 1].close).abs();
        tr.push(hl.max(hc).max(lc));
    }

    // Compute rolling mean (ATR)
    let mut atr: Vec<f64> = vec![0.0; bars.len()];
    for i in (period - 1)..bars.len() {
        let sum: f64 = tr[(i + 1 - period)..=i].iter().sum();
        atr[i] = sum / period as f64;
    }

    atr
}

/// Compute holding period statistics.
fn compute_holding_stats(
    excursions: &[TradeExcursion],
    config: &AnalysisConfig,
) -> HoldingPeriodStats {
    if excursions.is_empty() {
        return HoldingPeriodStats::default();
    }

    let holdings: Vec<usize> = excursions.iter().map(|e| e.holding_days).collect();
    let n = holdings.len();

    let mean = holdings.iter().sum::<usize>() as f64 / n as f64;
    let variance = holdings
        .iter()
        .map(|h| (*h as f64 - mean).powi(2))
        .sum::<f64>()
        / n as f64;
    let std = variance.sqrt();

    let mut sorted: Vec<usize> = holdings.clone();
    sorted.sort();

    let min = sorted[0];
    let max = sorted[n - 1];
    let median = sorted[n / 2] as f64;
    let p25 = sorted[n / 4] as f64;
    let p75 = sorted[(3 * n) / 4] as f64;

    // Build histogram buckets
    let buckets = &config.holding_buckets;
    let mut histogram: Vec<HoldingBucket> = Vec::new();

    let mut prev_edge = 0;
    for &edge in buckets.iter() {
        let label = if prev_edge == 0 {
            format!("1-{} days", edge)
        } else {
            format!("{}-{} days", prev_edge + 1, edge)
        };

        let in_bucket: Vec<&TradeExcursion> = excursions
            .iter()
            .filter(|e| e.holding_days > prev_edge && e.holding_days <= edge)
            .collect();

        let count = in_bucket.len();
        let pct = count as f64 / n as f64;
        let avg_return = if count > 0 {
            in_bucket.iter().map(|e| e.return_pct).sum::<f64>() / count as f64
        } else {
            0.0
        };
        let win_rate = if count > 0 {
            in_bucket.iter().filter(|e| e.is_winner).count() as f64 / count as f64
        } else {
            0.0
        };

        histogram.push(HoldingBucket {
            label,
            count,
            pct,
            avg_return,
            win_rate,
        });

        prev_edge = edge;
    }

    // Add final bucket (50+ days or whatever the last edge is)
    let last_edge = *buckets.last().unwrap_or(&50);
    let in_bucket: Vec<&TradeExcursion> = excursions
        .iter()
        .filter(|e| e.holding_days > last_edge)
        .collect();
    let count = in_bucket.len();
    if count > 0 {
        histogram.push(HoldingBucket {
            label: format!("{}+ days", last_edge + 1),
            count,
            pct: count as f64 / n as f64,
            avg_return: in_bucket.iter().map(|e| e.return_pct).sum::<f64>() / count as f64,
            win_rate: in_bucket.iter().filter(|e| e.is_winner).count() as f64 / count as f64,
        });
    }

    HoldingPeriodStats {
        mean,
        median,
        std,
        min,
        max,
        p25,
        p75,
        histogram,
    }
}

/// Compute excursion statistics (generic for MAE and MFE).
fn compute_excursion_stats<F>(excursions: &[TradeExcursion], get_value: F) -> ExcursionStats
where
    F: Fn(&TradeExcursion) -> f64,
{
    if excursions.is_empty() {
        return ExcursionStats::default();
    }

    let values: Vec<f64> = excursions.iter().map(&get_value).collect();
    let n = values.len();

    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    let std = variance.sqrt();

    let mut sorted = values.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let median = sorted[n / 2];
    let max = sorted[n - 1];
    let p75 = sorted[(3 * n) / 4];
    let p90 = sorted[(9 * n) / 10];

    // Separate by winners/losers
    let winners: Vec<f64> = excursions
        .iter()
        .filter(|e| e.is_winner)
        .map(&get_value)
        .collect();
    let losers: Vec<f64> = excursions
        .iter()
        .filter(|e| !e.is_winner)
        .map(&get_value)
        .collect();

    let winners_mean = if winners.is_empty() {
        0.0
    } else {
        winners.iter().sum::<f64>() / winners.len() as f64
    };

    let losers_mean = if losers.is_empty() {
        0.0
    } else {
        losers.iter().sum::<f64>() / losers.len() as f64
    };

    ExcursionStats {
        mean,
        median,
        std,
        max,
        p75,
        p90,
        winners_mean,
        losers_mean,
    }
}

/// Compute edge ratio (MFE/MAE) statistics.
fn compute_edge_ratio_stats(excursions: &[TradeExcursion]) -> EdgeRatioStats {
    if excursions.is_empty() {
        return EdgeRatioStats::default();
    }

    let ratios: Vec<f64> = excursions
        .iter()
        .map(|e| {
            if e.mae_pct > 0.0 {
                e.mfe_pct / e.mae_pct
            } else if e.mfe_pct > 0.0 {
                f64::INFINITY
            } else {
                1.0 // Both zero
            }
        })
        .filter(|r| r.is_finite())
        .collect();

    if ratios.is_empty() {
        return EdgeRatioStats::default();
    }

    let n = ratios.len();
    let mean = ratios.iter().sum::<f64>() / n as f64;

    let mut sorted = ratios.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[n / 2];

    let pct_favorable = ratios.iter().filter(|&&r| r > 1.0).count() as f64 / n as f64;

    // By winner/loser
    let winner_ratios: Vec<f64> = excursions
        .iter()
        .filter(|e| e.is_winner && e.mae_pct > 0.0)
        .map(|e| e.mfe_pct / e.mae_pct)
        .filter(|r| r.is_finite())
        .collect();

    let loser_ratios: Vec<f64> = excursions
        .iter()
        .filter(|e| !e.is_winner && e.mae_pct > 0.0)
        .map(|e| e.mfe_pct / e.mae_pct)
        .filter(|r| r.is_finite())
        .collect();

    let winners_mean = if winner_ratios.is_empty() {
        0.0
    } else {
        winner_ratios.iter().sum::<f64>() / winner_ratios.len() as f64
    };

    let losers_mean = if loser_ratios.is_empty() {
        0.0
    } else {
        loser_ratios.iter().sum::<f64>() / loser_ratios.len() as f64
    };

    EdgeRatioStats {
        mean,
        median,
        pct_favorable,
        winners_mean,
        losers_mean,
    }
}

/// Compute volatility at entry statistics.
fn compute_vol_at_entry_stats(excursions: &[TradeExcursion]) -> VolAtEntryStats {
    if excursions.is_empty() {
        return VolAtEntryStats::default();
    }

    let vols: Vec<f64> = excursions.iter().map(|e| e.entry_atr_pct).collect();
    let returns: Vec<f64> = excursions.iter().map(|e| e.return_pct).collect();
    let wins: Vec<f64> = excursions
        .iter()
        .map(|e| if e.is_winner { 1.0 } else { 0.0 })
        .collect();

    let n = vols.len();
    let mean_vol = vols.iter().sum::<f64>() / n as f64;

    let mut sorted_vols = vols.clone();
    sorted_vols.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_vol = sorted_vols[n / 2];

    // Compute correlations
    let return_correlation = pearson_correlation(&vols, &returns);
    let win_correlation = pearson_correlation(&vols, &wins);

    // Mean vol for winners vs losers
    let winners_vols: Vec<f64> = excursions
        .iter()
        .filter(|e| e.is_winner)
        .map(|e| e.entry_atr_pct)
        .collect();
    let losers_vols: Vec<f64> = excursions
        .iter()
        .filter(|e| !e.is_winner)
        .map(|e| e.entry_atr_pct)
        .collect();

    let winners_mean_vol = if winners_vols.is_empty() {
        0.0
    } else {
        winners_vols.iter().sum::<f64>() / winners_vols.len() as f64
    };

    let losers_mean_vol = if losers_vols.is_empty() {
        0.0
    } else {
        losers_vols.iter().sum::<f64>() / losers_vols.len() as f64
    };

    VolAtEntryStats {
        mean_atr_pct: mean_vol,
        median_atr_pct: median_vol,
        return_correlation,
        win_correlation,
        winners_mean_vol,
        losers_mean_vol,
    }
}

/// Compute Pearson correlation coefficient between two vectors.
fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
    if x.len() != y.len() || x.len() < 2 {
        return 0.0;
    }

    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;

    for i in 0..x.len() {
        let dx = x[i] - mean_x;
        let dy = y[i] - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    if var_x == 0.0 || var_y == 0.0 {
        return 0.0;
    }

    cov / (var_x.sqrt() * var_y.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile() {
        // Nearest-rank percentile: idx = round(p * (n-1))
        // For n=10: 0.5 -> round(4.5)=5 -> value 6.0
        //           0.1 -> round(0.9)=1 -> value 2.0
        //           0.9 -> round(8.1)=8 -> value 9.0
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert!((percentile(&data, 0.5) - 6.0).abs() < 0.1);
        assert!((percentile(&data, 0.1) - 2.0).abs() < 0.1);
        assert!((percentile(&data, 0.9) - 9.0).abs() < 0.1);

        // Edge cases
        assert!((percentile(&data, 0.0) - 1.0).abs() < 0.01);
        assert!((percentile(&data, 1.0) - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_skewness_symmetric() {
        // Symmetric distribution should have ~0 skewness
        let data: Vec<f64> = (-50..=50).map(|x| x as f64).collect();
        let mean = 0.0;
        let std = (data.iter().map(|x| x * x).sum::<f64>() / data.len() as f64).sqrt();
        let skew = compute_skewness(&data, mean, std);
        assert!(skew.abs() < 0.01);
    }

    #[test]
    fn test_pearson_correlation_perfect() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![2.0, 4.0, 6.0, 8.0, 10.0];
        let corr = pearson_correlation(&x, &y);
        assert!((corr - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_pearson_correlation_negative() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![10.0, 8.0, 6.0, 4.0, 2.0];
        let corr = pearson_correlation(&x, &y);
        assert!((corr - (-1.0)).abs() < 0.001);
    }

    // =========================================================================
    // DRAWDOWN REGIME TESTS
    // =========================================================================

    #[test]
    fn test_drawdown_regime_classification_normal() {
        // 0-5% drawdown should be Normal
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.0),
            DrawdownRegime::Normal
        );
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.03),
            DrawdownRegime::Normal
        );
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.049),
            DrawdownRegime::Normal
        );
    }

    #[test]
    fn test_drawdown_regime_classification_shallow() {
        // 5-10% drawdown should be Shallow
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.05),
            DrawdownRegime::Shallow
        );
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.07),
            DrawdownRegime::Shallow
        );
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.099),
            DrawdownRegime::Shallow
        );
    }

    #[test]
    fn test_drawdown_regime_classification_deep() {
        // 10-20% drawdown should be Deep
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.10),
            DrawdownRegime::Deep
        );
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.15),
            DrawdownRegime::Deep
        );
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.199),
            DrawdownRegime::Deep
        );
    }

    #[test]
    fn test_drawdown_regime_classification_recovery() {
        // >20% drawdown should be Recovery
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.20),
            DrawdownRegime::Recovery
        );
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.35),
            DrawdownRegime::Recovery
        );
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(0.50),
            DrawdownRegime::Recovery
        );
    }

    #[test]
    fn test_drawdown_regime_handles_negative() {
        // Negative values (shouldn't happen but handle gracefully)
        assert_eq!(
            DrawdownRegime::from_drawdown_pct(-0.05),
            DrawdownRegime::Shallow // Uses abs()
        );
    }

    // =========================================================================
    // HHI CONCENTRATION TESTS
    // =========================================================================

    #[test]
    fn test_hhi_concentration_equal_returns() {
        // Equal returns across 3 regimes should give low concentration
        let returns = [10.0, 10.0, 10.0];
        let (conc, _, _) = compute_hhi_concentration(&returns);
        assert!(
            conc < 0.1,
            "Equal returns should have ~0 concentration: {}",
            conc
        );
    }

    #[test]
    fn test_hhi_concentration_one_dominant() {
        // All returns from one regime should give concentration = 1
        let returns = [100.0, 0.0, 0.0];
        let (conc, dominant_idx, pct) = compute_hhi_concentration(&returns);
        assert!(
            conc > 0.99,
            "Single dominant should have ~1 concentration: {}",
            conc
        );
        assert_eq!(dominant_idx, 0);
        assert!((pct - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hhi_concentration_two_equal() {
        // Two equal regimes, one zero
        let returns = [50.0, 50.0, 0.0];
        let (conc, _, _) = compute_hhi_concentration(&returns);
        // HHI = 0.5^2 + 0.5^2 + 0 = 0.5, normalized = (0.5 - 0.333) / 0.667 = 0.25
        assert!(
            conc > 0.2 && conc < 0.4,
            "Two equal should have ~0.25 concentration: {}",
            conc
        );
    }

    #[test]
    fn test_hhi_concentration_handles_negative_returns() {
        // Mix of positive and negative returns
        let returns = [30.0, -10.0, 20.0];
        let (conc, dominant_idx, _) = compute_hhi_concentration(&returns);
        // Should use absolute values for share calculation
        assert!(
            conc < 0.5,
            "Mixed returns should have moderate concentration"
        );
        assert_eq!(dominant_idx, 0); // 30 is largest absolute
    }

    #[test]
    fn test_hhi_concentration_all_zero() {
        let returns = [0.0, 0.0, 0.0];
        let (conc, _, _) = compute_hhi_concentration(&returns);
        assert!(
            (conc - 0.0).abs() < 0.01,
            "All zero should have 0 concentration"
        );
    }

    #[test]
    fn test_hhi_concentration_empty() {
        let returns: [f64; 0] = [];
        let (conc, idx, pct) = compute_hhi_concentration(&returns);
        assert!((conc - 0.0).abs() < 0.01);
        assert_eq!(idx, 0);
        assert!((pct - 0.0).abs() < 0.01);
    }

    // =========================================================================
    // REGIME CONCENTRATION SCORE TESTS
    // =========================================================================

    #[test]
    fn test_regime_concentration_score_balanced() {
        // Create a balanced regime analysis
        let vol_analysis = RegimeAnalysis {
            high_vol: RegimeMetrics {
                total_return: 0.10,
                ..Default::default()
            },
            neutral_vol: RegimeMetrics {
                total_return: 0.10,
                ..Default::default()
            },
            low_vol: RegimeMetrics {
                total_return: 0.10,
                ..Default::default()
            },
            ..Default::default()
        };

        let score = compute_regime_concentration_score(&vol_analysis, None, None);
        assert!(
            score.vol_concentration < 0.1,
            "Balanced should have low concentration"
        );
        assert!(!score.is_concentrated());
        assert!((score.penalty_factor() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_regime_concentration_score_concentrated() {
        // Create a concentrated regime analysis (all returns from high vol)
        let vol_analysis = RegimeAnalysis {
            high_vol: RegimeMetrics {
                total_return: 0.50,
                ..Default::default()
            },
            neutral_vol: RegimeMetrics {
                total_return: 0.0,
                ..Default::default()
            },
            low_vol: RegimeMetrics {
                total_return: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let score = compute_regime_concentration_score(&vol_analysis, None, None);
        assert!(
            score.vol_concentration > 0.9,
            "Concentrated should have high score"
        );
        assert!(score.is_concentrated());
        assert!(score.penalty_factor() > 0.5);
        assert_eq!(score.dominant_vol_regime, Some(VolRegime::High));
    }

    #[test]
    fn test_regime_concentration_score_with_trend() {
        let vol_analysis = RegimeAnalysis {
            high_vol: RegimeMetrics {
                total_return: 0.10,
                ..Default::default()
            },
            neutral_vol: RegimeMetrics {
                total_return: 0.10,
                ..Default::default()
            },
            low_vol: RegimeMetrics {
                total_return: 0.10,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut by_regime = std::collections::HashMap::new();
        by_regime.insert(
            TrendRegime::StrongUp,
            RegimeMetrics {
                total_return: 0.50,
                ..Default::default()
            },
        );
        by_regime.insert(
            TrendRegime::WeakUp,
            RegimeMetrics {
                total_return: 0.0,
                ..Default::default()
            },
        );
        by_regime.insert(
            TrendRegime::Neutral,
            RegimeMetrics {
                total_return: 0.0,
                ..Default::default()
            },
        );
        by_regime.insert(
            TrendRegime::WeakDown,
            RegimeMetrics {
                total_return: 0.0,
                ..Default::default()
            },
        );
        by_regime.insert(
            TrendRegime::StrongDown,
            RegimeMetrics {
                total_return: 0.0,
                ..Default::default()
            },
        );

        let trend_analysis = TrendRegimeAnalysis {
            by_regime,
            ..Default::default()
        };

        let score = compute_regime_concentration_score(&vol_analysis, Some(&trend_analysis), None);

        // Vol is balanced (0), trend is concentrated (1), combined = 0.5*0 + 0.5*1 = 0.5
        assert!(score.trend_concentration > 0.9);
        assert!(score.combined_score > 0.4 && score.combined_score < 0.6);
        assert_eq!(score.dominant_trend_regime, Some(TrendRegime::StrongUp));
    }

    // =========================================================================
    // PENALTY FACTOR TESTS
    // =========================================================================

    #[test]
    fn test_penalty_factor_below_threshold() {
        let score = RegimeConcentrationScore {
            combined_score: 0.3,
            ..Default::default()
        };
        assert!((score.penalty_factor() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_penalty_factor_at_threshold() {
        let score = RegimeConcentrationScore {
            combined_score: 0.5,
            ..Default::default()
        };
        assert!((score.penalty_factor() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_penalty_factor_above_threshold() {
        let score = RegimeConcentrationScore {
            combined_score: 0.75,
            ..Default::default()
        };
        // (0.75 - 0.5) / 0.5 = 0.5
        assert!((score.penalty_factor() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_penalty_factor_maximum() {
        let score = RegimeConcentrationScore {
            combined_score: 1.0,
            ..Default::default()
        };
        assert!((score.penalty_factor() - 1.0).abs() < 0.01);
    }

    // =========================================================================
    // TREND REGIME ALL() TESTS
    // =========================================================================

    #[test]
    fn test_trend_regime_all_variants() {
        let all = TrendRegime::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&TrendRegime::StrongUp));
        assert!(all.contains(&TrendRegime::WeakUp));
        assert!(all.contains(&TrendRegime::Neutral));
        assert!(all.contains(&TrendRegime::WeakDown));
        assert!(all.contains(&TrendRegime::StrongDown));
    }

    #[test]
    fn test_drawdown_regime_all_variants() {
        let all = DrawdownRegime::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&DrawdownRegime::Normal));
        assert!(all.contains(&DrawdownRegime::Shallow));
        assert!(all.contains(&DrawdownRegime::Deep));
        assert!(all.contains(&DrawdownRegime::Recovery));
    }

    #[test]
    fn test_trend_regime_display_names() {
        assert_eq!(TrendRegime::StrongUp.display_name(), "Strong Up");
        assert_eq!(TrendRegime::WeakDown.display_name(), "Weak Down");
        assert_eq!(TrendRegime::Neutral.display_name(), "Neutral");
    }

    #[test]
    fn test_drawdown_regime_display_names() {
        assert_eq!(DrawdownRegime::Normal.display_name(), "Normal (<5%)");
        assert_eq!(DrawdownRegime::Deep.display_name(), "Deep (10-20%)");
        assert_eq!(DrawdownRegime::Recovery.display_name(), "Recovery (>20%)");
    }
}
