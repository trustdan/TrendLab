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
    DonchianBreakoutStrategy, EnsembleStrategy, FiftyTwoWeekHighStrategy, HeikinAshiRegimeStrategy,
    IchimokuStrategy, KeltnerBreakoutStrategy, LarryWilliamsStrategy, MACDAdxStrategy,
    MACDStrategy, MACrossoverStrategy, OpeningRangeBreakoutStrategy, OscillatorConfluenceStrategy,
    ParabolicSARStrategy, ROCStrategy, RSIBollingerStrategy, RSIStrategy, STARCBreakoutStrategy,
    StochasticStrategy, Strategy, SupertrendStrategy, TsmomStrategy, VotingMethod,
    WilliamsRStrategy,
};
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
        .map(|&(entry, exit)| {
            let config_id = ConfigId::new(entry, exit);
            let mut strategy = DonchianBreakoutStrategy::new(entry, exit);

            tracing::trace!(entry = entry, exit = exit, "Evaluating config");

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
    // Phase 1: ATR-Based Channels
    Keltner,
    STARC,
    Supertrend,
    // Phase 2: Momentum & Direction
    DmiAdx,
    Aroon,
    BollingerSqueeze,
    // Phase 3: Price Structure
    FiftyTwoWeekHigh,
    DarvasBox,
    LarryWilliams,
    HeikinAshi,
    // Phase 4: Complex Stateful + Ensemble
    ParabolicSar,
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
            // Phase 2
            Self::DmiAdx,
            Self::Aroon,
            Self::BollingerSqueeze,
            // Phase 3
            Self::FiftyTwoWeekHigh,
            Self::DarvasBox,
            Self::LarryWilliams,
            Self::HeikinAshi,
            // Phase 4
            Self::ParabolicSar,
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
            Self::DmiAdx => "DMI/ADX Directional",
            Self::Aroon => "Aroon Cross",
            Self::BollingerSqueeze => "Bollinger Squeeze",
            Self::FiftyTwoWeekHigh => "52-Week High",
            Self::DarvasBox => "Darvas Box",
            Self::LarryWilliams => "Larry Williams",
            Self::HeikinAshi => "Heikin-Ashi Regime",
            Self::ParabolicSar => "Parabolic SAR",
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
            Self::DmiAdx => "dmi_adx",
            Self::Aroon => "aroon",
            Self::BollingerSqueeze => "bollinger_squeeze",
            Self::FiftyTwoWeekHigh => "52wk_high",
            Self::DarvasBox => "darvas_box",
            Self::LarryWilliams => "larry_williams",
            Self::HeikinAshi => "heikin_ashi",
            Self::ParabolicSar => "parabolic_sar",
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
    // Phase 3: Price Structure
    FiftyTwoWeekHigh {
        period: usize,
        entry_pct: f64,
        exit_pct: f64,
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
            // Phase 3
            Self::FiftyTwoWeekHigh { .. } => StrategyTypeId::FiftyTwoWeekHigh,
            Self::DarvasBox { .. } => StrategyTypeId::DarvasBox,
            Self::LarryWilliams { .. } => StrategyTypeId::LarryWilliams,
            Self::HeikinAshi { .. } => StrategyTypeId::HeikinAshi,
            // Phase 4
            Self::ParabolicSar { .. } => StrategyTypeId::ParabolicSar,
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
            // Phase 3
            Self::FiftyTwoWeekHigh { period, .. } => ConfigId::new(*period, 0),
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
    // Phase 3: Price Structure
    FiftyTwoWeekHigh {
        periods: Vec<usize>,
        entry_pcts: Vec<f64>,
        exit_pcts: Vec<f64>,
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
    pub fn all_strategies_default() -> Self {
        Self {
            strategies: vec![
                StrategyGridConfig::donchian_default(),
                StrategyGridConfig::turtle_s1(),
                StrategyGridConfig::turtle_s2(),
                StrategyGridConfig::ma_crossover_default(),
                StrategyGridConfig::tsmom_default(),
                // Phase 2
                StrategyGridConfig::dmi_adx_default(),
                StrategyGridConfig::aroon_default(),
                StrategyGridConfig::bollinger_squeeze_default(),
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
    pub fn with_depth(depth: SweepDepth) -> Self {
        Self {
            strategies: vec![
                // Original strategies
                StrategyGridConfig::donchian_with_depth(depth),
                StrategyGridConfig::turtle_s1(), // Fixed params, no depth
                StrategyGridConfig::turtle_s2(), // Fixed params, no depth
                StrategyGridConfig::ma_crossover_with_depth(depth),
                StrategyGridConfig::tsmom_with_depth(depth),
                // Phase 1: ATR-Based Channels
                StrategyGridConfig::keltner_with_depth(depth),
                StrategyGridConfig::starc_with_depth(depth),
                StrategyGridConfig::supertrend_with_depth(depth),
                // Phase 2: Momentum/Direction
                StrategyGridConfig::dmi_adx_with_depth(depth),
                StrategyGridConfig::aroon_with_depth(depth),
                StrategyGridConfig::bollinger_squeeze_with_depth(depth),
                // Phase 3: Price Structure
                StrategyGridConfig::fifty_two_week_high_with_depth(depth),
                StrategyGridConfig::darvas_box_with_depth(depth),
                StrategyGridConfig::larry_williams_with_depth(depth),
                StrategyGridConfig::heikin_ashi_with_depth(depth),
                // Phase 4: Complex Stateful
                StrategyGridConfig::parabolic_sar_with_depth(depth),
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
            // Phase 3: Price Structure
            StrategyTypeId::FiftyTwoWeekHigh => {
                StrategyGridConfig::fifty_two_week_high_with_depth(depth)
            }
            StrategyTypeId::DarvasBox => StrategyGridConfig::darvas_box_with_depth(depth),
            StrategyTypeId::LarryWilliams => StrategyGridConfig::larry_williams_with_depth(depth),
            StrategyTypeId::HeikinAshi => StrategyGridConfig::heikin_ashi_with_depth(depth),
            // Phase 4: Complex Stateful
            StrategyTypeId::ParabolicSar => StrategyGridConfig::parabolic_sar_with_depth(depth),
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
