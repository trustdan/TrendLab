//! Parameter sweep infrastructure for systematic strategy exploration.
//!
//! This module provides:
//! - Sweep grid definition and iteration
//! - Parallel sweep execution with rayon
//! - Run manifests for reproducibility
//! - Ranking and stability analysis

use crate::backtest::{run_backtest, BacktestConfig, BacktestResult, CostModel};
use crate::bar::Bar;
use crate::metrics::{compute_metrics, Metrics};
use crate::strategy::DonchianBreakoutStrategy;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        let min_after_max = portfolio_equity
            .iter()
            .cloned()
            .fold(max_equity, f64::min);
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
