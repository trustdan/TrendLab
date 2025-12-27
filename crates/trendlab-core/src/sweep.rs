//! Parameter sweep infrastructure for systematic strategy exploration.
//!
//! This module provides:
//! - Sweep grid definition and iteration
//! - Parallel sweep execution with rayon
//! - Run manifests for reproducibility
//! - Ranking and stability analysis

use crate::backtest::{run_backtest, BacktestConfig, BacktestResult, CostModel};
use crate::bar::Bar;
use crate::indicators::MAType;
use crate::metrics::{compute_metrics, Metrics};
use crate::strategy::{DonchianBreakoutStrategy, MACrossoverStrategy, Strategy, TsmomStrategy};
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

    let config_results: Vec<SweepConfigResult> = combinations
        .par_iter()
        .map(|&(entry, exit)| {
            let config_id = ConfigId::new(entry, exit);
            let mut strategy = DonchianBreakoutStrategy::new(entry, exit);

            let backtest_result =
                run_backtest(bars, &mut strategy, backtest_config).expect("Backtest failed");

            let metrics = compute_metrics(&backtest_result, backtest_config.initial_cash);

            SweepConfigResult {
                config_id,
                backtest_result,
                metrics,
            }
        })
        .collect();

    let completed_at = Utc::now();

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
pub fn compute_cost_sensitivity(
    bars: &[Bar],
    config_id: &ConfigId,
    base_config: BacktestConfig,
    cost_levels_bps: &[f64],
) -> CostSensitivity {
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
        let result = run_backtest(bars, &mut strategy, config).expect("Backtest failed");
        let metrics = compute_metrics(&result, config.initial_cash);

        returns_at_cost.push(metrics.total_return);

        // Check for breakeven (returns going negative)
        if breakeven_cost_bps.is_none() && metrics.total_return <= 0.0 {
            breakeven_cost_bps = Some(cost_bps);
        }
    }

    CostSensitivity {
        config_id: config_id.clone(),
        cost_levels: cost_levels_bps.to_vec(),
        returns_at_cost,
        breakeven_cost_bps,
    }
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
        }
    }
}

/// Generic config identifier for any strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub fn all_strategies_default() -> Self {
        Self {
            strategies: vec![
                StrategyGridConfig::donchian_default(),
                StrategyGridConfig::turtle_s1(),
                StrategyGridConfig::turtle_s2(),
                StrategyGridConfig::ma_crossover_default(),
                StrategyGridConfig::tsmom_default(),
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
        };
        Self {
            strategies: vec![config],
        }
    }

    /// Create with all strategies using the specified sweep depth.
    ///
    /// This is the recommended way to create a multi-strategy grid with
    /// research-backed parameter ranges.
    pub fn with_depth(depth: SweepDepth) -> Self {
        Self {
            strategies: vec![
                StrategyGridConfig::donchian_with_depth(depth),
                StrategyGridConfig::turtle_s1(), // Fixed params, no depth
                StrategyGridConfig::turtle_s2(), // Fixed params, no depth
                StrategyGridConfig::ma_crossover_with_depth(depth),
                StrategyGridConfig::tsmom_with_depth(depth),
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
                    if config_result.metrics.num_trades > 0 {
                        if best_trading.is_none()
                            || config_result.metrics.sharpe
                                > best_trading.unwrap().1.metrics.sharpe
                        {
                            best_trading = Some((strategy_type, config_result));
                        }
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
                    if config_result.metrics.num_trades > 0 {
                        if best_trading.is_none()
                            || config_result.metrics.sharpe > best_trading.unwrap().1.metrics.sharpe
                        {
                            best_trading = Some((symbol, config_result));
                        }
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
