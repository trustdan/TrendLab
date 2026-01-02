//! Parameter sweep infrastructure for systematic strategy exploration.
//!
//! This module provides:
//! - Sweep grid definition and iteration
//! - Parallel sweep execution with rayon
//! - Run manifests for reproducibility
//! - Ranking and stability analysis

use crate::backtest::{run_backtest, BacktestConfig, BacktestResult, CostModel};
use crate::bar::Bar;
use crate::indicators::MACDEntryMode;
use crate::indicators::MAType;
use crate::indicators::OpeningPeriod;
use crate::metrics::{compute_metrics, Metrics};
use crate::strategy::{
    AroonCrossStrategy, BollingerSqueezeStrategy, CCIStrategy, DarvasBoxStrategy, DmiAdxStrategy,
    DonchianBreakoutStrategy, EnsembleStrategy, FiftyTwoWeekHighMomentumStrategy,
    FiftyTwoWeekHighStrategy, FiftyTwoWeekHighTrailingStrategy, HeikinAshiRegimeStrategy,
    IchimokuStrategy, KeltnerBreakoutStrategy, LarryWilliamsStrategy, MACDAdxStrategy,
    MACDStrategy, MACrossoverStrategy, OpeningRangeBreakoutStrategy, OscillatorConfluenceStrategy,
    ParabolicSARStrategy, ParabolicSarDelayedStrategy, ParabolicSarFilteredStrategy, ROCStrategy,
    RSIBollingerStrategy, RSIStrategy, STARCBreakoutStrategy, StochasticStrategy, Strategy,
    SupertrendAsymmetricStrategy, SupertrendConfirmedStrategy, SupertrendCooldownStrategy,
    SupertrendStrategy, SupertrendVolumeStrategy, TsmomStrategy, VotingMethod, WilliamsRStrategy,
};
use crate::TrendLabError;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// =============================================================================
// Sweep Depth Presets
// =============================================================================

/// Sweep depth presets controlling parameter range coverage.
///
/// Based on research from Turtle Trading, academic TSMOM papers, and
/// Golden Cross/Death Cross studies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SweepDepth {
    /// Quick sweep with core classic parameters (~11 configs)
    /// - Donchian: 20/55 entry, 10/20 exit (Turtle classics)
    /// - MA: 50/200 SMA only (Golden Cross)
    /// - TSMOM: 63, 252 days (quarterly + annual)
    Quick,

    /// Standard sweep with balanced coverage (~40 configs)
    /// Current default parameter ranges.
    #[default]
    Standard,

    /// Comprehensive sweep with extended + fine granularity (~88 configs)
    /// - Extended ranges covering day trading to position trading
    /// - Finer step sizes for more thorough exploration
    Comprehensive,
}

impl SweepDepth {
    /// Get all sweep depth options.
    pub fn all() -> Vec<Self> {
        vec![Self::Quick, Self::Standard, Self::Comprehensive]
    }

    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Quick => "Quick",
            Self::Standard => "Standard",
            Self::Comprehensive => "Comprehensive",
        }
    }

    /// Get description.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Quick => "Core classics only (~11 configs)",
            Self::Standard => "Balanced coverage (~40 configs)",
            Self::Comprehensive => "Extended + fine granularity (~88 configs)",
        }
    }

    /// Estimated total config count across all strategies.
    pub fn estimated_configs(&self) -> usize {
        match self {
            Self::Quick => 11,
            Self::Standard => 40,
            Self::Comprehensive => 88,
        }
    }
}

// =============================================================================
// Basic Sweep Types
// =============================================================================

/// A parameter grid for sweep exploration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepGrid {
    /// List of entry lookback values to test
    pub entry_lookbacks: Vec<usize>,
    /// List of exit lookback values to test
    pub exit_lookbacks: Vec<usize>,
}

impl SweepGrid {
    pub fn new(entry_lookbacks: Vec<usize>, exit_lookbacks: Vec<usize>) -> Self {
        Self {
            entry_lookbacks,
            exit_lookbacks,
        }
    }

    /// Returns all parameter combinations as (entry_lookback, exit_lookback) tuples.
    pub fn combinations(&self) -> Vec<(usize, usize)> {
        let mut combos = Vec::new();
        for &entry in &self.entry_lookbacks {
            for &exit in &self.exit_lookbacks {
                combos.push((entry, exit));
            }
        }
        combos
    }

    /// Total number of configurations in this grid.
    pub fn len(&self) -> usize {
        self.entry_lookbacks.len() * self.exit_lookbacks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entry_lookbacks.is_empty() || self.exit_lookbacks.is_empty()
    }
}

/// Configuration for a single backtest run within a sweep.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigId {
    pub entry_lookback: usize,
    pub exit_lookback: usize,
}

impl ConfigId {
    pub fn new(entry_lookback: usize, exit_lookback: usize) -> Self {
        Self {
            entry_lookback,
            exit_lookback,
        }
    }

    /// Returns a string identifier for this config.
    pub fn id(&self) -> String {
        format!("donchian_{}_{}", self.entry_lookback, self.exit_lookback)
    }
}

/// Result for a single configuration in a sweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepConfigResult {
    pub config_id: ConfigId,
    pub backtest_result: BacktestResult,
    pub metrics: Metrics,
}

/// Complete sweep result containing all configuration results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepResult {
    pub sweep_id: String,
    pub config_results: Vec<SweepConfigResult>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

impl SweepResult {
    /// Returns the number of configurations that were tested.
    pub fn len(&self) -> usize {
        self.config_results.len()
    }

    pub fn is_empty(&self) -> bool {
        self.config_results.is_empty()
    }

    /// Get result for a specific configuration.
    pub fn get(&self, config_id: &ConfigId) -> Option<&SweepConfigResult> {
        self.config_results
            .iter()
            .find(|r| &r.config_id == config_id)
    }

    /// Returns configs ranked by a metric (descending by default).
    pub fn rank_by(&self, metric: RankMetric, ascending: bool) -> Vec<&SweepConfigResult> {
        let mut results: Vec<&SweepConfigResult> = self.config_results.iter().collect();
        results.sort_by(|a, b| {
            let val_a = metric.extract(&a.metrics);
            let val_b = metric.extract(&b.metrics);
            if ascending {
                val_a
                    .partial_cmp(&val_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else {
                val_b
                    .partial_cmp(&val_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        });
        results
    }

    /// Returns the top N configurations by a metric.
    pub fn top_n(&self, n: usize, metric: RankMetric, ascending: bool) -> Vec<&SweepConfigResult> {
        self.rank_by(metric, ascending)
            .into_iter()
            .take(n)
            .collect()
    }
}

/// Metrics available for ranking.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RankMetric {
    Sharpe,
    Cagr,
    Sortino,
    MaxDrawdown,
    Calmar,
    WinRate,
    ProfitFactor,
    TotalReturn,
}

impl RankMetric {
    fn extract(&self, m: &Metrics) -> f64 {
        match self {
            RankMetric::Sharpe => m.sharpe,
            RankMetric::Cagr => m.cagr,
            RankMetric::Sortino => m.sortino,
            RankMetric::MaxDrawdown => m.max_drawdown,
            RankMetric::Calmar => m.calmar,
            RankMetric::WinRate => m.win_rate,
            RankMetric::ProfitFactor => m.profit_factor,
            RankMetric::TotalReturn => m.total_return,
        }
    }
}

/// Run manifest for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub sweep_id: String,
    pub sweep_config: SweepConfig,
    pub data_version: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub result_paths: ResultPaths,
}

/// Configuration for a sweep run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepConfig {
    pub grid: SweepGrid,
    pub backtest_config: BacktestConfig,
    pub symbol: String,
    pub start_date: String,
    pub end_date: String,
}

/// Paths to sweep output files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultPaths {
    pub manifest: PathBuf,
    pub results_parquet: PathBuf,
    pub summary_md: PathBuf,
}

impl ResultPaths {
    pub fn for_sweep(sweep_id: &str) -> Self {
        let base = PathBuf::from(format!("reports/runs/{}", sweep_id));
        Self {
            manifest: base.join("manifest.json"),
            results_parquet: base.join("results.parquet"),
            summary_md: base.join("summary.md"),
        }
    }
}

/// Neighbor sensitivity analysis for stability assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborSensitivity {
    pub config_id: ConfigId,
    pub metric: RankMetric,
    /// Performance of immediate neighbors (+/- 1 step)
    pub neighbors_1: Vec<f64>,
    /// Performance of extended neighbors (+/- 2 steps)
    pub neighbors_2: Vec<f64>,
    /// Variance across all neighbors
    pub variance: f64,
    /// Stability score (higher = more stable)
    pub stability_score: f64,
}

/// Cost sensitivity analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSensitivity {
    pub config_id: ConfigId,
    pub cost_levels: Vec<f64>,
    pub returns_at_cost: Vec<f64>,
    pub breakeven_cost_bps: Option<f64>,
}

/// Run a parameter sweep over the given bars.
pub fn run_sweep(bars: &[Bar], grid: &SweepGrid, backtest_config: BacktestConfig) -> SweepResult {
    let sweep_id = format!("sweep_{}", Utc::now().format("%Y%m%d_%H%M%S"));
    let started_at = Utc::now();

    let combinations = grid.combinations();
    let total_configs = combinations.len();

    tracing::info!(
        sweep_id = %sweep_id,
        configs = total_configs,
        bars = bars.len(),
        "Starting parameter sweep"
    );

    let config_results: Vec<SweepConfigResult> = combinations
        .par_iter()
        .filter_map(|&(entry, exit)| {
            let config_id = ConfigId::new(entry, exit);
            let mut strategy = DonchianBreakoutStrategy::new(entry, exit);

            tracing::trace!(entry = entry, exit = exit, "Evaluating config");

            match run_backtest(bars, &mut strategy, backtest_config) {
                Ok(backtest_result) => {
                    let metrics = compute_metrics(&backtest_result, backtest_config.initial_cash);
                    Some(SweepConfigResult {
                        config_id,
                        backtest_result,
                        metrics,
                    })
                }
                Err(e) => {
                    tracing::warn!(
                        entry = entry,
                        exit = exit,
                        error = %e,
                        "Backtest failed for config, skipping"
                    );
                    None
                }
            }
        })
        .collect();

    let completed_at = Utc::now();
    let elapsed_ms = (completed_at - started_at).num_milliseconds();

    tracing::info!(
        sweep_id = %sweep_id,
        configs = total_configs,
        elapsed_ms = elapsed_ms,
        "Sweep completed"
    );

    SweepResult {
        sweep_id,
        config_results,
        started_at,
        completed_at,
    }
}

/// Compute neighbor sensitivity for a specific configuration.
pub fn compute_neighbor_sensitivity(
    sweep_result: &SweepResult,
    config_id: &ConfigId,
    metric: RankMetric,
) -> Option<NeighborSensitivity> {
    let target = sweep_result.get(config_id)?;
    let target_value = metric.extract(&target.metrics);

    let mut neighbors_1 = Vec::new();
    let mut neighbors_2 = Vec::new();

    // Find neighbors by parameter distance
    for result in &sweep_result.config_results {
        let entry_diff =
            (result.config_id.entry_lookback as i32 - config_id.entry_lookback as i32).abs();
        let exit_diff =
            (result.config_id.exit_lookback as i32 - config_id.exit_lookback as i32).abs();
        let max_diff = entry_diff.max(exit_diff);

        if max_diff == 1 {
            neighbors_1.push(metric.extract(&result.metrics));
        } else if max_diff == 2 {
            neighbors_2.push(metric.extract(&result.metrics));
        }
    }

    // Calculate variance across all neighbors
    let all_neighbors: Vec<f64> = neighbors_1
        .iter()
        .chain(neighbors_2.iter())
        .copied()
        .collect();

    let variance = if !all_neighbors.is_empty() {
        let mean: f64 = all_neighbors.iter().sum::<f64>() / all_neighbors.len() as f64;
        all_neighbors
            .iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>()
            / all_neighbors.len() as f64
    } else {
        0.0
    };

    // Stability score: higher when performance is consistent with neighbors
    // and lower when the config is an outlier
    let stability_score = if variance > 0.0 {
        let neighbor_mean: f64 = if !all_neighbors.is_empty() {
            all_neighbors.iter().sum::<f64>() / all_neighbors.len() as f64
        } else {
            target_value
        };
        let deviation = (target_value - neighbor_mean).abs();
        let normalized_deviation = deviation / variance.sqrt().max(0.001);
        (1.0 / (1.0 + normalized_deviation)).min(1.0)
    } else {
        1.0 // No variance means perfectly stable
    };

    Some(NeighborSensitivity {
        config_id: config_id.clone(),
        metric,
        neighbors_1,
        neighbors_2,
        variance,
        stability_score,
    })
}

/// Compute cost sensitivity for a configuration.
///
/// Returns an error if any backtest fails.
pub fn compute_cost_sensitivity(
    bars: &[Bar],
    config_id: &ConfigId,
    base_config: BacktestConfig,
    cost_levels_bps: &[f64],
) -> Result<CostSensitivity, TrendLabError> {
    let mut returns_at_cost = Vec::new();
    let mut breakeven_cost_bps = None;

    for &cost_bps in cost_levels_bps {
        let mut config = base_config;
        config.cost_model = CostModel {
            fees_bps_per_side: cost_bps,
            slippage_bps: 0.0,
        };

        let mut strategy =
            DonchianBreakoutStrategy::new(config_id.entry_lookback, config_id.exit_lookback);
        let result = run_backtest(bars, &mut strategy, config)?;
        let metrics = compute_metrics(&result, config.initial_cash);

        returns_at_cost.push(metrics.total_return);

        // Check for breakeven (returns going negative)
        if breakeven_cost_bps.is_none() && metrics.total_return <= 0.0 {
            breakeven_cost_bps = Some(cost_bps);
        }
    }

    Ok(CostSensitivity {
        config_id: config_id.clone(),
        cost_levels: cost_levels_bps.to_vec(),
        returns_at_cost,
        breakeven_cost_bps,
    })
}

/// Result for a multi-symbol sweep containing per-symbol results and aggregated portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSweepResult {
    /// Unique identifier for this multi-sweep run
    pub sweep_id: String,
    /// Results per symbol
    pub symbol_results: std::collections::HashMap<String, SweepResult>,
    /// Aggregated portfolio performance (equal-weighted across symbols)
    pub aggregated: Option<AggregatedPortfolioResult>,
    /// When the sweep started
    pub started_at: DateTime<Utc>,
    /// When the sweep completed
    pub completed_at: DateTime<Utc>,
}

impl MultiSweepResult {
    /// Create a new multi-sweep result.
    pub fn new(sweep_id: String) -> Self {
        Self {
            sweep_id,
            symbol_results: std::collections::HashMap::new(),
            aggregated: None,
            started_at: Utc::now(),
            completed_at: Utc::now(),
        }
    }

    /// Returns the number of symbols that were tested.
    pub fn symbol_count(&self) -> usize {
        self.symbol_results.len()
    }

    /// Returns total number of configurations tested across all symbols.
    pub fn total_configs(&self) -> usize {
        self.symbol_results.values().map(|r| r.len()).sum()
    }

    /// Add a symbol's sweep result.
    pub fn add_symbol_result(&mut self, symbol: String, result: SweepResult) {
        self.symbol_results.insert(symbol, result);
    }

    /// Get the best config for a symbol.
    pub fn best_for_symbol(&self, symbol: &str, metric: RankMetric) -> Option<&SweepConfigResult> {
        self.symbol_results
            .get(symbol)
            .and_then(|r| r.top_n(1, metric, false).first().copied())
    }
}

/// Aggregated portfolio-level results across multiple symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedPortfolioResult {
    /// Combined equity curve (equal-weighted sum)
    pub equity_curve: Vec<f64>,
    /// Portfolio-level metrics
    pub metrics: Metrics,
    /// Best config per symbol
    pub best_configs: std::collections::HashMap<String, ConfigId>,
    /// Per-symbol contribution to total return
    pub symbol_contributions: std::collections::HashMap<String, f64>,
}

impl AggregatedPortfolioResult {
    /// Create aggregated results from individual sweep results.
    pub fn from_symbol_results(
        symbol_results: &std::collections::HashMap<String, SweepResult>,
        metric: RankMetric,
    ) -> Option<Self> {
        if symbol_results.is_empty() {
            return None;
        }

        let mut best_configs = std::collections::HashMap::new();
        let mut symbol_contributions = std::collections::HashMap::new();
        let mut equity_curves: Vec<Vec<f64>> = Vec::new();

        for (symbol, sweep_result) in symbol_results {
            if let Some(best) = sweep_result.top_n(1, metric, false).first() {
                best_configs.insert(symbol.clone(), best.config_id.clone());
                symbol_contributions.insert(symbol.clone(), best.metrics.total_return);

                // Extract equity curve
                let curve: Vec<f64> = best
                    .backtest_result
                    .equity
                    .iter()
                    .map(|p| p.equity)
                    .collect();
                equity_curves.push(curve);
            }
        }

        if equity_curves.is_empty() {
            return None;
        }

        // Compute equal-weighted portfolio equity curve
        let max_len = equity_curves.iter().map(|c| c.len()).max().unwrap_or(0);
        let mut portfolio_equity = vec![0.0; max_len];
        let n_symbols = equity_curves.len() as f64;

        for curve in &equity_curves {
            for (i, &equity) in curve.iter().enumerate() {
                portfolio_equity[i] += equity / n_symbols;
            }
        }

        // Compute portfolio metrics from the combined equity curve
        let total_return = if !portfolio_equity.is_empty() && portfolio_equity[0] > 0.0 {
            (portfolio_equity.last().unwrap_or(&0.0) / portfolio_equity[0]) - 1.0
        } else {
            0.0
        };

        // Simple metrics for aggregated portfolio
        let max_equity = portfolio_equity.iter().cloned().fold(0.0_f64, f64::max);
        let min_after_max = portfolio_equity.iter().cloned().fold(max_equity, f64::min);
        let max_drawdown = if max_equity > 0.0 {
            (min_after_max - max_equity) / max_equity
        } else {
            0.0
        };

        let metrics = Metrics {
            total_return,
            cagr: total_return, // Simplified - would need date range for proper CAGR
            sharpe: 0.0,        // Would need daily returns for proper Sharpe
            sortino: 0.0,
            max_drawdown,
            calmar: if max_drawdown.abs() > 0.0001 {
                total_return / max_drawdown.abs()
            } else {
                0.0
            },
            win_rate: 0.0,
            profit_factor: 0.0,
            num_trades: 0,
            turnover: 0.0,
            max_consecutive_losses: 0,
            max_consecutive_wins: 0,
            avg_losing_streak: 0.0,
        };

        Some(Self {
            equity_curve: portfolio_equity,
            metrics,
            best_configs,
            symbol_contributions,
        })
    }
}

/// Generate a summary markdown report for a sweep.
pub fn generate_summary_markdown(sweep_result: &SweepResult, top_n: usize) -> String {
    let mut md = String::new();

    md.push_str(&format!("# Sweep Summary: {}\n\n", sweep_result.sweep_id));
    md.push_str(&format!(
        "**Run Time:** {} - {}\n\n",
        sweep_result.started_at.format("%Y-%m-%d %H:%M:%S UTC"),
        sweep_result.completed_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    md.push_str(&format!(
        "**Configurations Tested:** {}\n\n",
        sweep_result.len()
    ));

    md.push_str("## Top Configurations by Sharpe\n\n");
    md.push_str("| Rank | Entry | Exit | Sharpe | CAGR | MaxDD | Trades |\n");
    md.push_str("|------|-------|------|--------|------|-------|--------|\n");

    let top = sweep_result.top_n(top_n, RankMetric::Sharpe, false);
    for (i, result) in top.iter().enumerate() {
        md.push_str(&format!(
            "| {} | {} | {} | {:.3} | {:.1}% | {:.1}% | {} |\n",
            i + 1,
            result.config_id.entry_lookback,
            result.config_id.exit_lookback,
            result.metrics.sharpe,
            result.metrics.cagr * 100.0,
            result.metrics.max_drawdown * 100.0,
            result.metrics.num_trades
        ));
    }

    md.push_str("\n## Top Configurations by CAGR\n\n");
    md.push_str("| Rank | Entry | Exit | CAGR | Sharpe | MaxDD | Trades |\n");
    md.push_str("|------|-------|------|------|--------|-------|--------|\n");

    let top_cagr = sweep_result.top_n(top_n, RankMetric::Cagr, false);
    for (i, result) in top_cagr.iter().enumerate() {
        md.push_str(&format!(
            "| {} | {} | {} | {:.1}% | {:.3} | {:.1}% | {} |\n",
            i + 1,
            result.config_id.entry_lookback,
            result.config_id.exit_lookback,
            result.metrics.cagr * 100.0,
            result.metrics.sharpe,
            result.metrics.max_drawdown * 100.0,
            result.metrics.num_trades
        ));
    }

    md
}

// =============================================================================
// Multi-Strategy Sweep Types
// =============================================================================

/// Identifier for strategy types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrategyTypeId {
    Donchian,
    TurtleS1,
    TurtleS2,
    MACrossover,
    Tsmom,
    // Phase 1: ATR-Based Channels
    Keltner,
    STARC,
    Supertrend,
    SupertrendVolume,     // Volume filter on entry
    SupertrendConfirmed,  // Confirmation bars before entry
    SupertrendAsymmetric, // Different exit multiplier
    SupertrendCooldown,   // Re-entry cooldown
    // Phase 2: Momentum & Direction
    DmiAdx,
    Aroon,
    BollingerSqueeze,
    // Phase 3: Price Structure
    FiftyTwoWeekHigh,
    FiftyTwoWeekHighMomentum, // ROC filter for acceleration
    FiftyTwoWeekHighTrailing, // Trailing stop instead of fixed exit
    DarvasBox,
    LarryWilliams,
    HeikinAshi,
    // Phase 4: Complex Stateful + Ensemble
    ParabolicSar,
    ParabolicSarFiltered, // MA trend filter
    ParabolicSarDelayed,  // Delay bars after flip
    OpeningRangeBreakout,
    Ensemble,
    // Phase 5: Oscillator Strategies
    Rsi,
    Macd,
    Stochastic,
    WilliamsR,
    Cci,
    Roc,
    RsiBollinger,
    MacdAdx,
    OscillatorConfluence,
    Ichimoku,
}

impl StrategyTypeId {
    /// Get all strategy types.
    pub fn all() -> Vec<Self> {
        vec![
            Self::Donchian,
            Self::TurtleS1,
            Self::TurtleS2,
            Self::MACrossover,
            Self::Tsmom,
            // Phase 1
            Self::Keltner,
            Self::STARC,
            Self::Supertrend,
            Self::SupertrendVolume,
            Self::SupertrendConfirmed,
            Self::SupertrendAsymmetric,
            Self::SupertrendCooldown,
            // Phase 2
            Self::DmiAdx,
            Self::Aroon,
            Self::BollingerSqueeze,
            // Phase 3
            Self::FiftyTwoWeekHigh,
            Self::FiftyTwoWeekHighMomentum,
            Self::FiftyTwoWeekHighTrailing,
            Self::DarvasBox,
            Self::LarryWilliams,
            Self::HeikinAshi,
            // Phase 4
            Self::ParabolicSar,
            Self::ParabolicSarFiltered,
            Self::ParabolicSarDelayed,
            Self::OpeningRangeBreakout,
            Self::Ensemble,
            // Phase 5
            Self::Rsi,
            Self::Macd,
            Self::Stochastic,
            Self::WilliamsR,
            Self::Cci,
            Self::Roc,
            Self::RsiBollinger,
            Self::MacdAdx,
            Self::OscillatorConfluence,
            Self::Ichimoku,
        ]
    }

    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Donchian => "Donchian Breakout",
            Self::TurtleS1 => "Turtle System 1",
            Self::TurtleS2 => "Turtle System 2",
            Self::MACrossover => "MA Crossover",
            Self::Tsmom => "TSMOM",
            Self::Keltner => "Keltner Channel",
            Self::STARC => "STARC Bands",
            Self::Supertrend => "Supertrend",
            Self::SupertrendVolume => "Supertrend + Volume",
            Self::SupertrendConfirmed => "Supertrend Confirmed",
            Self::SupertrendAsymmetric => "Supertrend Asymmetric",
            Self::SupertrendCooldown => "Supertrend Cooldown",
            Self::DmiAdx => "DMI/ADX Directional",
            Self::Aroon => "Aroon Cross",
            Self::BollingerSqueeze => "Bollinger Squeeze",
            Self::FiftyTwoWeekHigh => "52-Week High",
            Self::FiftyTwoWeekHighMomentum => "52-Week High Momentum",
            Self::FiftyTwoWeekHighTrailing => "52-Week High Trailing",
            Self::DarvasBox => "Darvas Box",
            Self::LarryWilliams => "Larry Williams",
            Self::HeikinAshi => "Heikin-Ashi Regime",
            Self::ParabolicSar => "Parabolic SAR",
            Self::ParabolicSarFiltered => "Parabolic SAR Filtered",
            Self::ParabolicSarDelayed => "Parabolic SAR Delayed",
            Self::OpeningRangeBreakout => "Opening Range Breakout",
            Self::Ensemble => "Multi-Horizon Ensemble",
            // Phase 5
            Self::Rsi => "RSI Crossover",
            Self::Macd => "MACD Crossover",
            Self::Stochastic => "Stochastic Crossover",
            Self::WilliamsR => "Williams %R",
            Self::Cci => "CCI Breakout",
            Self::Roc => "Rate of Change",
            Self::RsiBollinger => "RSI + Bollinger Bands",
            Self::MacdAdx => "MACD + ADX Filter",
            Self::OscillatorConfluence => "Oscillator Confluence",
            Self::Ichimoku => "Ichimoku Cloud",
        }
    }

    /// Get short identifier.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Donchian => "donchian",
            Self::TurtleS1 => "turtle_s1",
            Self::TurtleS2 => "turtle_s2",
            Self::MACrossover => "ma_crossover",
            Self::Tsmom => "tsmom",
            Self::Keltner => "keltner",
            Self::STARC => "starc",
            Self::Supertrend => "supertrend",
            Self::SupertrendVolume => "supertrend_volume",
            Self::SupertrendConfirmed => "supertrend_confirmed",
            Self::SupertrendAsymmetric => "supertrend_asymmetric",
            Self::SupertrendCooldown => "supertrend_cooldown",
            Self::DmiAdx => "dmi_adx",
            Self::Aroon => "aroon",
            Self::BollingerSqueeze => "bollinger_squeeze",
            Self::FiftyTwoWeekHigh => "52wk_high",
            Self::FiftyTwoWeekHighMomentum => "52wk_high_momentum",
            Self::FiftyTwoWeekHighTrailing => "52wk_high_trailing",
            Self::DarvasBox => "darvas_box",
            Self::LarryWilliams => "larry_williams",
            Self::HeikinAshi => "heikin_ashi",
            Self::ParabolicSar => "parabolic_sar",
            Self::ParabolicSarFiltered => "parabolic_sar_filtered",
            Self::ParabolicSarDelayed => "parabolic_sar_delayed",
            Self::OpeningRangeBreakout => "orb",
            Self::Ensemble => "ensemble",
            // Phase 5: Oscillator Strategies
            Self::Rsi => "rsi",
            Self::Macd => "macd",
            Self::Stochastic => "stochastic",
            Self::WilliamsR => "williams_r",
            Self::Cci => "cci",
            Self::Roc => "roc",
            Self::RsiBollinger => "rsi_bollinger",
            Self::MacdAdx => "macd_adx",
            Self::OscillatorConfluence => "oscillator_confluence",
            Self::Ichimoku => "ichimoku",
        }
    }
}

/// Generic config identifier for any strategy.
///
/// Note: Implements Eq and Hash manually using f64.to_bits() for float fields,
/// enabling use as HashMap keys for indicator caching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyConfigId {
    Donchian {
        entry_lookback: usize,
        exit_lookback: usize,
    },
    TurtleS1, // Fixed: 20/10
    TurtleS2, // Fixed: 55/20
    MACrossover {
        fast: usize,
        slow: usize,
        ma_type: MAType,
    },
    Tsmom {
        lookback: usize,
    },
    // Phase 2: Momentum & Direction
    DmiAdx {
        di_period: usize,
        adx_period: usize,
        adx_threshold: f64,
    },
    Aroon {
        period: usize,
    },
    BollingerSqueeze {
        period: usize,
        std_mult: f64,
        squeeze_threshold: f64,
    },
    // Phase 1: ATR-Based Channels
    Keltner {
        ema_period: usize,
        atr_period: usize,
        multiplier: f64,
    },
    STARC {
        sma_period: usize,
        atr_period: usize,
        multiplier: f64,
    },
    Supertrend {
        atr_period: usize,
        multiplier: f64,
    },
    SupertrendVolume {
        atr_period: usize,
        multiplier: f64,
        volume_lookback: usize,
        volume_threshold_pct: f64, // e.g., 1.2 = 120% of avg volume
    },
    SupertrendConfirmed {
        atr_period: usize,
        multiplier: f64,
        confirmation_bars: usize,
    },
    SupertrendAsymmetric {
        atr_period: usize,
        entry_multiplier: f64,
        exit_multiplier: f64,
    },
    SupertrendCooldown {
        atr_period: usize,
        multiplier: f64,
        cooldown_bars: usize,
    },
    // Phase 3: Price Structure
    FiftyTwoWeekHigh {
        period: usize,
        entry_pct: f64,
        exit_pct: f64,
    },
    FiftyTwoWeekHighMomentum {
        period: usize,
        entry_pct: f64,
        exit_pct: f64,
        momentum_period: usize,
        momentum_threshold: f64, // min ROC to enter (e.g., 0.0 or 0.02)
    },
    FiftyTwoWeekHighTrailing {
        period: usize,
        entry_pct: f64,
        trailing_stop_pct: f64, // e.g., 0.10 = 10% trailing stop
    },
    DarvasBox {
        box_confirmation_bars: usize,
    },
    LarryWilliams {
        range_multiplier: f64,
        atr_stop_mult: f64,
    },
    HeikinAshi {
        confirmation_bars: usize,
    },
    // Phase 4: Complex Stateful + Ensemble
    ParabolicSar {
        af_start: f64,
        af_step: f64,
        af_max: f64,
    },
    ParabolicSarFiltered {
        af_start: f64,
        af_step: f64,
        af_max: f64,
        trend_ma_period: usize, // Only enter when price > MA
    },
    ParabolicSarDelayed {
        af_start: f64,
        af_step: f64,
        af_max: f64,
        delay_bars: usize, // Wait N bars after flip
    },
    OpeningRangeBreakout {
        range_bars: usize,
        period: OpeningPeriod,
    },
    Ensemble {
        base_strategy: StrategyTypeId,
        horizons: Vec<usize>,
        voting: VotingMethod,
    },
    // Phase 5: Oscillator Strategies
    Rsi {
        period: usize,
        oversold: f64,
        overbought: f64,
    },
    Macd {
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
        entry_mode: MACDEntryMode,
    },
    Stochastic {
        k_period: usize,
        k_smooth: usize,
        d_period: usize,
        oversold: f64,
        overbought: f64,
    },
    WilliamsR {
        period: usize,
        oversold: f64,
        overbought: f64,
    },
    Cci {
        period: usize,
        entry_threshold: f64,
        exit_threshold: f64,
    },
    Roc {
        period: usize,
    },
    RsiBollinger {
        rsi_period: usize,
        rsi_oversold: f64,
        rsi_exit: f64,
        bb_period: usize,
        bb_std_mult: f64,
    },
    MacdAdx {
        fast_period: usize,
        slow_period: usize,
        signal_period: usize,
        adx_period: usize,
        adx_threshold: f64,
    },
    OscillatorConfluence {
        rsi_period: usize,
        rsi_oversold: f64,
        rsi_overbought: f64,
        stoch_k_period: usize,
        stoch_k_smooth: usize,
        stoch_d_period: usize,
        stoch_oversold: f64,
        stoch_overbought: f64,
    },
    Ichimoku {
        tenkan_period: usize,
        kijun_period: usize,
        senkou_b_period: usize,
    },
}

/// Helper to hash f64 values consistently using bit representation.
fn hash_f64<H: std::hash::Hasher>(value: f64, state: &mut H) {
    use std::hash::Hash;
    value.to_bits().hash(state);
}

impl PartialEq for StrategyConfigId {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Donchian {
                    entry_lookback: e1,
                    exit_lookback: x1,
                },
                Self::Donchian {
                    entry_lookback: e2,
                    exit_lookback: x2,
                },
            ) => e1 == e2 && x1 == x2,
            (Self::TurtleS1, Self::TurtleS1) => true,
            (Self::TurtleS2, Self::TurtleS2) => true,
            (
                Self::MACrossover {
                    fast: f1,
                    slow: s1,
                    ma_type: m1,
                },
                Self::MACrossover {
                    fast: f2,
                    slow: s2,
                    ma_type: m2,
                },
            ) => f1 == f2 && s1 == s2 && m1 == m2,
            (Self::Tsmom { lookback: l1 }, Self::Tsmom { lookback: l2 }) => l1 == l2,
            (
                Self::DmiAdx {
                    di_period: d1,
                    adx_period: a1,
                    adx_threshold: t1,
                },
                Self::DmiAdx {
                    di_period: d2,
                    adx_period: a2,
                    adx_threshold: t2,
                },
            ) => d1 == d2 && a1 == a2 && t1.to_bits() == t2.to_bits(),
            (Self::Aroon { period: p1 }, Self::Aroon { period: p2 }) => p1 == p2,
            (
                Self::BollingerSqueeze {
                    period: p1,
                    std_mult: s1,
                    squeeze_threshold: t1,
                },
                Self::BollingerSqueeze {
                    period: p2,
                    std_mult: s2,
                    squeeze_threshold: t2,
                },
            ) => p1 == p2 && s1.to_bits() == s2.to_bits() && t1.to_bits() == t2.to_bits(),
            (
                Self::Keltner {
                    ema_period: e1,
                    atr_period: a1,
                    multiplier: m1,
                },
                Self::Keltner {
                    ema_period: e2,
                    atr_period: a2,
                    multiplier: m2,
                },
            ) => e1 == e2 && a1 == a2 && m1.to_bits() == m2.to_bits(),
            (
                Self::STARC {
                    sma_period: s1,
                    atr_period: a1,
                    multiplier: m1,
                },
                Self::STARC {
                    sma_period: s2,
                    atr_period: a2,
                    multiplier: m2,
                },
            ) => s1 == s2 && a1 == a2 && m1.to_bits() == m2.to_bits(),
            (
                Self::Supertrend {
                    atr_period: a1,
                    multiplier: m1,
                },
                Self::Supertrend {
                    atr_period: a2,
                    multiplier: m2,
                },
            ) => a1 == a2 && m1.to_bits() == m2.to_bits(),
            (
                Self::SupertrendVolume {
                    atr_period: a1,
                    multiplier: m1,
                    volume_lookback: v1,
                    volume_threshold_pct: t1,
                },
                Self::SupertrendVolume {
                    atr_period: a2,
                    multiplier: m2,
                    volume_lookback: v2,
                    volume_threshold_pct: t2,
                },
            ) => {
                a1 == a2 && m1.to_bits() == m2.to_bits() && v1 == v2 && t1.to_bits() == t2.to_bits()
            }
            (
                Self::SupertrendConfirmed {
                    atr_period: a1,
                    multiplier: m1,
                    confirmation_bars: c1,
                },
                Self::SupertrendConfirmed {
                    atr_period: a2,
                    multiplier: m2,
                    confirmation_bars: c2,
                },
            ) => a1 == a2 && m1.to_bits() == m2.to_bits() && c1 == c2,
            (
                Self::SupertrendAsymmetric {
                    atr_period: a1,
                    entry_multiplier: e1,
                    exit_multiplier: x1,
                },
                Self::SupertrendAsymmetric {
                    atr_period: a2,
                    entry_multiplier: e2,
                    exit_multiplier: x2,
                },
            ) => a1 == a2 && e1.to_bits() == e2.to_bits() && x1.to_bits() == x2.to_bits(),
            (
                Self::SupertrendCooldown {
                    atr_period: a1,
                    multiplier: m1,
                    cooldown_bars: c1,
                },
                Self::SupertrendCooldown {
                    atr_period: a2,
                    multiplier: m2,
                    cooldown_bars: c2,
                },
            ) => a1 == a2 && m1.to_bits() == m2.to_bits() && c1 == c2,
            (
                Self::FiftyTwoWeekHigh {
                    period: p1,
                    entry_pct: e1,
                    exit_pct: x1,
                },
                Self::FiftyTwoWeekHigh {
                    period: p2,
                    entry_pct: e2,
                    exit_pct: x2,
                },
            ) => p1 == p2 && e1.to_bits() == e2.to_bits() && x1.to_bits() == x2.to_bits(),
            (
                Self::FiftyTwoWeekHighMomentum {
                    period: p1,
                    entry_pct: e1,
                    exit_pct: x1,
                    momentum_period: mp1,
                    momentum_threshold: mt1,
                },
                Self::FiftyTwoWeekHighMomentum {
                    period: p2,
                    entry_pct: e2,
                    exit_pct: x2,
                    momentum_period: mp2,
                    momentum_threshold: mt2,
                },
            ) => {
                p1 == p2
                    && e1.to_bits() == e2.to_bits()
                    && x1.to_bits() == x2.to_bits()
                    && mp1 == mp2
                    && mt1.to_bits() == mt2.to_bits()
            }
            (
                Self::FiftyTwoWeekHighTrailing {
                    period: p1,
                    entry_pct: e1,
                    trailing_stop_pct: t1,
                },
                Self::FiftyTwoWeekHighTrailing {
                    period: p2,
                    entry_pct: e2,
                    trailing_stop_pct: t2,
                },
            ) => p1 == p2 && e1.to_bits() == e2.to_bits() && t1.to_bits() == t2.to_bits(),
            (
                Self::DarvasBox {
                    box_confirmation_bars: b1,
                },
                Self::DarvasBox {
                    box_confirmation_bars: b2,
                },
            ) => b1 == b2,
            (
                Self::LarryWilliams {
                    range_multiplier: r1,
                    atr_stop_mult: a1,
                },
                Self::LarryWilliams {
                    range_multiplier: r2,
                    atr_stop_mult: a2,
                },
            ) => r1.to_bits() == r2.to_bits() && a1.to_bits() == a2.to_bits(),
            (
                Self::HeikinAshi {
                    confirmation_bars: c1,
                },
                Self::HeikinAshi {
                    confirmation_bars: c2,
                },
            ) => c1 == c2,
            (
                Self::ParabolicSar {
                    af_start: s1,
                    af_step: st1,
                    af_max: m1,
                },
                Self::ParabolicSar {
                    af_start: s2,
                    af_step: st2,
                    af_max: m2,
                },
            ) => {
                s1.to_bits() == s2.to_bits()
                    && st1.to_bits() == st2.to_bits()
                    && m1.to_bits() == m2.to_bits()
            }
            (
                Self::ParabolicSarFiltered {
                    af_start: s1,
                    af_step: st1,
                    af_max: m1,
                    trend_ma_period: t1,
                },
                Self::ParabolicSarFiltered {
                    af_start: s2,
                    af_step: st2,
                    af_max: m2,
                    trend_ma_period: t2,
                },
            ) => {
                s1.to_bits() == s2.to_bits()
                    && st1.to_bits() == st2.to_bits()
                    && m1.to_bits() == m2.to_bits()
                    && t1 == t2
            }
            (
                Self::ParabolicSarDelayed {
                    af_start: s1,
                    af_step: st1,
                    af_max: m1,
                    delay_bars: d1,
                },
                Self::ParabolicSarDelayed {
                    af_start: s2,
                    af_step: st2,
                    af_max: m2,
                    delay_bars: d2,
                },
            ) => {
                s1.to_bits() == s2.to_bits()
                    && st1.to_bits() == st2.to_bits()
                    && m1.to_bits() == m2.to_bits()
                    && d1 == d2
            }
            (
                Self::OpeningRangeBreakout {
                    range_bars: r1,
                    period: p1,
                },
                Self::OpeningRangeBreakout {
                    range_bars: r2,
                    period: p2,
                },
            ) => r1 == r2 && p1 == p2,
            (
                Self::Ensemble {
                    base_strategy: b1,
                    horizons: h1,
                    voting: v1,
                },
                Self::Ensemble {
                    base_strategy: b2,
                    horizons: h2,
                    voting: v2,
                },
            ) => b1 == b2 && h1 == h2 && v1 == v2,
            (
                Self::Rsi {
                    period: p1,
                    oversold: o1,
                    overbought: b1,
                },
                Self::Rsi {
                    period: p2,
                    oversold: o2,
                    overbought: b2,
                },
            ) => p1 == p2 && o1.to_bits() == o2.to_bits() && b1.to_bits() == b2.to_bits(),
            (
                Self::Macd {
                    fast_period: f1,
                    slow_period: s1,
                    signal_period: sg1,
                    entry_mode: e1,
                },
                Self::Macd {
                    fast_period: f2,
                    slow_period: s2,
                    signal_period: sg2,
                    entry_mode: e2,
                },
            ) => f1 == f2 && s1 == s2 && sg1 == sg2 && e1 == e2,
            (
                Self::Stochastic {
                    k_period: k1,
                    k_smooth: ks1,
                    d_period: d1,
                    oversold: o1,
                    overbought: b1,
                },
                Self::Stochastic {
                    k_period: k2,
                    k_smooth: ks2,
                    d_period: d2,
                    oversold: o2,
                    overbought: b2,
                },
            ) => {
                k1 == k2
                    && ks1 == ks2
                    && d1 == d2
                    && o1.to_bits() == o2.to_bits()
                    && b1.to_bits() == b2.to_bits()
            }
            (
                Self::WilliamsR {
                    period: p1,
                    oversold: o1,
                    overbought: b1,
                },
                Self::WilliamsR {
                    period: p2,
                    oversold: o2,
                    overbought: b2,
                },
            ) => p1 == p2 && o1.to_bits() == o2.to_bits() && b1.to_bits() == b2.to_bits(),
            (
                Self::Cci {
                    period: p1,
                    entry_threshold: e1,
                    exit_threshold: x1,
                },
                Self::Cci {
                    period: p2,
                    entry_threshold: e2,
                    exit_threshold: x2,
                },
            ) => p1 == p2 && e1.to_bits() == e2.to_bits() && x1.to_bits() == x2.to_bits(),
            (Self::Roc { period: p1 }, Self::Roc { period: p2 }) => p1 == p2,
            (
                Self::RsiBollinger {
                    rsi_period: rp1,
                    rsi_oversold: ro1,
                    rsi_exit: re1,
                    bb_period: bp1,
                    bb_std_mult: bm1,
                },
                Self::RsiBollinger {
                    rsi_period: rp2,
                    rsi_oversold: ro2,
                    rsi_exit: re2,
                    bb_period: bp2,
                    bb_std_mult: bm2,
                },
            ) => {
                rp1 == rp2
                    && ro1.to_bits() == ro2.to_bits()
                    && re1.to_bits() == re2.to_bits()
                    && bp1 == bp2
                    && bm1.to_bits() == bm2.to_bits()
            }
            (
                Self::MacdAdx {
                    fast_period: f1,
                    slow_period: s1,
                    signal_period: sg1,
                    adx_period: ap1,
                    adx_threshold: at1,
                },
                Self::MacdAdx {
                    fast_period: f2,
                    slow_period: s2,
                    signal_period: sg2,
                    adx_period: ap2,
                    adx_threshold: at2,
                },
            ) => f1 == f2 && s1 == s2 && sg1 == sg2 && ap1 == ap2 && at1.to_bits() == at2.to_bits(),
            (
                Self::OscillatorConfluence {
                    rsi_period: rp1,
                    rsi_oversold: ro1,
                    rsi_overbought: rb1,
                    stoch_k_period: sk1,
                    stoch_k_smooth: sks1,
                    stoch_d_period: sd1,
                    stoch_oversold: so1,
                    stoch_overbought: sb1,
                },
                Self::OscillatorConfluence {
                    rsi_period: rp2,
                    rsi_oversold: ro2,
                    rsi_overbought: rb2,
                    stoch_k_period: sk2,
                    stoch_k_smooth: sks2,
                    stoch_d_period: sd2,
                    stoch_oversold: so2,
                    stoch_overbought: sb2,
                },
            ) => {
                rp1 == rp2
                    && ro1.to_bits() == ro2.to_bits()
                    && rb1.to_bits() == rb2.to_bits()
                    && sk1 == sk2
                    && sks1 == sks2
                    && sd1 == sd2
                    && so1.to_bits() == so2.to_bits()
                    && sb1.to_bits() == sb2.to_bits()
            }
            (
                Self::Ichimoku {
                    tenkan_period: t1,
                    kijun_period: k1,
                    senkou_b_period: s1,
                },
                Self::Ichimoku {
                    tenkan_period: t2,
                    kijun_period: k2,
                    senkou_b_period: s2,
                },
            ) => t1 == t2 && k1 == k2 && s1 == s2,
            _ => false,
        }
    }
}

impl Eq for StrategyConfigId {}

impl std::hash::Hash for StrategyConfigId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash the discriminant first
        std::mem::discriminant(self).hash(state);

        match self {
            Self::Donchian {
                entry_lookback,
                exit_lookback,
            } => {
                entry_lookback.hash(state);
                exit_lookback.hash(state);
            }
            Self::TurtleS1 | Self::TurtleS2 => {}
            Self::MACrossover {
                fast,
                slow,
                ma_type,
            } => {
                fast.hash(state);
                slow.hash(state);
                ma_type.hash(state);
            }
            Self::Tsmom { lookback } => lookback.hash(state),
            Self::DmiAdx {
                di_period,
                adx_period,
                adx_threshold,
            } => {
                di_period.hash(state);
                adx_period.hash(state);
                hash_f64(*adx_threshold, state);
            }
            Self::Aroon { period } => period.hash(state),
            Self::BollingerSqueeze {
                period,
                std_mult,
                squeeze_threshold,
            } => {
                period.hash(state);
                hash_f64(*std_mult, state);
                hash_f64(*squeeze_threshold, state);
            }
            Self::Keltner {
                ema_period,
                atr_period,
                multiplier,
            } => {
                ema_period.hash(state);
                atr_period.hash(state);
                hash_f64(*multiplier, state);
            }
            Self::STARC {
                sma_period,
                atr_period,
                multiplier,
            } => {
                sma_period.hash(state);
                atr_period.hash(state);
                hash_f64(*multiplier, state);
            }
            Self::Supertrend {
                atr_period,
                multiplier,
            } => {
                atr_period.hash(state);
                hash_f64(*multiplier, state);
            }
            Self::SupertrendVolume {
                atr_period,
                multiplier,
                volume_lookback,
                volume_threshold_pct,
            } => {
                atr_period.hash(state);
                hash_f64(*multiplier, state);
                volume_lookback.hash(state);
                hash_f64(*volume_threshold_pct, state);
            }
            Self::SupertrendConfirmed {
                atr_period,
                multiplier,
                confirmation_bars,
            } => {
                atr_period.hash(state);
                hash_f64(*multiplier, state);
                confirmation_bars.hash(state);
            }
            Self::SupertrendAsymmetric {
                atr_period,
                entry_multiplier,
                exit_multiplier,
            } => {
                atr_period.hash(state);
                hash_f64(*entry_multiplier, state);
                hash_f64(*exit_multiplier, state);
            }
            Self::SupertrendCooldown {
                atr_period,
                multiplier,
                cooldown_bars,
            } => {
                atr_period.hash(state);
                hash_f64(*multiplier, state);
                cooldown_bars.hash(state);
            }
            Self::FiftyTwoWeekHigh {
                period,
                entry_pct,
                exit_pct,
            } => {
                period.hash(state);
                hash_f64(*entry_pct, state);
                hash_f64(*exit_pct, state);
            }
            Self::FiftyTwoWeekHighMomentum {
                period,
                entry_pct,
                exit_pct,
                momentum_period,
                momentum_threshold,
            } => {
                period.hash(state);
                hash_f64(*entry_pct, state);
                hash_f64(*exit_pct, state);
                momentum_period.hash(state);
                hash_f64(*momentum_threshold, state);
            }
            Self::FiftyTwoWeekHighTrailing {
                period,
                entry_pct,
                trailing_stop_pct,
            } => {
                period.hash(state);
                hash_f64(*entry_pct, state);
                hash_f64(*trailing_stop_pct, state);
            }
            Self::DarvasBox {
                box_confirmation_bars,
            } => box_confirmation_bars.hash(state),
            Self::LarryWilliams {
                range_multiplier,
                atr_stop_mult,
            } => {
                hash_f64(*range_multiplier, state);
                hash_f64(*atr_stop_mult, state);
            }
            Self::HeikinAshi { confirmation_bars } => confirmation_bars.hash(state),
            Self::ParabolicSar {
                af_start,
                af_step,
                af_max,
            } => {
                hash_f64(*af_start, state);
                hash_f64(*af_step, state);
                hash_f64(*af_max, state);
            }
            Self::ParabolicSarFiltered {
                af_start,
                af_step,
                af_max,
                trend_ma_period,
            } => {
                hash_f64(*af_start, state);
                hash_f64(*af_step, state);
                hash_f64(*af_max, state);
                trend_ma_period.hash(state);
            }
            Self::ParabolicSarDelayed {
                af_start,
                af_step,
                af_max,
                delay_bars,
            } => {
                hash_f64(*af_start, state);
                hash_f64(*af_step, state);
                hash_f64(*af_max, state);
                delay_bars.hash(state);
            }
            Self::OpeningRangeBreakout { range_bars, period } => {
                range_bars.hash(state);
                period.hash(state);
            }
            Self::Ensemble {
                base_strategy,
                horizons,
                voting,
            } => {
                base_strategy.hash(state);
                horizons.hash(state);
                voting.hash(state);
            }
            Self::Rsi {
                period,
                oversold,
                overbought,
            } => {
                period.hash(state);
                hash_f64(*oversold, state);
                hash_f64(*overbought, state);
            }
            Self::Macd {
                fast_period,
                slow_period,
                signal_period,
                entry_mode,
            } => {
                fast_period.hash(state);
                slow_period.hash(state);
                signal_period.hash(state);
                entry_mode.hash(state);
            }
            Self::Stochastic {
                k_period,
                k_smooth,
                d_period,
                oversold,
                overbought,
            } => {
                k_period.hash(state);
                k_smooth.hash(state);
                d_period.hash(state);
                hash_f64(*oversold, state);
                hash_f64(*overbought, state);
            }
            Self::WilliamsR {
                period,
                oversold,
                overbought,
            } => {
                period.hash(state);
                hash_f64(*oversold, state);
                hash_f64(*overbought, state);
            }
            Self::Cci {
                period,
                entry_threshold,
                exit_threshold,
            } => {
                period.hash(state);
                hash_f64(*entry_threshold, state);
                hash_f64(*exit_threshold, state);
            }
            Self::Roc { period } => period.hash(state),
            Self::RsiBollinger {
                rsi_period,
                rsi_oversold,
                rsi_exit,
                bb_period,
                bb_std_mult,
            } => {
                rsi_period.hash(state);
                hash_f64(*rsi_oversold, state);
                hash_f64(*rsi_exit, state);
                bb_period.hash(state);
                hash_f64(*bb_std_mult, state);
            }
            Self::MacdAdx {
                fast_period,
                slow_period,
                signal_period,
                adx_period,
                adx_threshold,
            } => {
                fast_period.hash(state);
                slow_period.hash(state);
                signal_period.hash(state);
                adx_period.hash(state);
                hash_f64(*adx_threshold, state);
            }
            Self::OscillatorConfluence {
                rsi_period,
                rsi_oversold,
                rsi_overbought,
                stoch_k_period,
                stoch_k_smooth,
                stoch_d_period,
                stoch_oversold,
                stoch_overbought,
            } => {
                rsi_period.hash(state);
                hash_f64(*rsi_oversold, state);
                hash_f64(*rsi_overbought, state);
                stoch_k_period.hash(state);
                stoch_k_smooth.hash(state);
                stoch_d_period.hash(state);
                hash_f64(*stoch_oversold, state);
                hash_f64(*stoch_overbought, state);
            }
            Self::Ichimoku {
                tenkan_period,
                kijun_period,
                senkou_b_period,
            } => {
                tenkan_period.hash(state);
                kijun_period.hash(state);
                senkou_b_period.hash(state);
            }
        }
    }
}

impl StrategyConfigId {
    /// Get the strategy type for this config.
    pub fn strategy_type(&self) -> StrategyTypeId {
        match self {
            Self::Donchian { .. } => StrategyTypeId::Donchian,
            Self::TurtleS1 => StrategyTypeId::TurtleS1,
            Self::TurtleS2 => StrategyTypeId::TurtleS2,
            Self::MACrossover { .. } => StrategyTypeId::MACrossover,
            Self::Tsmom { .. } => StrategyTypeId::Tsmom,
            Self::DmiAdx { .. } => StrategyTypeId::DmiAdx,
            Self::Aroon { .. } => StrategyTypeId::Aroon,
            Self::BollingerSqueeze { .. } => StrategyTypeId::BollingerSqueeze,
            // Phase 1
            Self::Keltner { .. } => StrategyTypeId::Keltner,
            Self::STARC { .. } => StrategyTypeId::STARC,
            Self::Supertrend { .. } => StrategyTypeId::Supertrend,
            Self::SupertrendVolume { .. } => StrategyTypeId::SupertrendVolume,
            Self::SupertrendConfirmed { .. } => StrategyTypeId::SupertrendConfirmed,
            Self::SupertrendAsymmetric { .. } => StrategyTypeId::SupertrendAsymmetric,
            Self::SupertrendCooldown { .. } => StrategyTypeId::SupertrendCooldown,
            // Phase 3
            Self::FiftyTwoWeekHigh { .. } => StrategyTypeId::FiftyTwoWeekHigh,
            Self::FiftyTwoWeekHighMomentum { .. } => StrategyTypeId::FiftyTwoWeekHighMomentum,
            Self::FiftyTwoWeekHighTrailing { .. } => StrategyTypeId::FiftyTwoWeekHighTrailing,
            Self::DarvasBox { .. } => StrategyTypeId::DarvasBox,
            Self::LarryWilliams { .. } => StrategyTypeId::LarryWilliams,
            Self::HeikinAshi { .. } => StrategyTypeId::HeikinAshi,
            // Phase 4
            Self::ParabolicSar { .. } => StrategyTypeId::ParabolicSar,
            Self::ParabolicSarFiltered { .. } => StrategyTypeId::ParabolicSarFiltered,
            Self::ParabolicSarDelayed { .. } => StrategyTypeId::ParabolicSarDelayed,
            Self::OpeningRangeBreakout { .. } => StrategyTypeId::OpeningRangeBreakout,
            Self::Ensemble { .. } => StrategyTypeId::Ensemble,
            // Phase 5: Oscillator Strategies
            Self::Rsi { .. } => StrategyTypeId::Rsi,
            Self::Macd { .. } => StrategyTypeId::Macd,
            Self::Stochastic { .. } => StrategyTypeId::Stochastic,
            Self::WilliamsR { .. } => StrategyTypeId::WilliamsR,
            Self::Cci { .. } => StrategyTypeId::Cci,
            Self::Roc { .. } => StrategyTypeId::Roc,
            Self::RsiBollinger { .. } => StrategyTypeId::RsiBollinger,
            Self::MacdAdx { .. } => StrategyTypeId::MacdAdx,
            Self::OscillatorConfluence { .. } => StrategyTypeId::OscillatorConfluence,
            Self::Ichimoku { .. } => StrategyTypeId::Ichimoku,
        }
    }

    /// Get display string for this config.
    pub fn display(&self) -> String {
        match self {
            Self::Donchian {
                entry_lookback,
                exit_lookback,
            } => format!("Donchian {}/{}", entry_lookback, exit_lookback),
            Self::TurtleS1 => "Turtle S1 20/10".to_string(),
            Self::TurtleS2 => "Turtle S2 55/20".to_string(),
            Self::MACrossover {
                fast,
                slow,
                ma_type,
            } => {
                format!("MA {} {}/{}", ma_type.name(), fast, slow)
            }
            Self::Tsmom { lookback } => format!("TSMOM {}", lookback),
            Self::DmiAdx {
                di_period,
                adx_period,
                adx_threshold,
            } => format!("DMI/ADX {}/{}/{:.0}", di_period, adx_period, adx_threshold),
            Self::Aroon { period } => format!("Aroon {}", period),
            Self::BollingerSqueeze {
                period,
                std_mult,
                squeeze_threshold,
            } => format!(
                "BB Squeeze {}/{:.1}/{:.2}",
                period, std_mult, squeeze_threshold
            ),
            // Phase 1: ATR-Based Channels
            Self::Keltner {
                ema_period,
                atr_period,
                multiplier,
            } => format!("Keltner {}/{}/{:.1}", ema_period, atr_period, multiplier),
            Self::STARC {
                sma_period,
                atr_period,
                multiplier,
            } => format!("STARC {}/{}/{:.1}", sma_period, atr_period, multiplier),
            Self::Supertrend {
                atr_period,
                multiplier,
            } => format!("Supertrend {}/{:.1}", atr_period, multiplier),
            Self::SupertrendVolume {
                atr_period,
                multiplier,
                volume_lookback,
                volume_threshold_pct,
            } => format!(
                "ST+Vol {}/{:.1}/{}/{:.0}%",
                atr_period,
                multiplier,
                volume_lookback,
                volume_threshold_pct * 100.0
            ),
            Self::SupertrendConfirmed {
                atr_period,
                multiplier,
                confirmation_bars,
            } => format!(
                "ST Confirmed {}/{:.1}/{}",
                atr_period, multiplier, confirmation_bars
            ),
            Self::SupertrendAsymmetric {
                atr_period,
                entry_multiplier,
                exit_multiplier,
            } => format!(
                "ST Asym {}/{:.1}/{:.1}",
                atr_period, entry_multiplier, exit_multiplier
            ),
            Self::SupertrendCooldown {
                atr_period,
                multiplier,
                cooldown_bars,
            } => format!(
                "ST Cooldown {}/{:.1}/{}",
                atr_period, multiplier, cooldown_bars
            ),
            // Phase 3: Price Structure
            Self::FiftyTwoWeekHigh {
                period,
                entry_pct,
                exit_pct,
            } => format!(
                "52wk High {}/{:.0}%/{:.0}%",
                period,
                entry_pct * 100.0,
                exit_pct * 100.0
            ),
            Self::FiftyTwoWeekHighMomentum {
                period,
                entry_pct,
                exit_pct,
                momentum_period,
                momentum_threshold,
            } => format!(
                "52wk Mom {}/{:.0}%/{:.0}%/{}/{:.0}%",
                period,
                entry_pct * 100.0,
                exit_pct * 100.0,
                momentum_period,
                momentum_threshold * 100.0
            ),
            Self::FiftyTwoWeekHighTrailing {
                period,
                entry_pct,
                trailing_stop_pct,
            } => format!(
                "52wk Trail {}/{:.0}%/{:.0}%",
                period,
                entry_pct * 100.0,
                trailing_stop_pct * 100.0
            ),
            Self::DarvasBox {
                box_confirmation_bars,
            } => {
                format!("Darvas Box {}", box_confirmation_bars)
            }
            Self::LarryWilliams {
                range_multiplier,
                atr_stop_mult,
            } => format!("Williams {:.2}/{:.1}", range_multiplier, atr_stop_mult),
            Self::HeikinAshi { confirmation_bars } => {
                format!("Heikin-Ashi {}", confirmation_bars)
            }
            // Phase 4
            Self::ParabolicSar {
                af_start,
                af_step,
                af_max,
            } => format!("SAR {:.2}/{:.2}/{:.2}", af_start, af_step, af_max),
            Self::ParabolicSarFiltered {
                af_start,
                af_step,
                af_max,
                trend_ma_period,
            } => format!(
                "SAR Filt {:.2}/{:.2}/{:.2}/MA{}",
                af_start, af_step, af_max, trend_ma_period
            ),
            Self::ParabolicSarDelayed {
                af_start,
                af_step,
                af_max,
                delay_bars,
            } => format!(
                "SAR Delay {:.2}/{:.2}/{:.2}/{}",
                af_start, af_step, af_max, delay_bars
            ),
            Self::OpeningRangeBreakout { range_bars, period } => {
                format!("ORB {} {:?}", range_bars, period)
            }
            Self::Ensemble {
                base_strategy,
                horizons,
                voting,
            } => format!(
                "Ensemble {} {:?} {:?}",
                base_strategy.name(),
                horizons,
                voting
            ),
            // Phase 5: Oscillator Strategies
            Self::Rsi {
                period,
                oversold,
                overbought,
            } => format!("RSI {}/{:.0}/{:.0}", period, oversold, overbought),
            Self::Macd {
                fast_period,
                slow_period,
                signal_period,
                entry_mode,
            } => format!(
                "MACD {}/{}/{} {:?}",
                fast_period, slow_period, signal_period, entry_mode
            ),
            Self::Stochastic {
                k_period,
                k_smooth,
                d_period,
                oversold,
                overbought,
            } => format!(
                "Stoch {}/{}/{}/{:.0}/{:.0}",
                k_period, k_smooth, d_period, oversold, overbought
            ),
            Self::WilliamsR {
                period,
                oversold,
                overbought,
            } => format!("Williams %R {}/{:.0}/{:.0}", period, oversold, overbought),
            Self::Cci {
                period,
                entry_threshold,
                exit_threshold,
            } => format!(
                "CCI {}/{:.0}/{:.0}",
                period, entry_threshold, exit_threshold
            ),
            Self::Roc { period } => format!("ROC {}", period),
            Self::RsiBollinger {
                rsi_period,
                rsi_oversold,
                rsi_exit,
                bb_period,
                bb_std_mult,
            } => format!(
                "RSI+BB {}/{:.0}/{:.0}/{}/{:.1}",
                rsi_period, rsi_oversold, rsi_exit, bb_period, bb_std_mult
            ),
            Self::MacdAdx {
                fast_period,
                slow_period,
                signal_period,
                adx_period,
                adx_threshold,
            } => format!(
                "MACD+ADX {}/{}/{}/{}/{:.0}",
                fast_period, slow_period, signal_period, adx_period, adx_threshold
            ),
            Self::OscillatorConfluence {
                rsi_period,
                rsi_oversold,
                rsi_overbought,
                stoch_k_period,
                stoch_k_smooth,
                stoch_d_period,
                stoch_oversold,
                stoch_overbought,
            } => format!(
                "Confluence RSI{}/{:.0}/{:.0} Stoch{}/{}/{}/{:.0}/{:.0}",
                rsi_period,
                rsi_oversold,
                rsi_overbought,
                stoch_k_period,
                stoch_k_smooth,
                stoch_d_period,
                stoch_oversold,
                stoch_overbought
            ),
            Self::Ichimoku {
                tenkan_period,
                kijun_period,
                senkou_b_period,
            } => format!(
                "Ichimoku {}/{}/{}",
                tenkan_period, kijun_period, senkou_b_period
            ),
        }
    }

    /// Get a filename-safe identifier for this config (for artifact exports).
    pub fn file_id(&self) -> String {
        match self {
            Self::Donchian {
                entry_lookback,
                exit_lookback,
            } => format!("{}_{}", entry_lookback, exit_lookback),
            Self::TurtleS1 => "20_10".to_string(),
            Self::TurtleS2 => "55_20".to_string(),
            Self::MACrossover {
                fast,
                slow,
                ma_type,
            } => {
                format!("{}_{}_{}", fast, slow, ma_type.name())
            }
            Self::Tsmom { lookback } => format!("{}", lookback),
            Self::DmiAdx {
                di_period,
                adx_period,
                adx_threshold,
            } => format!("{}_{}_{:.0}", di_period, adx_period, adx_threshold),
            Self::Aroon { period } => format!("{}", period),
            Self::BollingerSqueeze {
                period,
                std_mult,
                squeeze_threshold,
            } => format!("{}_{:.1}_{:.0}", period, std_mult, squeeze_threshold),
            Self::Keltner {
                ema_period,
                atr_period,
                multiplier,
            } => format!("{}_{}_{:.1}", ema_period, atr_period, multiplier),
            Self::STARC {
                sma_period,
                atr_period,
                multiplier,
            } => format!("{}_{}_{:.1}", sma_period, atr_period, multiplier),
            Self::Supertrend {
                atr_period,
                multiplier,
            } => format!("{}_{:.1}", atr_period, multiplier),
            Self::SupertrendVolume {
                atr_period,
                multiplier,
                volume_lookback,
                volume_threshold_pct,
            } => format!(
                "{}_{:.1}_{}_{:.0}",
                atr_period,
                multiplier,
                volume_lookback,
                volume_threshold_pct * 100.0
            ),
            Self::SupertrendConfirmed {
                atr_period,
                multiplier,
                confirmation_bars,
            } => format!("{}_{:.1}_{}", atr_period, multiplier, confirmation_bars),
            Self::SupertrendAsymmetric {
                atr_period,
                entry_multiplier,
                exit_multiplier,
            } => format!(
                "{}_{:.1}_{:.1}",
                atr_period, entry_multiplier, exit_multiplier
            ),
            Self::SupertrendCooldown {
                atr_period,
                multiplier,
                cooldown_bars,
            } => format!("{}_{:.1}_{}", atr_period, multiplier, cooldown_bars),
            Self::FiftyTwoWeekHigh {
                period,
                entry_pct,
                exit_pct,
            } => format!(
                "{}_{:.0}_{:.0}",
                period,
                entry_pct * 100.0,
                exit_pct * 100.0
            ),
            Self::FiftyTwoWeekHighMomentum {
                period,
                entry_pct,
                exit_pct,
                momentum_period,
                momentum_threshold,
            } => format!(
                "{}_{:.0}_{:.0}_{}_{:.0}",
                period,
                entry_pct * 100.0,
                exit_pct * 100.0,
                momentum_period,
                momentum_threshold * 100.0
            ),
            Self::FiftyTwoWeekHighTrailing {
                period,
                entry_pct,
                trailing_stop_pct,
            } => format!(
                "{}_{:.0}_{:.0}",
                period,
                entry_pct * 100.0,
                trailing_stop_pct * 100.0
            ),
            Self::DarvasBox {
                box_confirmation_bars,
            } => format!("{}", box_confirmation_bars),
            Self::LarryWilliams {
                range_multiplier,
                atr_stop_mult,
            } => format!("{:.1}_{:.1}", range_multiplier, atr_stop_mult),
            Self::HeikinAshi { confirmation_bars } => format!("{}", confirmation_bars),
            Self::ParabolicSar {
                af_start,
                af_step,
                af_max,
            } => format!("{:.2}_{:.2}_{:.2}", af_start, af_step, af_max),
            Self::ParabolicSarFiltered {
                af_start,
                af_step,
                af_max,
                trend_ma_period,
            } => format!(
                "{:.2}_{:.2}_{:.2}_{}",
                af_start, af_step, af_max, trend_ma_period
            ),
            Self::ParabolicSarDelayed {
                af_start,
                af_step,
                af_max,
                delay_bars,
            } => format!(
                "{:.2}_{:.2}_{:.2}_{}",
                af_start, af_step, af_max, delay_bars
            ),
            Self::OpeningRangeBreakout { range_bars, period } => {
                format!("{}__{:?}", range_bars, period)
            }
            Self::Ensemble {
                base_strategy,
                horizons,
                voting,
            } => format!(
                "{}_{}_{:?}",
                base_strategy.id(),
                horizons
                    .iter()
                    .map(|h| h.to_string())
                    .collect::<Vec<_>>()
                    .join("_"),
                voting
            ),
            // Phase 5: Oscillator Strategies
            Self::Rsi {
                period,
                oversold,
                overbought,
            } => format!("{}_{:.0}_{:.0}", period, oversold, overbought),
            Self::Macd {
                fast_period,
                slow_period,
                signal_period,
                entry_mode,
            } => format!(
                "{}_{}_{}_{:?}",
                fast_period, slow_period, signal_period, entry_mode
            ),
            Self::Stochastic {
                k_period,
                k_smooth,
                d_period,
                oversold,
                overbought,
            } => format!(
                "{}_{}_{}_{:.0}_{:.0}",
                k_period, k_smooth, d_period, oversold, overbought
            ),
            Self::WilliamsR {
                period,
                oversold,
                overbought,
            } => format!("{}_{:.0}_{:.0}", period, oversold, overbought),
            Self::Cci {
                period,
                entry_threshold,
                exit_threshold,
            } => format!("{}_{:.0}_{:.0}", period, entry_threshold, exit_threshold),
            Self::Roc { period } => format!("{}", period),
            Self::RsiBollinger {
                rsi_period,
                rsi_oversold,
                rsi_exit,
                bb_period,
                bb_std_mult,
            } => format!(
                "{}_{:.0}_{:.0}_{}_{:.1}",
                rsi_period, rsi_oversold, rsi_exit, bb_period, bb_std_mult
            ),
            Self::MacdAdx {
                fast_period,
                slow_period,
                signal_period,
                adx_period,
                adx_threshold,
            } => format!(
                "{}_{}_{}_{}_{:.0}",
                fast_period, slow_period, signal_period, adx_period, adx_threshold
            ),
            Self::OscillatorConfluence {
                rsi_period,
                rsi_oversold,
                rsi_overbought,
                stoch_k_period,
                stoch_k_smooth,
                stoch_d_period,
                stoch_oversold,
                stoch_overbought,
            } => format!(
                "rsi{}_{:.0}_{:.0}_stoch{}_{}_{}_{}_{:.0}",
                rsi_period,
                rsi_oversold,
                rsi_overbought,
                stoch_k_period,
                stoch_k_smooth,
                stoch_d_period,
                stoch_oversold,
                stoch_overbought
            ),
            Self::Ichimoku {
                tenkan_period,
                kijun_period,
                senkou_b_period,
            } => format!("{}_{}_{}", tenkan_period, kijun_period, senkou_b_period),
        }
    }

    /// Convert to legacy ConfigId for backwards compatibility (only for Donchian types).
    pub fn to_legacy_config_id(&self) -> ConfigId {
        match self {
            Self::Donchian {
                entry_lookback,
                exit_lookback,
            } => ConfigId::new(*entry_lookback, *exit_lookback),
            Self::TurtleS1 => ConfigId::new(20, 10),
            Self::TurtleS2 => ConfigId::new(55, 20),
            Self::MACrossover { fast, slow, .. } => ConfigId::new(*fast, *slow),
            Self::Tsmom { lookback } => ConfigId::new(*lookback, 0),
            Self::DmiAdx {
                di_period,
                adx_period,
                ..
            } => ConfigId::new(*di_period, *adx_period),
            Self::Aroon { period } => ConfigId::new(*period, 0),
            Self::BollingerSqueeze { period, .. } => ConfigId::new(*period, 0),
            // Phase 1: Pack params into legacy format
            Self::Keltner {
                ema_period,
                atr_period,
                ..
            } => ConfigId::new(*ema_period, *atr_period),
            Self::STARC {
                sma_period,
                atr_period,
                ..
            } => ConfigId::new(*sma_period, *atr_period),
            Self::Supertrend { atr_period, .. } => ConfigId::new(*atr_period, 0),
            Self::SupertrendVolume { atr_period, .. } => ConfigId::new(*atr_period, 0),
            Self::SupertrendConfirmed { atr_period, .. } => ConfigId::new(*atr_period, 0),
            Self::SupertrendAsymmetric { atr_period, .. } => ConfigId::new(*atr_period, 0),
            Self::SupertrendCooldown { atr_period, .. } => ConfigId::new(*atr_period, 0),
            // Phase 3
            Self::FiftyTwoWeekHigh { period, .. } => ConfigId::new(*period, 0),
            Self::FiftyTwoWeekHighMomentum { period, .. } => ConfigId::new(*period, 0),
            Self::FiftyTwoWeekHighTrailing { period, .. } => ConfigId::new(*period, 0),
            Self::DarvasBox {
                box_confirmation_bars,
            } => ConfigId::new(*box_confirmation_bars, 0),
            Self::LarryWilliams {
                range_multiplier, ..
            } => ConfigId::new((*range_multiplier * 100.0) as usize, 0),
            Self::HeikinAshi { confirmation_bars } => ConfigId::new(*confirmation_bars, 0),
            // Phase 4: Pack params into legacy format
            Self::ParabolicSar { af_start, .. } => {
                // Store af_start * 100 as entry_lookback
                ConfigId::new((*af_start * 100.0) as usize, 0)
            }
            Self::ParabolicSarFiltered { af_start, .. } => {
                ConfigId::new((*af_start * 100.0) as usize, 0)
            }
            Self::ParabolicSarDelayed { af_start, .. } => {
                ConfigId::new((*af_start * 100.0) as usize, 0)
            }
            Self::OpeningRangeBreakout { range_bars, .. } => ConfigId::new(*range_bars, 0),
            Self::Ensemble { horizons, .. } => {
                ConfigId::new(horizons.len(), horizons.first().copied().unwrap_or(0))
            }
            // Phase 5: Pack params into legacy format
            Self::Rsi { period, .. } => ConfigId::new(*period, 0),
            Self::Macd {
                fast_period,
                slow_period,
                ..
            } => ConfigId::new(*fast_period, *slow_period),
            Self::Stochastic {
                k_period, d_period, ..
            } => ConfigId::new(*k_period, *d_period),
            Self::WilliamsR { period, .. } => ConfigId::new(*period, 0),
            Self::Cci { period, .. } => ConfigId::new(*period, 0),
            Self::Roc { period } => ConfigId::new(*period, 0),
            Self::RsiBollinger {
                rsi_period,
                bb_period,
                ..
            } => ConfigId::new(*rsi_period, *bb_period),
            Self::MacdAdx {
                fast_period,
                adx_period,
                ..
            } => ConfigId::new(*fast_period, *adx_period),
            Self::OscillatorConfluence {
                rsi_period,
                stoch_k_period,
                ..
            } => ConfigId::new(*rsi_period, *stoch_k_period),
            Self::Ichimoku {
                tenkan_period,
                kijun_period,
                ..
            } => ConfigId::new(*tenkan_period, *kijun_period),
        }
    }

    /// Generate Pine Script v6 for this strategy configuration.
    ///
    /// Returns a complete, ready-to-use TradingView Pine Script.
    pub fn to_pine_script(
        &self,
        avg_sharpe: Option<f64>,
        hit_rate: Option<f64>,
        symbol_count: Option<usize>,
    ) -> String {
        let strategy_name = self.strategy_type().name();
        let config_display = self.display();

        // Performance comment
        let perf_comment = match (avg_sharpe, hit_rate, symbol_count) {
            (Some(sharpe), Some(hit), Some(count)) => {
                format!(
                    "// PERFORMANCE: Avg Sharpe {:.3}, Hit Rate {:.1}%, {} symbols\n",
                    sharpe,
                    hit * 100.0,
                    count
                )
            }
            _ => String::new(),
        };

        match self {
            Self::Supertrend {
                atr_period,
                multiplier,
            } => {
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Supertrend
// CONFIG: atr_period={2}, multiplier={3:.1}
// SOURCE: TrendLab YOLO sweep session
{4}// ============================================================================

// === INPUTS ===
int atrPeriodInput = input.int({2}, "ATR Period", minval=1, tooltip="ATR lookback period")
float multiplierInput = input.float({3:.1}, "Multiplier", minval=0.1, step=0.1, tooltip="ATR multiplier for bands")

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
[supertrendLine, supertrendDir] = ta.supertrend(multiplierInput, atrPeriodInput)

// === SIGNALS ===
// Supertrend direction: -1 = bullish (price above), 1 = bearish (price below)
bool entryCondition = supertrendDir == -1 and supertrendDir[1] == 1  // Flip to bullish
bool exitCondition = supertrendDir == 1 and supertrendDir[1] == -1   // Flip to bearish

// === STRATEGY EXECUTION ===
// Enter on bullish flip, exit on bearish flip
if supertrendDir == -1 and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if supertrendDir == 1 and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(supertrendLine, "Supertrend", color=supertrendDir == -1 ? color.green : color.red, linewidth=2)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)

// Entry/Exit markers
plotshape(supertrendDir == -1 and supertrendDir[1] == 1, "Entry", shape.triangleup, location.belowbar, color.green, size=size.small)
plotshape(supertrendDir == 1 and supertrendDir[1] == -1, "Exit", shape.triangledown, location.abovebar, color.red, size=size.small)
"#,
                    strategy_name, config_display, atr_period, multiplier, perf_comment
                )
            }

            Self::FiftyTwoWeekHigh {
                period,
                entry_pct,
                exit_pct,
            } => {
                let entry_pct_display = entry_pct * 100.0;
                let exit_pct_display = exit_pct * 100.0;
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: 52-Week High Breakout
// CONFIG: period={2}, entry_pct={3:.0}%, exit_pct={4:.0}%
// SOURCE: TrendLab YOLO sweep session
{5}// ============================================================================

// === INPUTS ===
int periodInput = input.int({2}, "Period (lookback days)", minval=1, tooltip="Lookback period for computing rolling high")
float entryPctInput = input.float({6:.2}, "Entry % of high", minval=0.01, maxval=1.0, step=0.01, tooltip="Enter when close >= this % of period high")
float exitPctInput = input.float({7:.2}, "Exit % of high", minval=0.01, maxval=1.0, step=0.01, tooltip="Exit when close < this % of period high")

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float periodHigh = ta.highest(high, periodInput)
float entryThreshold = periodHigh * entryPctInput
float exitThreshold = periodHigh * exitPctInput

// === SIGNALS ===
bool entryCondition = close >= entryThreshold
bool exitCondition = close < exitThreshold

// === STRATEGY EXECUTION ===
// Long-only: enter when price is strong (near highs), exit when weakness shows
if entryCondition and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if exitCondition and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(periodHigh, "Period High", color=color.blue, linewidth=1)
plot(entryThreshold, "Entry Threshold ({3:.0}%)", color=color.green, linewidth=1, style=plot.style_stepline)
plot(exitThreshold, "Exit Threshold ({4:.0}%)", color=color.red, linewidth=1, style=plot.style_stepline)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)

// Entry/Exit markers
plotshape(entryCondition and strategy.position_size[1] == 0, "Entry", shape.triangleup, location.belowbar, color.green, size=size.small)
plotshape(exitCondition and strategy.position_size[1] > 0, "Exit", shape.triangledown, location.abovebar, color.red, size=size.small)
"#,
                    strategy_name,
                    config_display,
                    period,
                    entry_pct_display,
                    exit_pct_display,
                    perf_comment,
                    entry_pct,
                    exit_pct
                )
            }

            Self::ParabolicSar {
                af_start,
                af_step,
                af_max,
            } => {
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Parabolic SAR
// CONFIG: af_start={2:.2}, af_step={3:.2}, af_max={4:.2}
// SOURCE: TrendLab YOLO sweep session
{5}// ============================================================================

// === INPUTS ===
float afStartInput = input.float({2:.2}, "AF Start", minval=0.01, step=0.01, tooltip="Initial acceleration factor")
float afStepInput = input.float({3:.2}, "AF Step", minval=0.01, step=0.01, tooltip="AF increment")
float afMaxInput = input.float({4:.2}, "AF Max", minval=0.1, step=0.01, tooltip="Maximum acceleration factor")

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float sarValue = ta.sar(afStartInput, afStepInput, afMaxInput)

// === SIGNALS ===
bool isBullish = close > sarValue
bool entryCondition = isBullish and not isBullish[1]  // SAR flips below price
bool exitCondition = not isBullish and isBullish[1]    // SAR flips above price

// === STRATEGY EXECUTION ===
if isBullish and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if not isBullish and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(sarValue, "SAR", style=plot.style_circles, color=isBullish ? color.green : color.red, linewidth=1)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)

// Entry/Exit markers
plotshape(entryCondition, "Entry", shape.triangleup, location.belowbar, color.green, size=size.small)
plotshape(exitCondition, "Exit", shape.triangledown, location.abovebar, color.red, size=size.small)
"#,
                    strategy_name, config_display, af_start, af_step, af_max, perf_comment
                )
            }

            Self::LarryWilliams {
                range_multiplier,
                atr_stop_mult,
            } => {
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Larry Williams Volatility Breakout
// CONFIG: range_multiplier={2:.2}, atr_stop_mult={3:.1}
// SOURCE: TrendLab YOLO sweep session
{4}// ============================================================================

// === INPUTS ===
float rangeMultInput = input.float({2:.2}, "Range Multiplier", minval=0.1, step=0.1, tooltip="Multiplier for previous day range")
float atrStopMultInput = input.float({3:.1}, "ATR Stop Multiplier", minval=0.5, step=0.1, tooltip="ATR multiplier for stop loss")
int atrPeriod = input.int(14, "ATR Period", minval=1)

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float prevRange = high[1] - low[1]
float entryLevel = open + prevRange * rangeMultInput
float atrValue = ta.atr(atrPeriod)
float stopLevel = close - atrValue * atrStopMultInput

// === SIGNALS ===
bool entryCondition = high >= entryLevel and close > open
bool exitCondition = close < stopLevel

// === STRATEGY EXECUTION ===
if entryCondition and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if exitCondition and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(entryLevel, "Entry Level", color=color.green, linewidth=1, style=plot.style_stepline)
plot(strategy.position_size > 0 ? stopLevel : na, "Stop Level", color=color.red, linewidth=1, style=plot.style_stepline)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)

// Entry/Exit markers
plotshape(entryCondition and strategy.position_size[1] == 0, "Entry", shape.triangleup, location.belowbar, color.green, size=size.small)
plotshape(exitCondition and strategy.position_size[1] > 0, "Exit", shape.triangledown, location.abovebar, color.red, size=size.small)
"#,
                    strategy_name, config_display, range_multiplier, atr_stop_mult, perf_comment
                )
            }

            Self::STARC {
                sma_period,
                atr_period,
                multiplier,
            } => {
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: STARC Bands
// CONFIG: sma_period={2}, atr_period={3}, multiplier={4:.1}
// SOURCE: TrendLab YOLO sweep session
{5}// ============================================================================

// === INPUTS ===
int smaPeriodInput = input.int({2}, "SMA Period", minval=1, tooltip="Period for center SMA")
int atrPeriodInput = input.int({3}, "ATR Period", minval=1, tooltip="Period for ATR calculation")
float multiplierInput = input.float({4:.1}, "Multiplier", minval=0.1, step=0.1, tooltip="ATR multiplier for bands")

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float smaValue = ta.sma(close, smaPeriodInput)
float atrValue = ta.atr(atrPeriodInput)
float upperBand = smaValue + atrValue * multiplierInput
float lowerBand = smaValue - atrValue * multiplierInput

// === SIGNALS ===
bool entryCondition = close > upperBand
bool exitCondition = close < smaValue

// === STRATEGY EXECUTION ===
if entryCondition and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if exitCondition and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(smaValue, "SMA", color=color.blue, linewidth=1)
plot(upperBand, "Upper Band", color=color.green, linewidth=1)
plot(lowerBand, "Lower Band", color=color.red, linewidth=1)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)

// Entry/Exit markers
plotshape(entryCondition and strategy.position_size[1] == 0, "Entry", shape.triangleup, location.belowbar, color.green, size=size.small)
plotshape(exitCondition and strategy.position_size[1] > 0, "Exit", shape.triangledown, location.abovebar, color.red, size=size.small)
"#,
                    strategy_name, config_display, sma_period, atr_period, multiplier, perf_comment
                )
            }

            Self::Tsmom { lookback } => {
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=false, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Time-Series Momentum (TSMOM)
// CONFIG: lookback={2}
// SOURCE: TrendLab YOLO sweep session
{3}// ============================================================================

// === INPUTS ===
int lookbackInput = input.int({2}, "Lookback Period", minval=1, tooltip="Period for momentum calculation")

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float momentum = (close - close[lookbackInput]) / close[lookbackInput] * 100

// === SIGNALS ===
bool entryCondition = momentum > 0 and momentum[1] <= 0
bool exitCondition = momentum < 0

// === STRATEGY EXECUTION ===
if momentum > 0 and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if momentum < 0 and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(momentum, "Momentum %", color=momentum > 0 ? color.green : color.red, linewidth=2)
hline(0, "Zero Line", color=color.gray)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)
"#,
                    strategy_name, config_display, lookback, perf_comment
                )
            }

            Self::MACrossover {
                fast,
                slow,
                ma_type,
            } => {
                let ma_func = match ma_type {
                    MAType::SMA => "ta.sma",
                    MAType::EMA => "ta.ema",
                };
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Moving Average Crossover
// CONFIG: fast={2}, slow={3}, type={4:?}
// SOURCE: TrendLab YOLO sweep session
{5}// ============================================================================

// === INPUTS ===
int fastPeriodInput = input.int({2}, "Fast Period", minval=1, tooltip="Fast MA period")
int slowPeriodInput = input.int({3}, "Slow Period", minval=1, tooltip="Slow MA period")

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float fastMA = {6}(close, fastPeriodInput)
float slowMA = {6}(close, slowPeriodInput)

// === SIGNALS ===
bool goldenCross = ta.crossover(fastMA, slowMA)
bool deathCross = ta.crossunder(fastMA, slowMA)

// === STRATEGY EXECUTION ===
if goldenCross and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if deathCross and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(fastMA, "Fast MA", color=color.green, linewidth=1)
plot(slowMA, "Slow MA", color=color.red, linewidth=2)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)

// Entry/Exit markers
plotshape(goldenCross, "Golden Cross", shape.triangleup, location.belowbar, color.green, size=size.small)
plotshape(deathCross, "Death Cross", shape.triangledown, location.abovebar, color.red, size=size.small)
"#,
                    strategy_name, config_display, fast, slow, ma_type, perf_comment, ma_func
                )
            }

            Self::Donchian {
                entry_lookback,
                exit_lookback,
            } => {
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Donchian Channel Breakout
// CONFIG: entry_lookback={2}, exit_lookback={3}
// SOURCE: TrendLab YOLO sweep session
{4}// ============================================================================

// === INPUTS ===
int entryLookbackInput = input.int({2}, "Entry Lookback", minval=1, tooltip="Lookback for entry channel")
int exitLookbackInput = input.int({3}, "Exit Lookback", minval=1, tooltip="Lookback for exit channel")

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float entryHigh = ta.highest(high, entryLookbackInput)
float entryLow = ta.lowest(low, entryLookbackInput)
float exitLow = ta.lowest(low, exitLookbackInput)

// === SIGNALS ===
bool entryCondition = high >= entryHigh[1]
bool exitCondition = low <= exitLow[1]

// === STRATEGY EXECUTION ===
if entryCondition and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if exitCondition and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(entryHigh, "Entry High", color=color.green, linewidth=1)
plot(entryLow, "Entry Low", color=color.blue, linewidth=1)
plot(exitLow, "Exit Low", color=color.red, linewidth=1, style=plot.style_stepline)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)

// Entry/Exit markers
plotshape(entryCondition and strategy.position_size[1] == 0, "Entry", shape.triangleup, location.belowbar, color.green, size=size.small)
plotshape(exitCondition and strategy.position_size[1] > 0, "Exit", shape.triangledown, location.abovebar, color.red, size=size.small)
"#,
                    strategy_name, config_display, entry_lookback, exit_lookback, perf_comment
                )
            }

            Self::Aroon { period } => {
                format!(
                    r#"//@version=6
strategy("{0} ({1})", overlay=false, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Aroon Indicator
// CONFIG: period={2}
// SOURCE: TrendLab YOLO sweep session
{3}// ============================================================================

// === INPUTS ===
int periodInput = input.int({2}, "Period", minval=1, tooltip="Aroon lookback period")

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float aroonUp = ta.aroon(periodInput, 1)
float aroonDown = ta.aroon(periodInput, -1)

// === SIGNALS ===
bool bullishCross = ta.crossover(aroonUp, aroonDown)
bool bearishCross = ta.crossunder(aroonUp, aroonDown)

// === STRATEGY EXECUTION ===
if bullishCross and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if bearishCross and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(aroonUp, "Aroon Up", color=color.green, linewidth=1)
plot(aroonDown, "Aroon Down", color=color.red, linewidth=1)
hline(50, "Mid Line", color=color.gray)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)
"#,
                    strategy_name, config_display, period, perf_comment
                )
            }

            // Turtle systems use fixed parameters
            Self::TurtleS1 => {
                format!(
                    r#"//@version=6
strategy("Turtle System S1 (20/10)", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Turtle Trading System S1
// CONFIG: Fixed 20-day entry, 10-day exit (original Turtle rules)
// SOURCE: TrendLab YOLO sweep session
{perf_comment}// ============================================================================

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float entryHigh = ta.highest(high, 20)
float exitLow = ta.lowest(low, 10)

// === SIGNALS ===
bool entryCondition = high >= entryHigh[1]
bool exitCondition = low <= exitLow[1]

// === STRATEGY EXECUTION ===
if entryCondition and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if exitCondition and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(entryHigh, "20-Day High", color=color.green, linewidth=1)
plot(exitLow, "10-Day Low", color=color.red, linewidth=1)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)
"#,
                    perf_comment = perf_comment
                )
            }

            Self::TurtleS2 => {
                format!(
                    r#"//@version=6
strategy("Turtle System S2 (55/20)", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// ============================================================================
// STRATEGY: Turtle Trading System S2
// CONFIG: Fixed 55-day entry, 20-day exit (original Turtle rules)
// SOURCE: TrendLab YOLO sweep session
{perf_comment}// ============================================================================

// === DATE RANGE ===
startDate = input.time(timestamp("2020-01-01"), "Start Date", tooltip="Backtest start date")
endDate = input.time(timestamp("2099-12-31"), "End Date", tooltip="Backtest end date")
bool inDateRange = time >= startDate and time <= endDate

// === INDICATORS ===
float entryHigh = ta.highest(high, 55)
float exitLow = ta.lowest(low, 20)

// === SIGNALS ===
bool entryCondition = high >= entryHigh[1]
bool exitCondition = low <= exitLow[1]

// === STRATEGY EXECUTION ===
if entryCondition and strategy.position_size == 0 and inDateRange
    strategy.entry("Long", strategy.long)

if exitCondition and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
plot(entryHigh, "55-Day High", color=color.green, linewidth=1)
plot(exitLow, "20-Day Low", color=color.red, linewidth=1)

// Background color when in position
bgcolor(strategy.position_size > 0 ? color.new(color.green, 90) : na)
"#,
                    perf_comment = perf_comment
                )
            }

            // Default fallback for strategies not yet implemented
            _ => {
                format!(
                    r#"//@version=6
// ============================================================================
// STRATEGY: {} ({})
// NOTE: Pine Script generation not yet implemented for this strategy type.
//       Use /pine:generate to create a custom script.
// ============================================================================
"#,
                    strategy_name, config_display
                )
            }
        }
    }
}

/// Parameter ranges for a strategy type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyParams {
    Donchian {
        entry_lookbacks: Vec<usize>,
        exit_lookbacks: Vec<usize>,
    },
    TurtleS1, // No params - fixed
    TurtleS2, // No params - fixed
    MACrossover {
        fast_periods: Vec<usize>,
        slow_periods: Vec<usize>,
        ma_types: Vec<MAType>,
    },
    Tsmom {
        lookbacks: Vec<usize>,
    },
    // Phase 2: Momentum & Direction
    DmiAdx {
        di_periods: Vec<usize>,
        adx_periods: Vec<usize>,
        adx_thresholds: Vec<f64>,
    },
    Aroon {
        periods: Vec<usize>,
    },
    BollingerSqueeze {
        periods: Vec<usize>,
        std_mults: Vec<f64>,
        squeeze_thresholds: Vec<f64>,
    },
    // Phase 1: ATR-Based Channels
    Keltner {
        ema_periods: Vec<usize>,
        atr_periods: Vec<usize>,
        multipliers: Vec<f64>,
    },
    STARC {
        sma_periods: Vec<usize>,
        atr_periods: Vec<usize>,
        multipliers: Vec<f64>,
    },
    Supertrend {
        atr_periods: Vec<usize>,
        multipliers: Vec<f64>,
    },
    SupertrendVolume {
        atr_periods: Vec<usize>,
        multipliers: Vec<f64>,
        volume_lookbacks: Vec<usize>,
        volume_threshold_pcts: Vec<f64>,
    },
    SupertrendConfirmed {
        atr_periods: Vec<usize>,
        multipliers: Vec<f64>,
        confirmation_bars: Vec<usize>,
    },
    SupertrendAsymmetric {
        atr_periods: Vec<usize>,
        entry_multipliers: Vec<f64>,
        exit_multipliers: Vec<f64>,
    },
    SupertrendCooldown {
        atr_periods: Vec<usize>,
        multipliers: Vec<f64>,
        cooldown_bars: Vec<usize>,
    },
    // Phase 3: Price Structure
    FiftyTwoWeekHigh {
        periods: Vec<usize>,
        entry_pcts: Vec<f64>,
        exit_pcts: Vec<f64>,
    },
    FiftyTwoWeekHighMomentum {
        periods: Vec<usize>,
        entry_pcts: Vec<f64>,
        exit_pcts: Vec<f64>,
        momentum_periods: Vec<usize>,
        momentum_thresholds: Vec<f64>,
    },
    FiftyTwoWeekHighTrailing {
        periods: Vec<usize>,
        entry_pcts: Vec<f64>,
        trailing_stop_pcts: Vec<f64>,
    },
    DarvasBox {
        box_confirmation_bars: Vec<usize>,
    },
    LarryWilliams {
        range_multipliers: Vec<f64>,
        atr_stop_mults: Vec<f64>,
    },
    HeikinAshi {
        confirmation_bars: Vec<usize>,
    },
    // Phase 4: Complex Stateful + Ensemble
    ParabolicSar {
        af_starts: Vec<f64>,
        af_steps: Vec<f64>,
        af_maxs: Vec<f64>,
    },
    ParabolicSarFiltered {
        af_starts: Vec<f64>,
        af_steps: Vec<f64>,
        af_maxs: Vec<f64>,
        trend_ma_periods: Vec<usize>,
    },
    ParabolicSarDelayed {
        af_starts: Vec<f64>,
        af_steps: Vec<f64>,
        af_maxs: Vec<f64>,
        delay_bars: Vec<usize>,
    },
    OpeningRangeBreakout {
        range_bars: Vec<usize>,
        periods: Vec<OpeningPeriod>,
    },
    Ensemble {
        base_strategies: Vec<StrategyTypeId>,
        horizon_sets: Vec<Vec<usize>>,
        voting_methods: Vec<VotingMethod>,
    },
    // Phase 5: Oscillator Strategies
    Rsi {
        periods: Vec<usize>,
        oversolds: Vec<f64>,
        overboughts: Vec<f64>,
    },
    Macd {
        fast_periods: Vec<usize>,
        slow_periods: Vec<usize>,
        signal_periods: Vec<usize>,
        entry_modes: Vec<MACDEntryMode>,
    },
    Stochastic {
        k_periods: Vec<usize>,
        k_smooths: Vec<usize>,
        d_periods: Vec<usize>,
        oversolds: Vec<f64>,
        overboughts: Vec<f64>,
    },
    WilliamsR {
        periods: Vec<usize>,
        oversolds: Vec<f64>,
        overboughts: Vec<f64>,
    },
    Cci {
        periods: Vec<usize>,
        entry_thresholds: Vec<f64>,
        exit_thresholds: Vec<f64>,
    },
    Roc {
        periods: Vec<usize>,
    },
    RsiBollinger {
        rsi_periods: Vec<usize>,
        rsi_oversolds: Vec<f64>,
        rsi_exits: Vec<f64>,
        bb_periods: Vec<usize>,
        bb_std_mults: Vec<f64>,
    },
    MacdAdx {
        fast_periods: Vec<usize>,
        slow_periods: Vec<usize>,
        signal_periods: Vec<usize>,
        adx_periods: Vec<usize>,
        adx_thresholds: Vec<f64>,
    },
    OscillatorConfluence {
        rsi_periods: Vec<usize>,
        rsi_oversolds: Vec<f64>,
        rsi_overboughts: Vec<f64>,
        stoch_k_periods: Vec<usize>,
        stoch_k_smooths: Vec<usize>,
        stoch_d_periods: Vec<usize>,
        stoch_oversolds: Vec<f64>,
        stoch_overboughts: Vec<f64>,
    },
    Ichimoku {
        tenkan_periods: Vec<usize>,
        kijun_periods: Vec<usize>,
        senkou_b_periods: Vec<usize>,
    },
}

impl StrategyParams {
    /// Generate all config combinations from these params.
    pub fn generate_configs(&self) -> Vec<StrategyConfigId> {
        match self {
            Self::Donchian {
                entry_lookbacks,
                exit_lookbacks,
            } => {
                let mut configs = Vec::new();
                for &entry in entry_lookbacks {
                    for &exit in exit_lookbacks {
                        if exit < entry {
                            // Exit lookback should be less than entry
                            configs.push(StrategyConfigId::Donchian {
                                entry_lookback: entry,
                                exit_lookback: exit,
                            });
                        }
                    }
                }
                configs
            }
            Self::TurtleS1 => vec![StrategyConfigId::TurtleS1],
            Self::TurtleS2 => vec![StrategyConfigId::TurtleS2],
            Self::MACrossover {
                fast_periods,
                slow_periods,
                ma_types,
            } => {
                let mut configs = Vec::new();
                for &fast in fast_periods {
                    for &slow in slow_periods {
                        if fast < slow {
                            for &ma_type in ma_types {
                                configs.push(StrategyConfigId::MACrossover {
                                    fast,
                                    slow,
                                    ma_type,
                                });
                            }
                        }
                    }
                }
                configs
            }
            Self::Tsmom { lookbacks } => lookbacks
                .iter()
                .map(|&lookback| StrategyConfigId::Tsmom { lookback })
                .collect(),
            // Phase 2: Momentum & Direction
            Self::DmiAdx {
                di_periods,
                adx_periods,
                adx_thresholds,
            } => {
                let mut configs = Vec::new();
                for &di_period in di_periods {
                    for &adx_period in adx_periods {
                        for &adx_threshold in adx_thresholds {
                            configs.push(StrategyConfigId::DmiAdx {
                                di_period,
                                adx_period,
                                adx_threshold,
                            });
                        }
                    }
                }
                configs
            }
            Self::Aroon { periods } => periods
                .iter()
                .map(|&period| StrategyConfigId::Aroon { period })
                .collect(),
            Self::BollingerSqueeze {
                periods,
                std_mults,
                squeeze_thresholds,
            } => {
                let mut configs = Vec::new();
                for &period in periods {
                    for &std_mult in std_mults {
                        for &squeeze_threshold in squeeze_thresholds {
                            configs.push(StrategyConfigId::BollingerSqueeze {
                                period,
                                std_mult,
                                squeeze_threshold,
                            });
                        }
                    }
                }
                configs
            }
            // Phase 1: ATR-Based Channels
            Self::Keltner {
                ema_periods,
                atr_periods,
                multipliers,
            } => {
                let mut configs = Vec::new();
                for &ema_period in ema_periods {
                    for &atr_period in atr_periods {
                        for &multiplier in multipliers {
                            configs.push(StrategyConfigId::Keltner {
                                ema_period,
                                atr_period,
                                multiplier,
                            });
                        }
                    }
                }
                configs
            }
            Self::STARC {
                sma_periods,
                atr_periods,
                multipliers,
            } => {
                let mut configs = Vec::new();
                for &sma_period in sma_periods {
                    for &atr_period in atr_periods {
                        for &multiplier in multipliers {
                            configs.push(StrategyConfigId::STARC {
                                sma_period,
                                atr_period,
                                multiplier,
                            });
                        }
                    }
                }
                configs
            }
            Self::Supertrend {
                atr_periods,
                multipliers,
            } => {
                let mut configs = Vec::new();
                for &atr_period in atr_periods {
                    for &multiplier in multipliers {
                        configs.push(StrategyConfigId::Supertrend {
                            atr_period,
                            multiplier,
                        });
                    }
                }
                configs
            }
            Self::SupertrendVolume {
                atr_periods,
                multipliers,
                volume_lookbacks,
                volume_threshold_pcts,
            } => {
                let mut configs = Vec::new();
                for &atr_period in atr_periods {
                    for &multiplier in multipliers {
                        for &volume_lookback in volume_lookbacks {
                            for &volume_threshold_pct in volume_threshold_pcts {
                                configs.push(StrategyConfigId::SupertrendVolume {
                                    atr_period,
                                    multiplier,
                                    volume_lookback,
                                    volume_threshold_pct,
                                });
                            }
                        }
                    }
                }
                configs
            }
            Self::SupertrendConfirmed {
                atr_periods,
                multipliers,
                confirmation_bars,
            } => {
                let mut configs = Vec::new();
                for &atr_period in atr_periods {
                    for &multiplier in multipliers {
                        for &bars in confirmation_bars {
                            configs.push(StrategyConfigId::SupertrendConfirmed {
                                atr_period,
                                multiplier,
                                confirmation_bars: bars,
                            });
                        }
                    }
                }
                configs
            }
            Self::SupertrendAsymmetric {
                atr_periods,
                entry_multipliers,
                exit_multipliers,
            } => {
                let mut configs = Vec::new();
                for &atr_period in atr_periods {
                    for &entry_multiplier in entry_multipliers {
                        for &exit_multiplier in exit_multipliers {
                            if exit_multiplier > entry_multiplier {
                                configs.push(StrategyConfigId::SupertrendAsymmetric {
                                    atr_period,
                                    entry_multiplier,
                                    exit_multiplier,
                                });
                            }
                        }
                    }
                }
                configs
            }
            Self::SupertrendCooldown {
                atr_periods,
                multipliers,
                cooldown_bars,
            } => {
                let mut configs = Vec::new();
                for &atr_period in atr_periods {
                    for &multiplier in multipliers {
                        for &bars in cooldown_bars {
                            configs.push(StrategyConfigId::SupertrendCooldown {
                                atr_period,
                                multiplier,
                                cooldown_bars: bars,
                            });
                        }
                    }
                }
                configs
            }
            // Phase 3: Price Structure
            Self::FiftyTwoWeekHigh {
                periods,
                entry_pcts,
                exit_pcts,
            } => {
                let mut configs = Vec::new();
                for &period in periods {
                    for &entry_pct in entry_pcts {
                        for &exit_pct in exit_pcts {
                            if exit_pct < entry_pct {
                                configs.push(StrategyConfigId::FiftyTwoWeekHigh {
                                    period,
                                    entry_pct,
                                    exit_pct,
                                });
                            }
                        }
                    }
                }
                configs
            }
            Self::FiftyTwoWeekHighMomentum {
                periods,
                entry_pcts,
                exit_pcts,
                momentum_periods,
                momentum_thresholds,
            } => {
                let mut configs = Vec::new();
                for &period in periods {
                    for &entry_pct in entry_pcts {
                        for &exit_pct in exit_pcts {
                            for &momentum_period in momentum_periods {
                                for &momentum_threshold in momentum_thresholds {
                                    if exit_pct < entry_pct {
                                        configs.push(StrategyConfigId::FiftyTwoWeekHighMomentum {
                                            period,
                                            entry_pct,
                                            exit_pct,
                                            momentum_period,
                                            momentum_threshold,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                configs
            }
            Self::FiftyTwoWeekHighTrailing {
                periods,
                entry_pcts,
                trailing_stop_pcts,
            } => {
                let mut configs = Vec::new();
                for &period in periods {
                    for &entry_pct in entry_pcts {
                        for &trailing_stop_pct in trailing_stop_pcts {
                            configs.push(StrategyConfigId::FiftyTwoWeekHighTrailing {
                                period,
                                entry_pct,
                                trailing_stop_pct,
                            });
                        }
                    }
                }
                configs
            }
            Self::DarvasBox {
                box_confirmation_bars,
            } => box_confirmation_bars
                .iter()
                .map(|&bars| StrategyConfigId::DarvasBox {
                    box_confirmation_bars: bars,
                })
                .collect(),
            Self::LarryWilliams {
                range_multipliers,
                atr_stop_mults,
            } => {
                let mut configs = Vec::new();
                for &range_multiplier in range_multipliers {
                    for &atr_stop_mult in atr_stop_mults {
                        configs.push(StrategyConfigId::LarryWilliams {
                            range_multiplier,
                            atr_stop_mult,
                        });
                    }
                }
                configs
            }
            Self::HeikinAshi { confirmation_bars } => confirmation_bars
                .iter()
                .map(|&bars| StrategyConfigId::HeikinAshi {
                    confirmation_bars: bars,
                })
                .collect(),
            // Phase 4: Complex Stateful + Ensemble
            Self::ParabolicSar {
                af_starts,
                af_steps,
                af_maxs,
            } => {
                let mut configs = Vec::new();
                for &af_start in af_starts {
                    for &af_step in af_steps {
                        for &af_max in af_maxs {
                            if af_max >= af_start {
                                configs.push(StrategyConfigId::ParabolicSar {
                                    af_start,
                                    af_step,
                                    af_max,
                                });
                            }
                        }
                    }
                }
                configs
            }
            Self::ParabolicSarFiltered {
                af_starts,
                af_steps,
                af_maxs,
                trend_ma_periods,
            } => {
                let mut configs = Vec::new();
                for &af_start in af_starts {
                    for &af_step in af_steps {
                        for &af_max in af_maxs {
                            for &trend_ma_period in trend_ma_periods {
                                if af_max >= af_start {
                                    configs.push(StrategyConfigId::ParabolicSarFiltered {
                                        af_start,
                                        af_step,
                                        af_max,
                                        trend_ma_period,
                                    });
                                }
                            }
                        }
                    }
                }
                configs
            }
            Self::ParabolicSarDelayed {
                af_starts,
                af_steps,
                af_maxs,
                delay_bars,
            } => {
                let mut configs = Vec::new();
                for &af_start in af_starts {
                    for &af_step in af_steps {
                        for &af_max in af_maxs {
                            for &bars in delay_bars {
                                if af_max >= af_start {
                                    configs.push(StrategyConfigId::ParabolicSarDelayed {
                                        af_start,
                                        af_step,
                                        af_max,
                                        delay_bars: bars,
                                    });
                                }
                            }
                        }
                    }
                }
                configs
            }
            Self::OpeningRangeBreakout {
                range_bars,
                periods,
            } => {
                let mut configs = Vec::new();
                for &rb in range_bars {
                    for period in periods {
                        configs.push(StrategyConfigId::OpeningRangeBreakout {
                            range_bars: rb,
                            period: *period,
                        });
                    }
                }
                configs
            }
            Self::Ensemble {
                base_strategies,
                horizon_sets,
                voting_methods,
            } => {
                let mut configs = Vec::new();
                for &base_strategy in base_strategies {
                    for horizons in horizon_sets {
                        for &voting in voting_methods {
                            configs.push(StrategyConfigId::Ensemble {
                                base_strategy,
                                horizons: horizons.clone(),
                                voting,
                            });
                        }
                    }
                }
                configs
            }
            // Phase 5: Oscillator Strategies
            Self::Rsi {
                periods,
                oversolds,
                overboughts,
            } => {
                let mut configs = Vec::new();
                for &period in periods {
                    for &oversold in oversolds {
                        for &overbought in overboughts {
                            configs.push(StrategyConfigId::Rsi {
                                period,
                                oversold,
                                overbought,
                            });
                        }
                    }
                }
                configs
            }
            Self::Macd {
                fast_periods,
                slow_periods,
                signal_periods,
                entry_modes,
            } => {
                let mut configs = Vec::new();
                for &fast_period in fast_periods {
                    for &slow_period in slow_periods {
                        if slow_period > fast_period {
                            for &signal_period in signal_periods {
                                for &entry_mode in entry_modes {
                                    configs.push(StrategyConfigId::Macd {
                                        fast_period,
                                        slow_period,
                                        signal_period,
                                        entry_mode,
                                    });
                                }
                            }
                        }
                    }
                }
                configs
            }
            Self::Stochastic {
                k_periods,
                k_smooths,
                d_periods,
                oversolds,
                overboughts,
            } => {
                let mut configs = Vec::new();
                for &k_period in k_periods {
                    for &k_smooth in k_smooths {
                        for &d_period in d_periods {
                            for &oversold in oversolds {
                                for &overbought in overboughts {
                                    configs.push(StrategyConfigId::Stochastic {
                                        k_period,
                                        k_smooth,
                                        d_period,
                                        oversold,
                                        overbought,
                                    });
                                }
                            }
                        }
                    }
                }
                configs
            }
            Self::WilliamsR {
                periods,
                oversolds,
                overboughts,
            } => {
                let mut configs = Vec::new();
                for &period in periods {
                    for &oversold in oversolds {
                        for &overbought in overboughts {
                            configs.push(StrategyConfigId::WilliamsR {
                                period,
                                oversold,
                                overbought,
                            });
                        }
                    }
                }
                configs
            }
            Self::Cci {
                periods,
                entry_thresholds,
                exit_thresholds,
            } => {
                let mut configs = Vec::new();
                for &period in periods {
                    for &entry_threshold in entry_thresholds {
                        for &exit_threshold in exit_thresholds {
                            configs.push(StrategyConfigId::Cci {
                                period,
                                entry_threshold,
                                exit_threshold,
                            });
                        }
                    }
                }
                configs
            }
            Self::Roc { periods } => periods
                .iter()
                .map(|&period| StrategyConfigId::Roc { period })
                .collect(),
            Self::RsiBollinger {
                rsi_periods,
                rsi_oversolds,
                rsi_exits,
                bb_periods,
                bb_std_mults,
            } => {
                let mut configs = Vec::new();
                for &rsi_period in rsi_periods {
                    for &rsi_oversold in rsi_oversolds {
                        for &rsi_exit in rsi_exits {
                            for &bb_period in bb_periods {
                                for &bb_std_mult in bb_std_mults {
                                    configs.push(StrategyConfigId::RsiBollinger {
                                        rsi_period,
                                        rsi_oversold,
                                        rsi_exit,
                                        bb_period,
                                        bb_std_mult,
                                    });
                                }
                            }
                        }
                    }
                }
                configs
            }
            Self::MacdAdx {
                fast_periods,
                slow_periods,
                signal_periods,
                adx_periods,
                adx_thresholds,
            } => {
                let mut configs = Vec::new();
                for &fast_period in fast_periods {
                    for &slow_period in slow_periods {
                        if slow_period > fast_period {
                            for &signal_period in signal_periods {
                                for &adx_period in adx_periods {
                                    for &adx_threshold in adx_thresholds {
                                        configs.push(StrategyConfigId::MacdAdx {
                                            fast_period,
                                            slow_period,
                                            signal_period,
                                            adx_period,
                                            adx_threshold,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                configs
            }
            Self::OscillatorConfluence {
                rsi_periods,
                rsi_oversolds,
                rsi_overboughts,
                stoch_k_periods,
                stoch_k_smooths,
                stoch_d_periods,
                stoch_oversolds,
                stoch_overboughts,
            } => {
                let mut configs = Vec::new();
                for &rsi_period in rsi_periods {
                    for &rsi_oversold in rsi_oversolds {
                        for &rsi_overbought in rsi_overboughts {
                            for &stoch_k_period in stoch_k_periods {
                                for &stoch_k_smooth in stoch_k_smooths {
                                    for &stoch_d_period in stoch_d_periods {
                                        for &stoch_oversold in stoch_oversolds {
                                            for &stoch_overbought in stoch_overboughts {
                                                configs.push(
                                                    StrategyConfigId::OscillatorConfluence {
                                                        rsi_period,
                                                        rsi_oversold,
                                                        rsi_overbought,
                                                        stoch_k_period,
                                                        stoch_k_smooth,
                                                        stoch_d_period,
                                                        stoch_oversold,
                                                        stoch_overbought,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                configs
            }
            Self::Ichimoku {
                tenkan_periods,
                kijun_periods,
                senkou_b_periods,
            } => {
                let mut configs = Vec::new();
                for &tenkan_period in tenkan_periods {
                    for &kijun_period in kijun_periods {
                        for &senkou_b_period in senkou_b_periods {
                            configs.push(StrategyConfigId::Ichimoku {
                                tenkan_period,
                                kijun_period,
                                senkou_b_period,
                            });
                        }
                    }
                }
                configs
            }
        }
    }

    /// Count configurations this would generate.
    pub fn config_count(&self) -> usize {
        self.generate_configs().len()
    }
}

/// Configuration for a strategy grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyGridConfig {
    pub strategy_type: StrategyTypeId,
    pub enabled: bool,
    pub params: StrategyParams,
}

impl StrategyGridConfig {
    /// Mark this config as disabled (won't be included in sweeps).
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Default Donchian grid.
    pub fn donchian_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::Donchian,
            enabled: true,
            params: StrategyParams::Donchian {
                entry_lookbacks: vec![10, 20, 30, 40, 55],
                exit_lookbacks: vec![5, 10, 15, 20],
            },
        }
    }

    /// Turtle System 1 (fixed preset).
    pub fn turtle_s1() -> Self {
        Self {
            strategy_type: StrategyTypeId::TurtleS1,
            enabled: true,
            params: StrategyParams::TurtleS1,
        }
    }

    /// Turtle System 2 (fixed preset).
    pub fn turtle_s2() -> Self {
        Self {
            strategy_type: StrategyTypeId::TurtleS2,
            enabled: true,
            params: StrategyParams::TurtleS2,
        }
    }

    /// Default MA Crossover grid.
    pub fn ma_crossover_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::MACrossover,
            enabled: true,
            params: StrategyParams::MACrossover {
                fast_periods: vec![10, 20, 50],
                slow_periods: vec![50, 100, 200],
                ma_types: vec![MAType::SMA, MAType::EMA],
            },
        }
    }

    /// Default TSMOM grid.
    pub fn tsmom_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::Tsmom,
            enabled: true,
            params: StrategyParams::Tsmom {
                lookbacks: vec![21, 63, 126, 252],
            },
        }
    }

    // -------------------------------------------------------------------------
    // Depth-aware constructors (research-backed parameter ranges)
    // -------------------------------------------------------------------------

    /// Donchian grid with specified sweep depth.
    ///
    /// Parameter ranges based on Turtle Trading rules:
    /// - Quick: Classic Turtle values (20/55 entry, 10/20 exit)
    /// - Standard: Extended range with common values
    /// - Comprehensive: Fine granularity from day trading to position trading
    pub fn donchian_with_depth(depth: SweepDepth) -> Self {
        let (entry_lookbacks, exit_lookbacks) = match depth {
            SweepDepth::Quick => (vec![20, 55], vec![10, 20]),
            SweepDepth::Standard => (vec![10, 20, 30, 40, 55], vec![5, 10, 15, 20]),
            SweepDepth::Comprehensive => (
                vec![10, 15, 20, 30, 40, 55, 80, 100],
                vec![5, 10, 15, 20, 25, 40],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::Donchian,
            enabled: true,
            params: StrategyParams::Donchian {
                entry_lookbacks,
                exit_lookbacks,
            },
        }
    }

    /// MA Crossover grid with specified sweep depth.
    ///
    /// Parameter ranges based on Golden Cross/Death Cross research:
    /// - Quick: Classic 50/200 SMA only
    /// - Standard: Common fast/slow combinations with SMA + EMA
    /// - Comprehensive: Extended periods including short-term scalping
    pub fn ma_crossover_with_depth(depth: SweepDepth) -> Self {
        let (fast_periods, slow_periods, ma_types) = match depth {
            SweepDepth::Quick => (vec![20, 50], vec![50, 200], vec![MAType::SMA]),
            SweepDepth::Standard => (
                vec![10, 20, 50],
                vec![50, 100, 200],
                vec![MAType::SMA, MAType::EMA],
            ),
            SweepDepth::Comprehensive => (
                vec![5, 9, 10, 20, 50],
                vec![20, 21, 50, 100, 200],
                vec![MAType::SMA, MAType::EMA],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::MACrossover,
            enabled: true,
            params: StrategyParams::MACrossover {
                fast_periods,
                slow_periods,
                ma_types,
            },
        }
    }

    /// TSMOM grid with specified sweep depth.
    ///
    /// Parameter ranges based on academic momentum research (Moskowitz et al.):
    /// - Quick: Quarterly + annual (63, 252 trading days)
    /// - Standard: Monthly to annual (21, 63, 126, 252)
    /// - Comprehensive: Bi-monthly steps through the year
    pub fn tsmom_with_depth(depth: SweepDepth) -> Self {
        let lookbacks = match depth {
            SweepDepth::Quick => vec![63, 252],
            SweepDepth::Standard => vec![21, 63, 126, 252],
            SweepDepth::Comprehensive => vec![21, 42, 63, 126, 189, 252],
        };
        Self {
            strategy_type: StrategyTypeId::Tsmom,
            enabled: true,
            params: StrategyParams::Tsmom { lookbacks },
        }
    }

    // -------------------------------------------------------------------------
    // Phase 2: Momentum & Direction Strategies
    // -------------------------------------------------------------------------

    /// Default DMI/ADX grid.
    pub fn dmi_adx_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::DmiAdx,
            enabled: true,
            params: StrategyParams::DmiAdx {
                di_periods: vec![10, 14, 20, 25],
                adx_periods: vec![10, 14, 20],
                adx_thresholds: vec![20.0, 25.0, 30.0],
            },
        }
    }

    /// DMI/ADX grid with specified sweep depth.
    ///
    /// Parameter ranges based on Wilder's original research:
    /// - Quick: Classic 14-period with standard threshold
    /// - Standard: Common variations
    /// - Comprehensive: Extended exploration
    pub fn dmi_adx_with_depth(depth: SweepDepth) -> Self {
        let (di_periods, adx_periods, adx_thresholds) = match depth {
            SweepDepth::Quick => (vec![14], vec![14], vec![25.0]),
            SweepDepth::Standard => (
                vec![10, 14, 20, 25],
                vec![10, 14, 20],
                vec![20.0, 25.0, 30.0],
            ),
            SweepDepth::Comprehensive => (
                vec![7, 10, 14, 20, 25, 30],
                vec![7, 10, 14, 20, 25],
                vec![15.0, 20.0, 25.0, 30.0, 35.0],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::DmiAdx,
            enabled: true,
            params: StrategyParams::DmiAdx {
                di_periods,
                adx_periods,
                adx_thresholds,
            },
        }
    }

    /// Default Aroon grid.
    pub fn aroon_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::Aroon,
            enabled: true,
            params: StrategyParams::Aroon {
                periods: vec![14, 20, 25, 30, 50],
            },
        }
    }

    /// Aroon grid with specified sweep depth.
    ///
    /// Parameter ranges based on common trading timeframes:
    /// - Quick: Standard 25-period
    /// - Standard: Common variations
    /// - Comprehensive: Extended exploration
    pub fn aroon_with_depth(depth: SweepDepth) -> Self {
        let periods = match depth {
            SweepDepth::Quick => vec![25],
            SweepDepth::Standard => vec![14, 20, 25, 30, 50],
            SweepDepth::Comprehensive => vec![10, 14, 20, 25, 30, 40, 50, 70],
        };
        Self {
            strategy_type: StrategyTypeId::Aroon,
            enabled: true,
            params: StrategyParams::Aroon { periods },
        }
    }

    /// Default Bollinger Squeeze grid.
    pub fn bollinger_squeeze_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::BollingerSqueeze,
            enabled: true,
            params: StrategyParams::BollingerSqueeze {
                periods: vec![15, 20, 25],
                std_mults: vec![1.5, 2.0, 2.5],
                squeeze_thresholds: vec![0.03, 0.04, 0.05],
            },
        }
    }

    /// Bollinger Squeeze grid with specified sweep depth.
    ///
    /// Parameter ranges based on Bollinger's research:
    /// - Quick: Standard 20-period with 2.0 std dev
    /// - Standard: Common variations
    /// - Comprehensive: Extended exploration
    pub fn bollinger_squeeze_with_depth(depth: SweepDepth) -> Self {
        let (periods, std_mults, squeeze_thresholds) = match depth {
            SweepDepth::Quick => (vec![20], vec![2.0], vec![0.04]),
            SweepDepth::Standard => (
                vec![15, 20, 25],
                vec![1.5, 2.0, 2.5],
                vec![0.03, 0.04, 0.05],
            ),
            SweepDepth::Comprehensive => (
                vec![10, 15, 20, 25, 30],
                vec![1.5, 2.0, 2.5, 3.0],
                vec![0.02, 0.03, 0.04, 0.05, 0.06],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::BollingerSqueeze,
            enabled: true,
            params: StrategyParams::BollingerSqueeze {
                periods,
                std_mults,
                squeeze_thresholds,
            },
        }
    }

    // -------------------------------------------------------------------------
    // Phase 1: ATR-Based Channel Strategies
    // -------------------------------------------------------------------------

    /// Default Keltner Channel grid.
    pub fn keltner_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::Keltner,
            enabled: true,
            params: StrategyParams::Keltner {
                ema_periods: vec![10, 20, 30],
                atr_periods: vec![10, 14, 20],
                multipliers: vec![1.5, 2.0, 2.5],
            },
        }
    }

    /// Keltner Channel grid with specified sweep depth.
    pub fn keltner_with_depth(depth: SweepDepth) -> Self {
        let (ema_periods, atr_periods, multipliers) = match depth {
            SweepDepth::Quick => (vec![20], vec![10], vec![2.0]),
            SweepDepth::Standard => (vec![10, 20, 30], vec![10, 14, 20], vec![1.5, 2.0, 2.5]),
            SweepDepth::Comprehensive => (
                vec![10, 15, 20, 30, 50],
                vec![7, 10, 14, 20],
                vec![1.0, 1.5, 2.0, 2.5, 3.0],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::Keltner,
            enabled: true,
            params: StrategyParams::Keltner {
                ema_periods,
                atr_periods,
                multipliers,
            },
        }
    }

    /// Default STARC Bands grid.
    pub fn starc_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::STARC,
            enabled: true,
            params: StrategyParams::STARC {
                sma_periods: vec![15, 20, 25],
                atr_periods: vec![10, 15, 20],
                multipliers: vec![1.5, 2.0, 2.5],
            },
        }
    }

    /// STARC Bands grid with specified sweep depth.
    pub fn starc_with_depth(depth: SweepDepth) -> Self {
        let (sma_periods, atr_periods, multipliers) = match depth {
            SweepDepth::Quick => (vec![20], vec![15], vec![2.0]),
            SweepDepth::Standard => (vec![15, 20, 25], vec![10, 15, 20], vec![1.5, 2.0, 2.5]),
            SweepDepth::Comprehensive => (
                vec![10, 15, 20, 25, 30],
                vec![7, 10, 15, 20],
                vec![1.0, 1.5, 2.0, 2.5, 3.0],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::STARC,
            enabled: true,
            params: StrategyParams::STARC {
                sma_periods,
                atr_periods,
                multipliers,
            },
        }
    }

    /// Default Supertrend grid.
    pub fn supertrend_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::Supertrend,
            enabled: true,
            params: StrategyParams::Supertrend {
                atr_periods: vec![7, 10, 14],
                multipliers: vec![2.0, 3.0, 4.0],
            },
        }
    }

    /// Supertrend grid with specified sweep depth.
    pub fn supertrend_with_depth(depth: SweepDepth) -> Self {
        let (atr_periods, multipliers) = match depth {
            SweepDepth::Quick => (vec![10], vec![3.0]),
            SweepDepth::Standard => (vec![7, 10, 14], vec![2.0, 3.0, 4.0]),
            SweepDepth::Comprehensive => (vec![5, 7, 10, 14, 20], vec![1.5, 2.0, 2.5, 3.0, 4.0]),
        };
        Self {
            strategy_type: StrategyTypeId::Supertrend,
            enabled: true,
            params: StrategyParams::Supertrend {
                atr_periods,
                multipliers,
            },
        }
    }

    /// Supertrend + Volume filter grid with specified sweep depth.
    pub fn supertrend_volume_with_depth(depth: SweepDepth) -> Self {
        let (atr_periods, multipliers, volume_lookbacks, volume_threshold_pcts) = match depth {
            SweepDepth::Quick => (vec![10], vec![3.0], vec![20], vec![1.2]),
            SweepDepth::Standard => (vec![10, 14], vec![2.5, 3.0], vec![20], vec![1.2, 1.5]),
            SweepDepth::Comprehensive => (
                vec![7, 10, 14],
                vec![2.0, 2.5, 3.0],
                vec![10, 20],
                vec![1.0, 1.2, 1.5],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::SupertrendVolume,
            enabled: true,
            params: StrategyParams::SupertrendVolume {
                atr_periods,
                multipliers,
                volume_lookbacks,
                volume_threshold_pcts,
            },
        }
    }

    /// Supertrend with confirmation bars grid.
    pub fn supertrend_confirmed_with_depth(depth: SweepDepth) -> Self {
        let (atr_periods, multipliers, confirmation_bars) = match depth {
            SweepDepth::Quick => (vec![10], vec![3.0], vec![2]),
            SweepDepth::Standard => (vec![10, 14], vec![2.5, 3.0], vec![2, 3]),
            SweepDepth::Comprehensive => (vec![7, 10, 14], vec![2.0, 2.5, 3.0], vec![2, 3, 5]),
        };
        Self {
            strategy_type: StrategyTypeId::SupertrendConfirmed,
            enabled: true,
            params: StrategyParams::SupertrendConfirmed {
                atr_periods,
                multipliers,
                confirmation_bars,
            },
        }
    }

    /// Supertrend with asymmetric entry/exit multipliers grid.
    pub fn supertrend_asymmetric_with_depth(depth: SweepDepth) -> Self {
        let (atr_periods, entry_multipliers, exit_multipliers) = match depth {
            SweepDepth::Quick => (vec![10], vec![2.5], vec![3.5]),
            SweepDepth::Standard => (vec![10, 14], vec![2.0, 2.5], vec![3.0, 3.5]),
            SweepDepth::Comprehensive => {
                (vec![7, 10, 14], vec![2.0, 2.5, 3.0], vec![3.0, 3.5, 4.0])
            }
        };
        Self {
            strategy_type: StrategyTypeId::SupertrendAsymmetric,
            enabled: true,
            params: StrategyParams::SupertrendAsymmetric {
                atr_periods,
                entry_multipliers,
                exit_multipliers,
            },
        }
    }

    /// Supertrend with re-entry cooldown grid.
    pub fn supertrend_cooldown_with_depth(depth: SweepDepth) -> Self {
        let (atr_periods, multipliers, cooldown_bars) = match depth {
            SweepDepth::Quick => (vec![10], vec![3.0], vec![10]),
            SweepDepth::Standard => (vec![10, 14], vec![2.5, 3.0], vec![5, 10, 20]),
            SweepDepth::Comprehensive => {
                (vec![7, 10, 14], vec![2.0, 2.5, 3.0], vec![5, 10, 15, 20])
            }
        };
        Self {
            strategy_type: StrategyTypeId::SupertrendCooldown,
            enabled: true,
            params: StrategyParams::SupertrendCooldown {
                atr_periods,
                multipliers,
                cooldown_bars,
            },
        }
    }

    // -------------------------------------------------------------------------
    // Phase 3: Price Structure Strategies
    // -------------------------------------------------------------------------

    /// Default 52-Week High grid.
    pub fn fifty_two_week_high_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::FiftyTwoWeekHigh,
            enabled: true,
            params: StrategyParams::FiftyTwoWeekHigh {
                periods: vec![126, 189, 252],
                entry_pcts: vec![0.95, 0.98, 1.0],
                exit_pcts: vec![0.85, 0.90],
            },
        }
    }

    /// 52-Week High grid with specified sweep depth.
    pub fn fifty_two_week_high_with_depth(depth: SweepDepth) -> Self {
        let (periods, entry_pcts, exit_pcts) = match depth {
            SweepDepth::Quick => (vec![252], vec![0.95], vec![0.90]),
            SweepDepth::Standard => (vec![126, 189, 252], vec![0.95, 0.98, 1.0], vec![0.85, 0.90]),
            SweepDepth::Comprehensive => (
                vec![63, 126, 189, 252],
                vec![0.90, 0.95, 0.98, 1.0],
                vec![0.80, 0.85, 0.90],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::FiftyTwoWeekHigh,
            enabled: true,
            params: StrategyParams::FiftyTwoWeekHigh {
                periods,
                entry_pcts,
                exit_pcts,
            },
        }
    }

    /// 52-Week High Momentum (ROC filter) grid with specified sweep depth.
    pub fn fifty_two_week_high_momentum_with_depth(depth: SweepDepth) -> Self {
        let (periods, entry_pcts, exit_pcts, momentum_periods, momentum_thresholds) = match depth {
            SweepDepth::Quick => (vec![252], vec![0.95], vec![0.90], vec![10], vec![0.0]),
            SweepDepth::Standard => (
                vec![189, 252],
                vec![0.95, 0.98],
                vec![0.85, 0.90],
                vec![10, 20],
                vec![0.0, 0.02],
            ),
            SweepDepth::Comprehensive => (
                vec![126, 189, 252],
                vec![0.95, 0.98, 1.0],
                vec![0.80, 0.85, 0.90],
                vec![10, 20],
                vec![0.0, 0.02, 0.05],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::FiftyTwoWeekHighMomentum,
            enabled: true,
            params: StrategyParams::FiftyTwoWeekHighMomentum {
                periods,
                entry_pcts,
                exit_pcts,
                momentum_periods,
                momentum_thresholds,
            },
        }
    }

    /// 52-Week High Trailing Stop grid with specified sweep depth.
    pub fn fifty_two_week_high_trailing_with_depth(depth: SweepDepth) -> Self {
        let (periods, entry_pcts, trailing_stop_pcts) = match depth {
            SweepDepth::Quick => (vec![252], vec![0.95], vec![0.10]),
            SweepDepth::Standard => (vec![189, 252], vec![0.95, 0.98], vec![0.05, 0.10, 0.15]),
            SweepDepth::Comprehensive => (
                vec![126, 189, 252],
                vec![0.95, 0.98, 1.0],
                vec![0.05, 0.08, 0.10, 0.15],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::FiftyTwoWeekHighTrailing,
            enabled: true,
            params: StrategyParams::FiftyTwoWeekHighTrailing {
                periods,
                entry_pcts,
                trailing_stop_pcts,
            },
        }
    }

    /// Default Darvas Box grid.
    pub fn darvas_box_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::DarvasBox,
            enabled: true,
            params: StrategyParams::DarvasBox {
                box_confirmation_bars: vec![2, 3, 4],
            },
        }
    }

    /// Darvas Box grid with specified sweep depth.
    pub fn darvas_box_with_depth(depth: SweepDepth) -> Self {
        let box_confirmation_bars = match depth {
            SweepDepth::Quick => vec![3],
            SweepDepth::Standard => vec![2, 3, 4],
            SweepDepth::Comprehensive => vec![2, 3, 4, 5],
        };
        Self {
            strategy_type: StrategyTypeId::DarvasBox,
            enabled: true,
            params: StrategyParams::DarvasBox {
                box_confirmation_bars,
            },
        }
    }

    /// Default Larry Williams grid.
    pub fn larry_williams_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::LarryWilliams,
            enabled: true,
            params: StrategyParams::LarryWilliams {
                range_multipliers: vec![0.3, 0.5, 0.7],
                atr_stop_mults: vec![1.5, 2.0, 2.5],
            },
        }
    }

    /// Larry Williams grid with specified sweep depth.
    pub fn larry_williams_with_depth(depth: SweepDepth) -> Self {
        let (range_multipliers, atr_stop_mults) = match depth {
            SweepDepth::Quick => (vec![0.5], vec![2.0]),
            SweepDepth::Standard => (vec![0.3, 0.5, 0.7], vec![1.5, 2.0, 2.5]),
            SweepDepth::Comprehensive => {
                (vec![0.2, 0.3, 0.5, 0.7, 1.0], vec![1.0, 1.5, 2.0, 2.5, 3.0])
            }
        };
        Self {
            strategy_type: StrategyTypeId::LarryWilliams,
            enabled: true,
            params: StrategyParams::LarryWilliams {
                range_multipliers,
                atr_stop_mults,
            },
        }
    }

    /// Default Heikin-Ashi grid.
    pub fn heikin_ashi_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::HeikinAshi,
            enabled: true,
            params: StrategyParams::HeikinAshi {
                confirmation_bars: vec![1, 2, 3],
            },
        }
    }

    /// Heikin-Ashi grid with specified sweep depth.
    pub fn heikin_ashi_with_depth(depth: SweepDepth) -> Self {
        let confirmation_bars = match depth {
            SweepDepth::Quick => vec![1],
            SweepDepth::Standard => vec![1, 2, 3],
            SweepDepth::Comprehensive => vec![1, 2, 3, 4],
        };
        Self {
            strategy_type: StrategyTypeId::HeikinAshi,
            enabled: true,
            params: StrategyParams::HeikinAshi { confirmation_bars },
        }
    }

    // -------------------------------------------------------------------------
    // Phase 4: Complex Stateful + Ensemble Strategies
    // -------------------------------------------------------------------------

    /// Default Parabolic SAR grid.
    pub fn parabolic_sar_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::ParabolicSar,
            enabled: true,
            params: StrategyParams::ParabolicSar {
                af_starts: vec![0.01, 0.02, 0.03],
                af_steps: vec![0.02],
                af_maxs: vec![0.15, 0.20, 0.25],
            },
        }
    }

    /// Parabolic SAR grid with specified sweep depth.
    ///
    /// Parameter ranges based on Wilder's original research:
    /// - Quick: Standard 0.02/0.02/0.20 only (1 config)
    /// - Standard: Common variations (~9 configs)
    /// - Comprehensive: Finer exploration (~27 configs)
    pub fn parabolic_sar_with_depth(depth: SweepDepth) -> Self {
        let (af_starts, af_steps, af_maxs) = match depth {
            SweepDepth::Quick => (vec![0.02], vec![0.02], vec![0.20]),
            SweepDepth::Standard => (vec![0.01, 0.02, 0.03], vec![0.02], vec![0.15, 0.20, 0.25]),
            SweepDepth::Comprehensive => (
                vec![0.01, 0.015, 0.02, 0.025, 0.03],
                vec![0.01, 0.02, 0.03],
                vec![0.10, 0.15, 0.20, 0.25, 0.30],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::ParabolicSar,
            enabled: true,
            params: StrategyParams::ParabolicSar {
                af_starts,
                af_steps,
                af_maxs,
            },
        }
    }

    /// Parabolic SAR with MA trend filter grid.
    pub fn parabolic_sar_filtered_with_depth(depth: SweepDepth) -> Self {
        let (af_starts, af_steps, af_maxs, trend_ma_periods) = match depth {
            SweepDepth::Quick => (vec![0.02], vec![0.02], vec![0.20], vec![50]),
            SweepDepth::Standard => (vec![0.02, 0.03], vec![0.02], vec![0.20], vec![50, 200]),
            SweepDepth::Comprehensive => (
                vec![0.01, 0.02, 0.03],
                vec![0.02],
                vec![0.15, 0.20, 0.25],
                vec![50, 100, 200],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::ParabolicSarFiltered,
            enabled: true,
            params: StrategyParams::ParabolicSarFiltered {
                af_starts,
                af_steps,
                af_maxs,
                trend_ma_periods,
            },
        }
    }

    /// Parabolic SAR with delayed entry after flip grid.
    pub fn parabolic_sar_delayed_with_depth(depth: SweepDepth) -> Self {
        let (af_starts, af_steps, af_maxs, delay_bars) = match depth {
            SweepDepth::Quick => (vec![0.02], vec![0.02], vec![0.20], vec![1]),
            SweepDepth::Standard => (vec![0.02, 0.03], vec![0.02], vec![0.20], vec![1, 2, 3]),
            SweepDepth::Comprehensive => (
                vec![0.01, 0.02, 0.03],
                vec![0.02],
                vec![0.15, 0.20, 0.25],
                vec![1, 2, 3, 5],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::ParabolicSarDelayed,
            enabled: true,
            params: StrategyParams::ParabolicSarDelayed {
                af_starts,
                af_steps,
                af_maxs,
                delay_bars,
            },
        }
    }

    /// Default Opening Range Breakout grid.
    pub fn orb_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::OpeningRangeBreakout,
            enabled: true,
            params: StrategyParams::OpeningRangeBreakout {
                range_bars: vec![3, 5, 10],
                periods: vec![
                    OpeningPeriod::Weekly,
                    OpeningPeriod::Monthly,
                    OpeningPeriod::Rolling,
                ],
            },
        }
    }

    /// Opening Range Breakout grid with specified sweep depth.
    ///
    /// Parameter ranges:
    /// - Quick: 5 bars, Weekly only (1 config)
    /// - Standard: 3/5/10 bars with Weekly + Rolling (6 configs)
    /// - Comprehensive: Extended bar counts with all periods (12 configs)
    pub fn orb_with_depth(depth: SweepDepth) -> Self {
        let (range_bars, periods) = match depth {
            SweepDepth::Quick => (vec![5], vec![OpeningPeriod::Weekly]),
            SweepDepth::Standard => (
                vec![3, 5, 10],
                vec![OpeningPeriod::Weekly, OpeningPeriod::Rolling],
            ),
            SweepDepth::Comprehensive => (
                vec![3, 5, 7, 10],
                vec![
                    OpeningPeriod::Weekly,
                    OpeningPeriod::Monthly,
                    OpeningPeriod::Rolling,
                ],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::OpeningRangeBreakout,
            enabled: true,
            params: StrategyParams::OpeningRangeBreakout {
                range_bars,
                periods,
            },
        }
    }

    /// Default Ensemble grid (presets only).
    pub fn ensemble_default() -> Self {
        Self {
            strategy_type: StrategyTypeId::Ensemble,
            enabled: true,
            params: StrategyParams::Ensemble {
                base_strategies: vec![StrategyTypeId::Donchian, StrategyTypeId::MACrossover],
                horizon_sets: vec![vec![20, 50, 100], vec![10, 21, 63]],
                voting_methods: vec![VotingMethod::Majority, VotingMethod::WeightedByHorizon],
            },
        }
    }

    /// Ensemble grid with specified sweep depth.
    ///
    /// Parameter ranges:
    /// - Quick: Donchian triple only (1 config)
    /// - Standard: Donchian + MA with 2 voting methods (4 configs)
    /// - Comprehensive: Multiple base strategies and horizon sets (12 configs)
    pub fn ensemble_with_depth(depth: SweepDepth) -> Self {
        let (base_strategies, horizon_sets, voting_methods) = match depth {
            SweepDepth::Quick => (
                vec![StrategyTypeId::Donchian],
                vec![vec![20, 50, 100]],
                vec![VotingMethod::Majority],
            ),
            SweepDepth::Standard => (
                vec![StrategyTypeId::Donchian, StrategyTypeId::MACrossover],
                vec![vec![20, 50, 100], vec![10, 21, 63]],
                vec![VotingMethod::Majority, VotingMethod::WeightedByHorizon],
            ),
            SweepDepth::Comprehensive => (
                vec![
                    StrategyTypeId::Donchian,
                    StrategyTypeId::MACrossover,
                    StrategyTypeId::Tsmom,
                ],
                vec![vec![20, 50, 100], vec![10, 21, 63], vec![21, 63, 252]],
                vec![
                    VotingMethod::Majority,
                    VotingMethod::WeightedByHorizon,
                    VotingMethod::UnanimousEntry,
                ],
            ),
        };
        Self {
            strategy_type: StrategyTypeId::Ensemble,
            enabled: true,
            params: StrategyParams::Ensemble {
                base_strategies,
                horizon_sets,
                voting_methods,
            },
        }
    }

    /// Generate all configs for this strategy.
    pub fn generate_configs(&self) -> Vec<StrategyConfigId> {
        if !self.enabled {
            return Vec::new();
        }
        self.params.generate_configs()
    }

    /// Count configurations.
    pub fn config_count(&self) -> usize {
        if !self.enabled {
            0
        } else {
            self.params.config_count()
        }
    }
}

/// Collection of strategy grids for multi-strategy sweeps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiStrategyGrid {
    pub strategies: Vec<StrategyGridConfig>,
}

impl MultiStrategyGrid {
    /// Create with all strategies using default parameters.
    ///
    /// Note: The following strategies are disabled by default due to poor
    /// backtesting performance (Sharpe < 0.15): TurtleS1, TurtleS2,
    /// DmiAdx, BollingerSqueeze.
    pub fn all_strategies_default() -> Self {
        Self {
            strategies: vec![
                StrategyGridConfig::donchian_default(),
                StrategyGridConfig::turtle_s1().disabled(), // Poor performance
                StrategyGridConfig::turtle_s2().disabled(), // Poor performance
                StrategyGridConfig::ma_crossover_default(),
                StrategyGridConfig::tsmom_default(),
                // Phase 2
                StrategyGridConfig::dmi_adx_default().disabled(), // Poor performance
                StrategyGridConfig::aroon_default(),
                StrategyGridConfig::bollinger_squeeze_default().disabled(), // Poor performance
                // Phase 4
                StrategyGridConfig::parabolic_sar_default(),
                StrategyGridConfig::orb_default(),
                StrategyGridConfig::ensemble_default(),
            ],
        }
    }

    /// Create with a single strategy.
    pub fn single_strategy(strategy_type: StrategyTypeId) -> Self {
        let config = match strategy_type {
            StrategyTypeId::Donchian => StrategyGridConfig::donchian_default(),
            StrategyTypeId::TurtleS1 => StrategyGridConfig::turtle_s1(),
            StrategyTypeId::TurtleS2 => StrategyGridConfig::turtle_s2(),
            StrategyTypeId::MACrossover => StrategyGridConfig::ma_crossover_default(),
            StrategyTypeId::Tsmom => StrategyGridConfig::tsmom_default(),
            StrategyTypeId::DmiAdx => StrategyGridConfig::dmi_adx_default(),
            StrategyTypeId::Aroon => StrategyGridConfig::aroon_default(),
            StrategyTypeId::BollingerSqueeze => StrategyGridConfig::bollinger_squeeze_default(),
            // Phase 1: ATR-Based Channels
            StrategyTypeId::Keltner => StrategyGridConfig::keltner_default(),
            StrategyTypeId::STARC => StrategyGridConfig::starc_default(),
            StrategyTypeId::Supertrend => StrategyGridConfig::supertrend_default(),
            // Phase 3: Price Structure
            StrategyTypeId::FiftyTwoWeekHigh => StrategyGridConfig::fifty_two_week_high_default(),
            StrategyTypeId::DarvasBox => StrategyGridConfig::darvas_box_default(),
            StrategyTypeId::LarryWilliams => StrategyGridConfig::larry_williams_default(),
            StrategyTypeId::HeikinAshi => StrategyGridConfig::heikin_ashi_default(),
            // Phase 4
            StrategyTypeId::ParabolicSar => StrategyGridConfig::parabolic_sar_default(),
            StrategyTypeId::OpeningRangeBreakout => StrategyGridConfig::orb_default(),
            StrategyTypeId::Ensemble => StrategyGridConfig::ensemble_default(),
            // Phase 5 oscillator strategies - not yet implemented in grid
            _ => panic!("Grid config not yet implemented for this strategy type"),
        };
        Self {
            strategies: vec![config],
        }
    }

    /// Create with all strategies using the specified sweep depth.
    ///
    /// This is the recommended way to create a multi-strategy grid with
    /// research-backed parameter ranges.
    ///
    /// Note: The following strategies are disabled by default due to poor
    /// backtesting performance (Sharpe < 0.15): TurtleS1, TurtleS2, Keltner,
    /// DmiAdx, BollingerSqueeze, DarvasBox.
    pub fn with_depth(depth: SweepDepth) -> Self {
        Self {
            strategies: vec![
                // Original strategies
                StrategyGridConfig::donchian_with_depth(depth),
                StrategyGridConfig::turtle_s1().disabled(), // Poor performance
                StrategyGridConfig::turtle_s2().disabled(), // Poor performance
                StrategyGridConfig::ma_crossover_with_depth(depth),
                StrategyGridConfig::tsmom_with_depth(depth),
                // Phase 1: ATR-Based Channels
                StrategyGridConfig::keltner_with_depth(depth).disabled(), // Poor performance
                StrategyGridConfig::starc_with_depth(depth),
                StrategyGridConfig::supertrend_with_depth(depth),
                StrategyGridConfig::supertrend_volume_with_depth(depth),
                StrategyGridConfig::supertrend_confirmed_with_depth(depth),
                StrategyGridConfig::supertrend_asymmetric_with_depth(depth),
                StrategyGridConfig::supertrend_cooldown_with_depth(depth),
                // Phase 2: Momentum/Direction
                StrategyGridConfig::dmi_adx_with_depth(depth).disabled(), // Poor performance
                StrategyGridConfig::aroon_with_depth(depth),
                StrategyGridConfig::bollinger_squeeze_with_depth(depth).disabled(), // Poor performance
                // Phase 3: Price Structure
                StrategyGridConfig::fifty_two_week_high_with_depth(depth),
                StrategyGridConfig::fifty_two_week_high_momentum_with_depth(depth),
                StrategyGridConfig::fifty_two_week_high_trailing_with_depth(depth),
                StrategyGridConfig::darvas_box_with_depth(depth).disabled(), // Poor performance
                StrategyGridConfig::larry_williams_with_depth(depth),
                StrategyGridConfig::heikin_ashi_with_depth(depth),
                // Phase 4: Complex Stateful
                StrategyGridConfig::parabolic_sar_with_depth(depth),
                StrategyGridConfig::parabolic_sar_filtered_with_depth(depth),
                StrategyGridConfig::parabolic_sar_delayed_with_depth(depth),
                StrategyGridConfig::orb_with_depth(depth),
                StrategyGridConfig::ensemble_with_depth(depth),
            ],
        }
    }

    /// Create with a single strategy at specified sweep depth.
    pub fn single_strategy_with_depth(strategy_type: StrategyTypeId, depth: SweepDepth) -> Self {
        let config = match strategy_type {
            StrategyTypeId::Donchian => StrategyGridConfig::donchian_with_depth(depth),
            StrategyTypeId::TurtleS1 => StrategyGridConfig::turtle_s1(),
            StrategyTypeId::TurtleS2 => StrategyGridConfig::turtle_s2(),
            StrategyTypeId::MACrossover => StrategyGridConfig::ma_crossover_with_depth(depth),
            StrategyTypeId::Tsmom => StrategyGridConfig::tsmom_with_depth(depth),
            StrategyTypeId::DmiAdx => StrategyGridConfig::dmi_adx_with_depth(depth),
            StrategyTypeId::Aroon => StrategyGridConfig::aroon_with_depth(depth),
            StrategyTypeId::BollingerSqueeze => {
                StrategyGridConfig::bollinger_squeeze_with_depth(depth)
            }
            // Phase 1: ATR-Based Channels
            StrategyTypeId::Keltner => StrategyGridConfig::keltner_with_depth(depth),
            StrategyTypeId::STARC => StrategyGridConfig::starc_with_depth(depth),
            StrategyTypeId::Supertrend => StrategyGridConfig::supertrend_with_depth(depth),
            StrategyTypeId::SupertrendVolume => {
                StrategyGridConfig::supertrend_volume_with_depth(depth)
            }
            StrategyTypeId::SupertrendConfirmed => {
                StrategyGridConfig::supertrend_confirmed_with_depth(depth)
            }
            StrategyTypeId::SupertrendAsymmetric => {
                StrategyGridConfig::supertrend_asymmetric_with_depth(depth)
            }
            StrategyTypeId::SupertrendCooldown => {
                StrategyGridConfig::supertrend_cooldown_with_depth(depth)
            }
            // Phase 3: Price Structure
            StrategyTypeId::FiftyTwoWeekHigh => {
                StrategyGridConfig::fifty_two_week_high_with_depth(depth)
            }
            StrategyTypeId::FiftyTwoWeekHighMomentum => {
                StrategyGridConfig::fifty_two_week_high_momentum_with_depth(depth)
            }
            StrategyTypeId::FiftyTwoWeekHighTrailing => {
                StrategyGridConfig::fifty_two_week_high_trailing_with_depth(depth)
            }
            StrategyTypeId::DarvasBox => StrategyGridConfig::darvas_box_with_depth(depth),
            StrategyTypeId::LarryWilliams => StrategyGridConfig::larry_williams_with_depth(depth),
            StrategyTypeId::HeikinAshi => StrategyGridConfig::heikin_ashi_with_depth(depth),
            // Phase 4: Complex Stateful
            StrategyTypeId::ParabolicSar => StrategyGridConfig::parabolic_sar_with_depth(depth),
            StrategyTypeId::ParabolicSarFiltered => {
                StrategyGridConfig::parabolic_sar_filtered_with_depth(depth)
            }
            StrategyTypeId::ParabolicSarDelayed => {
                StrategyGridConfig::parabolic_sar_delayed_with_depth(depth)
            }
            StrategyTypeId::OpeningRangeBreakout => StrategyGridConfig::orb_with_depth(depth),
            StrategyTypeId::Ensemble => StrategyGridConfig::ensemble_with_depth(depth),
            // Phase 5 oscillator strategies - not yet implemented in grid
            _ => panic!("Grid config not yet implemented for this strategy type"),
        };
        Self {
            strategies: vec![config],
        }
    }

    /// Total number of configurations across all strategies.
    pub fn total_configs(&self) -> usize {
        self.strategies.iter().map(|s| s.config_count()).sum()
    }

    /// Get enabled strategies.
    pub fn enabled_strategies(&self) -> Vec<&StrategyGridConfig> {
        self.strategies.iter().filter(|s| s.enabled).collect()
    }
}

/// Best result for a strategy (either per-symbol or aggregate).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyBestResult {
    pub strategy_type: StrategyTypeId,
    pub config_id: StrategyConfigId,
    pub symbol: Option<String>, // None for aggregate
    pub metrics: Metrics,
    pub equity_curve: Vec<f64>,
    pub dates: Vec<chrono::DateTime<chrono::Utc>>,
}

/// Comparison entry summarizing a strategy's performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyComparisonEntry {
    pub strategy_type: StrategyTypeId,
    pub total_configs_tested: usize,
    pub avg_cagr: f64,
    pub avg_sharpe: f64,
    pub best_sharpe: f64,
    pub worst_drawdown: f64,
}

/// Result for a multi-strategy sweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiStrategySweepResult {
    pub sweep_id: String,
    /// Results indexed by (symbol, strategy_type)
    pub results: HashMap<(String, StrategyTypeId), SweepResult>,
    /// Best strategy per symbol
    pub best_per_symbol: HashMap<String, StrategyBestResult>,
    /// Best config per strategy across all symbols
    pub best_per_strategy: HashMap<StrategyTypeId, StrategyBestResult>,
    /// Strategy comparison: aggregated metrics per strategy
    pub strategy_comparison: Vec<StrategyComparisonEntry>,
    /// When the sweep started
    pub started_at: DateTime<Utc>,
    /// When the sweep completed
    pub completed_at: DateTime<Utc>,
}

impl MultiStrategySweepResult {
    /// Create a new multi-strategy sweep result.
    pub fn new(sweep_id: String) -> Self {
        Self {
            sweep_id,
            results: HashMap::new(),
            best_per_symbol: HashMap::new(),
            best_per_strategy: HashMap::new(),
            strategy_comparison: Vec::new(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
        }
    }

    /// Add a result for a symbol/strategy combination.
    pub fn add_result(
        &mut self,
        symbol: String,
        strategy_type: StrategyTypeId,
        result: SweepResult,
    ) {
        self.results.insert((symbol, strategy_type), result);
    }

    /// Compute aggregations after all results are added.
    pub fn compute_aggregations(&mut self) {
        self.compute_best_per_symbol();
        self.compute_best_per_strategy();
        self.compute_strategy_comparison();
        self.completed_at = Utc::now();
    }

    fn compute_best_per_symbol(&mut self) {
        // Group results by symbol
        let mut by_symbol: HashMap<String, Vec<(&StrategyTypeId, &SweepResult)>> = HashMap::new();
        for ((symbol, strategy_type), result) in &self.results {
            by_symbol
                .entry(symbol.clone())
                .or_default()
                .push((strategy_type, result));
        }

        // Find best across all strategies for each symbol
        // Two-tier selection: prefer configs that actually trade (num_trades > 0),
        // but don't require a minimum trade count.
        for (symbol, results) in by_symbol {
            let mut best_trading: Option<(&StrategyTypeId, &SweepConfigResult)> = None;
            let mut best_any: Option<(&StrategyTypeId, &SweepConfigResult)> = None;

            for (strategy_type, sweep_result) in results {
                for config_result in &sweep_result.config_results {
                    // Track best among configs that actually trade
                    if config_result.metrics.num_trades > 0
                        && (best_trading.is_none()
                            || config_result.metrics.sharpe
                                > best_trading.unwrap().1.metrics.sharpe)
                    {
                        best_trading = Some((strategy_type, config_result));
                    }
                    // Track best overall (including no-trade configs)
                    if best_any.is_none()
                        || config_result.metrics.sharpe > best_any.unwrap().1.metrics.sharpe
                    {
                        best_any = Some((strategy_type, config_result));
                    }
                }
            }

            // Prefer trading configs; fall back to best overall if none trade
            let best = best_trading.or(best_any);

            if let Some((strategy_type, config_result)) = best {
                let equity_curve: Vec<f64> = config_result
                    .backtest_result
                    .equity
                    .iter()
                    .map(|p| p.equity)
                    .collect();
                let dates: Vec<chrono::DateTime<chrono::Utc>> = config_result
                    .backtest_result
                    .equity
                    .iter()
                    .map(|p| p.ts)
                    .collect();
                self.best_per_symbol.insert(
                    symbol.clone(),
                    StrategyBestResult {
                        strategy_type: *strategy_type,
                        config_id: config_id_to_strategy_config_id(
                            &config_result.config_id,
                            *strategy_type,
                        ),
                        symbol: Some(symbol),
                        metrics: config_result.metrics.clone(),
                        equity_curve,
                        dates,
                    },
                );
            }
        }
    }

    fn compute_best_per_strategy(&mut self) {
        // Group results by strategy type
        let mut by_strategy: HashMap<StrategyTypeId, Vec<(&String, &SweepResult)>> = HashMap::new();
        for ((symbol, strategy_type), result) in &self.results {
            by_strategy
                .entry(*strategy_type)
                .or_default()
                .push((symbol, result));
        }

        // Find best config per strategy across all symbols
        // Two-tier selection: prefer configs that actually trade (num_trades > 0),
        // but don't require a minimum trade count. If no configs trade, fall back
        // to best overall (which preserves capital by not trading).
        for (strategy_type, results) in by_strategy {
            let mut best_trading: Option<(&String, &SweepConfigResult)> = None;
            let mut best_any: Option<(&String, &SweepConfigResult)> = None;

            for (symbol, sweep_result) in results {
                for config_result in &sweep_result.config_results {
                    // Track best among configs that actually trade
                    if config_result.metrics.num_trades > 0
                        && (best_trading.is_none()
                            || config_result.metrics.sharpe
                                > best_trading.unwrap().1.metrics.sharpe)
                    {
                        best_trading = Some((symbol, config_result));
                    }
                    // Track best overall (including no-trade configs)
                    if best_any.is_none()
                        || config_result.metrics.sharpe > best_any.unwrap().1.metrics.sharpe
                    {
                        best_any = Some((symbol, config_result));
                    }
                }
            }

            // Prefer trading configs; fall back to best overall if none trade
            let best = best_trading.or(best_any);

            if let Some((symbol, config_result)) = best {
                let equity_curve: Vec<f64> = config_result
                    .backtest_result
                    .equity
                    .iter()
                    .map(|p| p.equity)
                    .collect();
                let dates: Vec<chrono::DateTime<chrono::Utc>> = config_result
                    .backtest_result
                    .equity
                    .iter()
                    .map(|p| p.ts)
                    .collect();
                self.best_per_strategy.insert(
                    strategy_type,
                    StrategyBestResult {
                        strategy_type,
                        config_id: config_id_to_strategy_config_id(
                            &config_result.config_id,
                            strategy_type,
                        ),
                        symbol: Some(symbol.clone()),
                        metrics: config_result.metrics.clone(),
                        equity_curve,
                        dates,
                    },
                );
            }
        }
    }

    fn compute_strategy_comparison(&mut self) {
        // Group results by strategy type
        let mut by_strategy: HashMap<StrategyTypeId, Vec<&Metrics>> = HashMap::new();
        for ((_, strategy_type), result) in &self.results {
            for config_result in &result.config_results {
                by_strategy
                    .entry(*strategy_type)
                    .or_default()
                    .push(&config_result.metrics);
            }
        }

        // Compute aggregate stats per strategy
        for (strategy_type, metrics_list) in by_strategy {
            if metrics_list.is_empty() {
                continue;
            }
            let n = metrics_list.len() as f64;
            let avg_cagr = metrics_list.iter().map(|m| m.cagr).sum::<f64>() / n;
            let avg_sharpe = metrics_list.iter().map(|m| m.sharpe).sum::<f64>() / n;
            let best_sharpe = metrics_list
                .iter()
                .map(|m| m.sharpe)
                .fold(f64::NEG_INFINITY, f64::max);
            let worst_drawdown = metrics_list
                .iter()
                .map(|m| m.max_drawdown)
                .fold(0.0_f64, f64::min); // More negative = worse

            self.strategy_comparison.push(StrategyComparisonEntry {
                strategy_type,
                total_configs_tested: metrics_list.len(),
                avg_cagr,
                avg_sharpe,
                best_sharpe,
                worst_drawdown,
            });
        }

        // Sort by best sharpe descending
        self.strategy_comparison.sort_by(|a, b| {
            b.best_sharpe
                .partial_cmp(&a.best_sharpe)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// Convert legacy ConfigId to StrategyConfigId (for backwards compatibility).
fn config_id_to_strategy_config_id(
    config_id: &ConfigId,
    strategy_type: StrategyTypeId,
) -> StrategyConfigId {
    match strategy_type {
        StrategyTypeId::Donchian => StrategyConfigId::Donchian {
            entry_lookback: config_id.entry_lookback,
            exit_lookback: config_id.exit_lookback,
        },
        StrategyTypeId::TurtleS1 => StrategyConfigId::TurtleS1,
        StrategyTypeId::TurtleS2 => StrategyConfigId::TurtleS2,
        StrategyTypeId::MACrossover => StrategyConfigId::MACrossover {
            fast: config_id.entry_lookback,
            slow: config_id.exit_lookback,
            ma_type: MAType::SMA, // Default, actual type stored elsewhere
        },
        StrategyTypeId::Tsmom => StrategyConfigId::Tsmom {
            lookback: config_id.entry_lookback,
        },
        // Phase 2: Momentum & Direction
        // Note: These use legacy ConfigId layout where entry_lookback=di_period/period,
        // exit_lookback=adx_period/0. Thresholds use defaults.
        StrategyTypeId::DmiAdx => StrategyConfigId::DmiAdx {
            di_period: config_id.entry_lookback,
            adx_period: config_id.exit_lookback,
            adx_threshold: 25.0, // Default
        },
        StrategyTypeId::Aroon => StrategyConfigId::Aroon {
            period: config_id.entry_lookback,
        },
        StrategyTypeId::BollingerSqueeze => StrategyConfigId::BollingerSqueeze {
            period: config_id.entry_lookback,
            std_mult: 2.0,           // Default
            squeeze_threshold: 0.04, // Default
        },
        // Phase 4: Complex Stateful + Ensemble
        StrategyTypeId::ParabolicSar => StrategyConfigId::ParabolicSar {
            af_start: (config_id.entry_lookback as f64) / 100.0,
            af_step: 0.02, // Default
            af_max: 0.20,  // Default
        },
        StrategyTypeId::OpeningRangeBreakout => StrategyConfigId::OpeningRangeBreakout {
            range_bars: config_id.entry_lookback,
            period: OpeningPeriod::Weekly, // Default
        },
        StrategyTypeId::Ensemble => StrategyConfigId::Ensemble {
            base_strategy: StrategyTypeId::Donchian,
            horizons: vec![20, 50, 100], // Default
            voting: VotingMethod::Majority,
        },
        // Phase 1: ATR-Based Channels
        // Note: Legacy ConfigId layout maps entry_lookback=ema/sma period, exit_lookback=atr period
        StrategyTypeId::Keltner => StrategyConfigId::Keltner {
            ema_period: config_id.entry_lookback,
            atr_period: config_id.exit_lookback,
            multiplier: 2.0, // Default
        },
        StrategyTypeId::STARC => StrategyConfigId::STARC {
            sma_period: config_id.entry_lookback,
            atr_period: config_id.exit_lookback,
            multiplier: 2.0, // Default
        },
        StrategyTypeId::Supertrend => StrategyConfigId::Supertrend {
            atr_period: config_id.entry_lookback,
            multiplier: 3.0, // Default
        },
        // Phase 3: Price Structure
        StrategyTypeId::FiftyTwoWeekHigh => StrategyConfigId::FiftyTwoWeekHigh {
            period: config_id.entry_lookback,
            entry_pct: 0.95, // Default
            exit_pct: 0.90,  // Default
        },
        StrategyTypeId::DarvasBox => StrategyConfigId::DarvasBox {
            box_confirmation_bars: config_id.entry_lookback.max(1),
        },
        StrategyTypeId::LarryWilliams => StrategyConfigId::LarryWilliams {
            range_multiplier: 0.5, // Default
            atr_stop_mult: 2.0,    // Default
        },
        StrategyTypeId::HeikinAshi => StrategyConfigId::HeikinAshi {
            confirmation_bars: config_id.entry_lookback.max(1),
        },
        // Phase 5 oscillator strategies - not yet implemented
        _ => panic!("StrategyConfigId conversion not yet implemented for this strategy type"),
    }
}

/// Create a strategy instance from a config.
pub fn create_strategy_from_config(config: &StrategyConfigId) -> Box<dyn Strategy> {
    match config {
        StrategyConfigId::Donchian {
            entry_lookback,
            exit_lookback,
        } => Box::new(DonchianBreakoutStrategy::new(
            *entry_lookback,
            *exit_lookback,
        )),
        StrategyConfigId::TurtleS1 => Box::new(DonchianBreakoutStrategy::turtle_system_1()),
        StrategyConfigId::TurtleS2 => Box::new(DonchianBreakoutStrategy::turtle_system_2()),
        StrategyConfigId::MACrossover {
            fast,
            slow,
            ma_type,
        } => Box::new(MACrossoverStrategy::new(*fast, *slow, *ma_type)),
        StrategyConfigId::Tsmom { lookback } => Box::new(TsmomStrategy::new(*lookback)),
        // Phase 2: Momentum & Direction
        StrategyConfigId::DmiAdx {
            di_period,
            adx_period,
            adx_threshold,
        } => Box::new(DmiAdxStrategy::new(*di_period, *adx_period, *adx_threshold)),
        StrategyConfigId::Aroon { period } => Box::new(AroonCrossStrategy::new(*period)),
        StrategyConfigId::BollingerSqueeze {
            period,
            std_mult,
            squeeze_threshold,
        } => Box::new(BollingerSqueezeStrategy::new(
            *period,
            *std_mult,
            *squeeze_threshold,
        )),
        // Phase 3: Price Structure - implemented strategies
        StrategyConfigId::FiftyTwoWeekHigh {
            period,
            entry_pct,
            exit_pct,
        } => Box::new(FiftyTwoWeekHighStrategy::new(
            *period, *entry_pct, *exit_pct,
        )),
        // FiftyTwoWeekHigh variants
        StrategyConfigId::FiftyTwoWeekHighMomentum {
            period,
            entry_pct,
            exit_pct,
            momentum_period,
            momentum_threshold,
        } => Box::new(FiftyTwoWeekHighMomentumStrategy::new(
            *period,
            *entry_pct,
            *exit_pct,
            *momentum_period,
            *momentum_threshold,
        )),
        StrategyConfigId::FiftyTwoWeekHighTrailing {
            period,
            entry_pct,
            trailing_stop_pct,
        } => Box::new(FiftyTwoWeekHighTrailingStrategy::new(
            *period,
            *entry_pct,
            *trailing_stop_pct,
        )),
        StrategyConfigId::DarvasBox {
            box_confirmation_bars,
        } => Box::new(DarvasBoxStrategy::new(*box_confirmation_bars)),
        StrategyConfigId::LarryWilliams {
            range_multiplier,
            atr_stop_mult,
        } => Box::new(LarryWilliamsStrategy::new(
            *range_multiplier,
            *atr_stop_mult,
            10,
        )),
        StrategyConfigId::HeikinAshi { confirmation_bars } => {
            Box::new(HeikinAshiRegimeStrategy::new(*confirmation_bars))
        }
        // Phase 4: Complex Stateful + Ensemble
        StrategyConfigId::ParabolicSar {
            af_start,
            af_step,
            af_max,
        } => Box::new(ParabolicSARStrategy::new(*af_start, *af_step, *af_max)),
        // ParabolicSar variants
        StrategyConfigId::ParabolicSarFiltered {
            af_start,
            af_step,
            af_max,
            trend_ma_period,
        } => Box::new(ParabolicSarFilteredStrategy::new(
            *af_start,
            *af_step,
            *af_max,
            *trend_ma_period,
        )),
        StrategyConfigId::ParabolicSarDelayed {
            af_start,
            af_step,
            af_max,
            delay_bars,
        } => Box::new(ParabolicSarDelayedStrategy::new(
            *af_start,
            *af_step,
            *af_max,
            *delay_bars,
        )),
        StrategyConfigId::OpeningRangeBreakout { range_bars, period } => {
            Box::new(OpeningRangeBreakoutStrategy::new(*range_bars, *period))
        }
        StrategyConfigId::Ensemble {
            base_strategy,
            horizons,
            voting,
        } => Box::new(EnsembleStrategy::from_base_strategy(
            *base_strategy,
            horizons.clone(),
            *voting,
        )),
        // Phase 1: ATR-Based Channels
        StrategyConfigId::Keltner {
            ema_period,
            atr_period,
            multiplier,
        } => Box::new(KeltnerBreakoutStrategy::new(
            *ema_period,
            *atr_period,
            *multiplier,
        )),
        StrategyConfigId::STARC {
            sma_period,
            atr_period,
            multiplier,
        } => Box::new(STARCBreakoutStrategy::new(
            *sma_period,
            *atr_period,
            *multiplier,
        )),
        StrategyConfigId::Supertrend {
            atr_period,
            multiplier,
        } => Box::new(SupertrendStrategy::new(*atr_period, *multiplier)),
        // Supertrend variants
        StrategyConfigId::SupertrendVolume {
            atr_period,
            multiplier,
            volume_lookback,
            volume_threshold_pct,
        } => Box::new(SupertrendVolumeStrategy::new(
            *atr_period,
            *multiplier,
            *volume_lookback,
            *volume_threshold_pct,
        )),
        StrategyConfigId::SupertrendConfirmed {
            atr_period,
            multiplier,
            confirmation_bars,
        } => Box::new(SupertrendConfirmedStrategy::new(
            *atr_period,
            *multiplier,
            *confirmation_bars,
        )),
        StrategyConfigId::SupertrendAsymmetric {
            atr_period,
            entry_multiplier,
            exit_multiplier,
        } => Box::new(SupertrendAsymmetricStrategy::new(
            *atr_period,
            *entry_multiplier,
            *exit_multiplier,
        )),
        StrategyConfigId::SupertrendCooldown {
            atr_period,
            multiplier,
            cooldown_bars,
        } => Box::new(SupertrendCooldownStrategy::new(
            *atr_period,
            *multiplier,
            *cooldown_bars,
        )),
        // Phase 5: Oscillator Strategies
        StrategyConfigId::Rsi {
            period,
            oversold,
            overbought,
        } => Box::new(RSIStrategy::new(*period, *oversold, *overbought)),
        StrategyConfigId::Macd {
            fast_period,
            slow_period,
            signal_period,
            entry_mode,
        } => Box::new(MACDStrategy::new(
            *fast_period,
            *slow_period,
            *signal_period,
            *entry_mode,
        )),
        StrategyConfigId::Stochastic {
            k_period,
            k_smooth,
            d_period,
            oversold,
            overbought,
        } => Box::new(StochasticStrategy::new(
            *k_period,
            *k_smooth,
            *d_period,
            *oversold,
            *overbought,
        )),
        StrategyConfigId::WilliamsR {
            period,
            oversold,
            overbought,
        } => Box::new(WilliamsRStrategy::new(*period, *oversold, *overbought)),
        StrategyConfigId::Cci {
            period,
            entry_threshold,
            exit_threshold,
        } => Box::new(CCIStrategy::new(*period, *entry_threshold, *exit_threshold)),
        StrategyConfigId::Roc { period } => Box::new(ROCStrategy::new(*period)),
        StrategyConfigId::RsiBollinger {
            rsi_period,
            rsi_oversold,
            rsi_exit,
            bb_period,
            bb_std_mult,
        } => Box::new(RSIBollingerStrategy::new(
            *rsi_period,
            *rsi_oversold,
            *rsi_exit,
            *bb_period,
            *bb_std_mult,
        )),
        StrategyConfigId::MacdAdx {
            fast_period,
            slow_period,
            signal_period,
            adx_period,
            adx_threshold,
        } => Box::new(MACDAdxStrategy::new(
            *fast_period,
            *slow_period,
            *signal_period,
            *adx_period,
            *adx_threshold,
        )),
        StrategyConfigId::OscillatorConfluence {
            rsi_period,
            rsi_oversold,
            rsi_overbought,
            stoch_k_period,
            stoch_k_smooth,
            stoch_d_period,
            stoch_oversold,
            stoch_overbought,
        } => Box::new(OscillatorConfluenceStrategy::new(
            *rsi_period,
            *rsi_oversold,
            *rsi_overbought,
            *stoch_k_period,
            *stoch_k_smooth,
            *stoch_d_period,
            *stoch_oversold,
            *stoch_overbought,
        )),
        StrategyConfigId::Ichimoku {
            tenkan_period,
            kijun_period,
            senkou_b_period,
        } => Box::new(IchimokuStrategy::new(
            *tenkan_period,
            *kijun_period,
            *senkou_b_period,
        )),
    }
}

/// Run a single backtest with a specific config.
pub fn run_single_config_backtest(
    bars: &[Bar],
    config: &StrategyConfigId,
    backtest_config: BacktestConfig,
) -> Option<SweepConfigResult> {
    let mut strategy = create_strategy_from_config(config);
    let backtest_result = run_backtest(bars, &mut *strategy, backtest_config).ok()?;
    let metrics = compute_metrics(&backtest_result, backtest_config.initial_cash);
    Some(SweepConfigResult {
        config_id: config.to_legacy_config_id(),
        backtest_result,
        metrics,
    })
}

/// Run a sweep for a single strategy type on given bars.
pub fn run_strategy_sweep(
    bars: &[Bar],
    strategy_config: &StrategyGridConfig,
    backtest_config: BacktestConfig,
) -> SweepResult {
    let sweep_id = format!(
        "{}_{}_{}",
        strategy_config.strategy_type.id(),
        bars.first().map(|b| b.symbol.clone()).unwrap_or_default(),
        Utc::now().format("%Y%m%d_%H%M%S")
    );
    let started_at = Utc::now();

    let configs = strategy_config.generate_configs();

    let config_results: Vec<SweepConfigResult> = configs
        .par_iter()
        .filter_map(|config| run_single_config_backtest(bars, config, backtest_config))
        .collect();

    let completed_at = Utc::now();

    SweepResult {
        sweep_id,
        config_results,
        started_at,
        completed_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_test_bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
                    + chrono::Duration::days(i as i64);
                let price = 100.0 + (i as f64).sin() * 10.0;
                Bar::new(
                    ts,
                    price,
                    price + 2.0,
                    price - 2.0,
                    price + 1.0,
                    1000.0,
                    "TEST",
                    "1d",
                )
            })
            .collect()
    }

    #[test]
    fn test_sweep_grid_combinations() {
        let grid = SweepGrid::new(vec![10, 20], vec![5, 10]);
        let combos = grid.combinations();
        assert_eq!(combos.len(), 4);
        assert!(combos.contains(&(10, 5)));
        assert!(combos.contains(&(10, 10)));
        assert!(combos.contains(&(20, 5)));
        assert!(combos.contains(&(20, 10)));
    }

    #[test]
    fn test_run_sweep() {
        let bars = make_test_bars(100);
        let grid = SweepGrid::new(vec![10, 20], vec![5, 10]);
        let config = BacktestConfig::default();

        let result = run_sweep(&bars, &grid, config);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_ranking() {
        let bars = make_test_bars(100);
        let grid = SweepGrid::new(vec![10, 15, 20], vec![5, 8, 10]);
        let config = BacktestConfig::default();

        let result = run_sweep(&bars, &grid, config);
        let top3 = result.top_n(3, RankMetric::Sharpe, false);
        assert_eq!(top3.len(), 3);

        // Verify descending order
        for i in 0..top3.len() - 1 {
            assert!(top3[i].metrics.sharpe >= top3[i + 1].metrics.sharpe);
        }
    }

    #[test]
    fn test_result_paths() {
        let paths = ResultPaths::for_sweep("test_sweep_001");
        assert!(paths.manifest.to_string_lossy().contains("test_sweep_001"));
        assert!(paths.manifest.to_string_lossy().ends_with("manifest.json"));
    }
}
