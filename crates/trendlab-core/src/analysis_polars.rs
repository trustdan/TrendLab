//! Polars-based statistical analysis computations.
//!
//! Implements vectorized computation for:
//! - Return distribution metrics (VaR, CVaR, skewness, kurtosis)
//! - Regime-based performance analysis
//! - Trade-level statistics (MAE, MFE, holding period)

use crate::analysis::{
    AnalysisConfig, EdgeRatioStats, ExcursionStats, HoldingBucket, HoldingPeriodStats,
    RegimeAnalysis, RegimeMetrics, ReturnDistribution, StatisticalAnalysis, TradeAnalysis,
    TradeExcursion, VolAtEntryStats, VolRegime,
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
pub fn compute_return_distribution(equity: &[EquityPoint]) -> Result<ReturnDistribution, PolarsError> {
    if equity.len() < 2 {
        return Ok(ReturnDistribution::default());
    }

    // Extract equity values and compute daily returns
    let equities: Vec<f64> = equity.iter().map(|e| e.equity).collect();

    let df = DataFrame::new(vec![
        Column::new("equity".into(), equities),
    ])?;

    // Compute daily returns
    let returns_df = df
        .lazy()
        .with_column(
            (col("equity") / col("equity").shift(lit(1)) - lit(1.0)).alias("return"),
        )
        .collect()?;

    // Extract returns, filtering nulls
    let returns_col = returns_df.column("return")?.f64()?;
    let returns: Vec<f64> = returns_col
        .into_iter()
        .filter_map(|x| x)
        .collect();

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
    let atr_values: Vec<f64> = atr_col.into_iter().filter_map(|x| x).collect();

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
        neutral_vol: compute_regime_metrics(&neutral_trades, neutral_days, total_days, &neutral_returns),
        low_vol: compute_regime_metrics(&low_trades, low_days, total_days, &low_returns),
        median_atr,
        atr_period: config.atr_period,
    })
}

/// Compute equity returns from equity points.
fn compute_equity_returns(equity: &[EquityPoint]) -> Vec<(i64, f64)> {
    equity
        .windows(2)
        .map(|w| (w[1].ts.timestamp(), (w[1].equity - w[0].equity) / w[0].equity))
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
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
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
    let ts_to_idx: std::collections::HashMap<i64, usize> =
        bars.iter().enumerate().map(|(i, b)| (b.ts.timestamp(), i)).collect();

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
fn compute_holding_stats(excursions: &[TradeExcursion], config: &AnalysisConfig) -> HoldingPeriodStats {
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
}
