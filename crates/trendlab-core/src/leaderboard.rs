//! YOLO Mode leaderboard for tracking top-performing strategies.
//!
//! This module provides:
//! - LeaderboardEntry: A single discovered winning strategy
//! - Leaderboard: Maintains top N strategies by Sharpe ratio
//! - CrossSymbolLeaderboard: Aggregated performance across symbols
//! - Session vs All-Time tracking for persistent discovery
//! - Persistence to/from JSON

use crate::metrics::Metrics;
use crate::statistics::{benjamini_hochberg, ConfidenceGrade};
use crate::sweep::{StrategyConfigId, StrategyTypeId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::Path;

// =============================================================================
// Leaderboard Scope (Session vs All-Time)
// =============================================================================

/// Scope for leaderboard viewing - session (current app launch) vs all-time (persistent).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LeaderboardScope {
    /// Current session only (reset each app launch)
    #[default]
    Session,
    /// All-time persistent leaderboard
    AllTime,
}

impl LeaderboardScope {
    /// Toggle between Session and AllTime.
    pub fn toggle(&self) -> Self {
        match self {
            Self::Session => Self::AllTime,
            Self::AllTime => Self::Session,
        }
    }

    /// Display name for UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::AllTime => "All-Time",
        }
    }
}

/// Generate a session ID from the current timestamp.
/// Format: YYYYMMDDTHHMMSS (ISO 8601 basic format)
pub fn generate_session_id() -> String {
    Utc::now().format("%Y%m%dT%H%M%S").to_string()
}

// =============================================================================
// Leaderboard Entry
// =============================================================================

/// A single leaderboard entry representing a discovered winning strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    /// Current rank (1-based, updated on insert/sort)
    pub rank: usize,

    /// Strategy type (Donchian, MACrossover, etc.)
    pub strategy_type: StrategyTypeId,

    /// Full configuration (entry/exit lookbacks, etc.)
    pub config: StrategyConfigId,

    /// Symbol this was tested on (None for aggregate/portfolio)
    pub symbol: Option<String>,

    /// Sector this symbol belongs to (e.g., "technology", "healthcare")
    /// Derived from universe.toml sector mappings
    #[serde(default)]
    pub sector: Option<String>,

    /// Performance metrics
    pub metrics: Metrics,

    /// Equity curve for charting
    pub equity_curve: Vec<f64>,

    /// Timestamps corresponding to each equity curve point (for x-axis labels)
    #[serde(default)]
    pub dates: Vec<DateTime<Utc>>,

    /// When this entry was discovered
    pub discovered_at: DateTime<Utc>,

    /// Which YOLO iteration found this config
    pub iteration: u32,

    /// Session ID that discovered this entry (for tracking session vs all-time)
    #[serde(default)]
    pub session_id: Option<String>,

    /// Statistical confidence grade (computed from bootstrap analysis of returns)
    /// None if not computed yet or insufficient data
    #[serde(default)]
    pub confidence_grade: Option<ConfidenceGrade>,

    // =========================================================================
    // Walk-Forward Validation Fields (Phase 1)
    // =========================================================================
    /// Walk-forward validation result for this config/symbol pair.
    /// Contains fold-by-fold IS/OOS performance for robustness assessment.
    #[serde(default)]
    pub walk_forward_grade: Option<char>,

    /// Mean out-of-sample Sharpe ratio across walk-forward folds.
    /// This is the primary anti-overfit metric - should be positive and stable.
    #[serde(default)]
    pub mean_oos_sharpe: Option<f64>,

    /// Sharpe degradation ratio: mean_oos_sharpe / mean_is_sharpe.
    /// Values close to 1.0 indicate good generalization; < 0.5 suggests overfit.
    #[serde(default)]
    pub sharpe_degradation: Option<f64>,

    /// Percentage of walk-forward folds with positive OOS Sharpe.
    /// High values (>70%) indicate consistent performance across time periods.
    #[serde(default)]
    pub pct_profitable_folds: Option<f64>,

    /// P-value from one-sided test of OOS Sharpe > 0.
    /// Used as input for FDR correction across all tested configs.
    #[serde(default)]
    pub oos_p_value: Option<f64>,

    /// FDR-adjusted p-value after Benjamini-Hochberg correction.
    /// Accounts for multiple comparisons in YOLO mode.
    #[serde(default)]
    pub fdr_adjusted_p_value: Option<f64>,
}

impl LeaderboardEntry {
    /// Create a unique hash for deduplication (strategy_type + config + symbol).
    pub fn config_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        // Hash strategy type name
        format!("{:?}", self.strategy_type).hash(&mut hasher);
        // Hash config
        format!("{:?}", self.config).hash(&mut hasher);
        // Hash symbol
        self.symbol.hash(&mut hasher);
        hasher.finish()
    }

    /// Compute confidence grade from equity curve using bootstrap analysis.
    ///
    /// Returns None if there's insufficient data for reliable analysis.
    pub fn compute_confidence_grade(&self) -> Option<ConfidenceGrade> {
        compute_confidence_from_equity(&self.equity_curve)
    }
}

/// Compute confidence grade from an equity curve.
///
/// Uses bootstrap resampling on daily returns to assess:
/// - Whether the Sharpe ratio is significantly positive
/// - The width of the confidence interval (narrower = more confident)
///
/// Returns None if there's insufficient data (< 30 days).
pub fn compute_confidence_from_equity(equity_curve: &[f64]) -> Option<ConfidenceGrade> {
    use crate::statistics::{bootstrap_sharpe, BootstrapConfig};

    // Need at least 30 data points for meaningful bootstrap
    if equity_curve.len() < 30 {
        return Some(ConfidenceGrade::Insufficient);
    }

    // Compute daily returns
    let returns: Vec<f64> = equity_curve
        .windows(2)
        .map(|w| (w[1] - w[0]) / w[0].max(1e-10))
        .collect();

    if returns.len() < 30 {
        return Some(ConfidenceGrade::Insufficient);
    }

    // Use quick bootstrap (fewer iterations for YOLO mode responsiveness)
    let config = BootstrapConfig::quick();

    // Use 252 trading days per year for annualization
    match bootstrap_sharpe(&returns, 252.0, &config) {
        Ok(result) => {
            // Grade based on:
            // 1. Is Sharpe significantly positive? (ci_lower > 0)
            // 2. Is CI narrow? (ci_width < 1.0)

            let ci_width = result.ci_width();
            let is_positive = result.ci_lower > 0.0;
            let is_strongly_positive = result.ci_lower > 0.5;

            if is_strongly_positive && ci_width < 1.0 {
                Some(ConfidenceGrade::High)
            } else if is_positive && ci_width < 2.0 {
                Some(ConfidenceGrade::Medium)
            } else {
                Some(ConfidenceGrade::Low)
            }
        }
        Err(_) => Some(ConfidenceGrade::Insufficient),
    }
}

/// Compute a confidence grade for *cross-symbol* results using cross-sectional evidence.
///
/// Rationale:
/// - `compute_confidence_from_equity` measures time-series significance of returns.
/// - For YOLO cross-symbol aggregation we care about robustness *across symbols*.
///
/// This grades confidence by bootstrapping the mean per-symbol Sharpe ratio and
/// checking whether the lower bound is meaningfully positive.
///
/// Returns `Insufficient` if there are too few symbols to be meaningful.
pub fn compute_cross_symbol_confidence_from_metrics(
    per_symbol: &HashMap<String, Metrics>,
) -> ConfidenceGrade {
    use crate::statistics::{bootstrap_ci, BootstrapConfig};

    // Need enough symbols to make a cross-sectional statement.
    // (10 is a pragmatic minimum; typical YOLO runs use 30-100+)
    if per_symbol.len() < 10 {
        return ConfidenceGrade::Insufficient;
    }

    let sharpes: Vec<f64> = per_symbol.values().map(|m| m.sharpe).collect();
    if sharpes.len() < 10 {
        return ConfidenceGrade::Insufficient;
    }

    // Tail/robustness guardrails.
    // These thresholds are intentionally conservative: YOLO should avoid strategies
    // that have strong averages but catastrophic tails.
    let min_symbol_sharpe = sharpes.iter().copied().fold(f64::INFINITY, f64::min);
    let positive_frac = sharpes.iter().filter(|&&x| x > 0.0).count() as f64 / sharpes.len() as f64;

    // Bootstrap the mean Sharpe across symbols.
    let cfg = BootstrapConfig::quick();
    let result = match bootstrap_ci(
        &sharpes,
        |xs| xs.iter().sum::<f64>() / xs.len() as f64,
        &cfg,
    ) {
        Ok(r) => r,
        Err(_) => return ConfidenceGrade::Insufficient,
    };

    // Grade thresholds:
    // - High: mean Sharpe is strongly positive with a reasonably tight CI
    // - Medium: mean Sharpe is significantly positive (ci_lower > 0)
    // - Low: not significantly positive
    //
    // NOTE: These thresholds are intentionally less strict than the time-series version
    // because cross-sectional uncertainty behaves differently.
    if result.ci_lower > 0.50
        && result.ci_width() < 0.75
        && positive_frac >= 0.80
        && min_symbol_sharpe > -0.25
    {
        ConfidenceGrade::High
    } else if result.ci_lower > 0.0 && positive_frac >= 0.70 && min_symbol_sharpe > -0.75 {
        ConfidenceGrade::Medium
    } else {
        ConfidenceGrade::Low
    }
}

/// Compute a confidence grade for *cross-sector* robustness.
///
/// This is a stricter, intent-aligned validation for YOLO mode:
/// - Group per-symbol metrics into sectors
/// - Compute mean Sharpe per sector
/// - Bootstrap the mean across sectors
/// - Add guardrails so a single "bad sector" can prevent a High grade
///
/// Returns `None` if there isn't enough sector coverage to make a meaningful statement.
pub fn compute_cross_sector_confidence_from_metrics(
    per_symbol: &HashMap<String, Metrics>,
    per_symbol_sectors: &HashMap<String, String>,
) -> Option<ConfidenceGrade> {
    use crate::statistics::{bootstrap_ci, BootstrapConfig};

    // Need a reasonable number of symbols total (prevents tiny-sample weirdness).
    if per_symbol.len() < 10 {
        return None;
    }

    // Group Sharpe values by sector id.
    let mut by_sector: HashMap<String, Vec<f64>> = HashMap::new();
    for (sym, metrics) in per_symbol {
        let Some(sector_id) = per_symbol_sectors.get(sym) else {
            continue;
        };
        by_sector
            .entry(sector_id.clone())
            .or_default()
            .push(metrics.sharpe);
    }

    // Need enough distinct sectors to talk about "cross-sector" robustness.
    if by_sector.len() < 5 {
        return None;
    }

    // Compute sector mean Sharpe.
    let mut sector_means: Vec<f64> = Vec::with_capacity(by_sector.len());
    for sharpes in by_sector.values() {
        if sharpes.is_empty() {
            continue;
        }
        let mean = sharpes.iter().sum::<f64>() / sharpes.len() as f64;
        sector_means.push(mean);
    }

    if sector_means.len() < 5 {
        return None;
    }

    let worst_sector_mean = sector_means.iter().copied().fold(f64::INFINITY, f64::min);
    let positive_sectors = sector_means.iter().filter(|&&x| x > 0.0).count();
    let positive_frac = positive_sectors as f64 / sector_means.len() as f64;

    // Bootstrap the mean across sectors (equal-weighted by sector).
    let cfg = BootstrapConfig::quick();
    let result = bootstrap_ci(
        &sector_means,
        |xs| xs.iter().sum::<f64>() / xs.len() as f64,
        &cfg,
    )
    .ok()?;

    // Sector-aware grading:
    // - High: significantly strong, tight, and no sector is outright bad
    // - Medium: significantly positive and most sectors are positive
    // - Low: otherwise
    if result.ci_lower > 0.50
        && result.ci_width() < 0.75
        && positive_frac >= 0.80
        && worst_sector_mean > 0.0
    {
        Some(ConfidenceGrade::High)
    } else if result.ci_lower > 0.0 && positive_frac >= 0.70 && worst_sector_mean > -0.25 {
        Some(ConfidenceGrade::Medium)
    } else {
        Some(ConfidenceGrade::Low)
    }
}

// =============================================================================
// Leaderboard
// =============================================================================

/// YOLO Mode leaderboard - maintains top N strategies by Sharpe ratio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leaderboard {
    /// Sorted entries (best Sharpe first)
    pub entries: Vec<LeaderboardEntry>,

    /// Maximum number of entries to keep
    pub max_entries: usize,

    /// Total iterations run so far
    pub total_iterations: u32,

    /// When the leaderboard was created
    pub started_at: DateTime<Utc>,

    /// When the leaderboard was last updated
    pub last_updated: DateTime<Utc>,

    /// Total configs tested across all iterations
    pub total_configs_tested: u64,
}

impl Default for Leaderboard {
    fn default() -> Self {
        Self::new(4) // Default top 4
    }
}

impl Leaderboard {
    /// Create a new empty leaderboard with the given capacity.
    pub fn new(max_entries: usize) -> Self {
        let now = Utc::now();
        Self {
            entries: Vec::with_capacity(max_entries),
            max_entries,
            total_iterations: 0,
            started_at: now,
            last_updated: now,
            total_configs_tested: 0,
        }
    }

    /// Try to insert an entry. Returns true if the entry was added (either new or replaced worse).
    ///
    /// Deduplication: If an entry with the same config_hash exists:
    /// - If new Sharpe > existing Sharpe: replace it
    /// - Otherwise: skip
    ///
    /// Otherwise:
    /// - If not full: add it
    /// - If full and new Sharpe > worst: replace worst
    pub fn try_insert(&mut self, entry: LeaderboardEntry) -> bool {
        let hash = entry.config_hash();
        let new_sharpe = entry.metrics.sharpe;
        let symbol = entry.symbol.as_deref().unwrap_or("N/A");

        // Check for existing entry with same config
        if let Some(pos) = self.entries.iter().position(|e| e.config_hash() == hash) {
            // Same config exists - only replace if better Sharpe
            if new_sharpe > self.entries[pos].metrics.sharpe {
                tracing::debug!(
                    symbol = %symbol,
                    old_sharpe = %self.entries[pos].metrics.sharpe,
                    new_sharpe = %new_sharpe,
                    "Leaderboard: replaced existing entry with better Sharpe"
                );
                self.entries[pos] = entry;
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
            tracing::trace!(
                symbol = %symbol,
                existing_sharpe = %self.entries[pos].metrics.sharpe,
                new_sharpe = %new_sharpe,
                "Leaderboard: rejected duplicate (not better)"
            );
            return false;
        }

        // New config - check if we should add it
        if self.entries.len() < self.max_entries {
            // Not full, just add
            tracing::debug!(
                symbol = %symbol,
                sharpe = %new_sharpe,
                rank = self.entries.len() + 1,
                "Leaderboard: added new entry (not full)"
            );
            self.entries.push(entry);
            self.sort_and_rerank();
            self.last_updated = Utc::now();
            return true;
        }

        // Full - check if better than worst
        if let Some(worst) = self.entries.last() {
            if new_sharpe > worst.metrics.sharpe {
                // Replace worst
                tracing::debug!(
                    symbol = %symbol,
                    sharpe = %new_sharpe,
                    replaced_sharpe = %worst.metrics.sharpe,
                    "Leaderboard: replaced worst entry"
                );
                self.entries.pop();
                self.entries.push(entry);
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
        }

        tracing::trace!(
            symbol = %symbol,
            sharpe = %new_sharpe,
            "Leaderboard: rejected (worse than worst)"
        );
        false
    }

    /// Sort entries by Sharpe (descending) and update ranks.
    fn sort_and_rerank(&mut self) {
        // Sort by Sharpe descending
        self.entries.sort_by(|a, b| {
            b.metrics
                .sharpe
                .partial_cmp(&a.metrics.sharpe)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Update ranks (1-based)
        for (i, entry) in self.entries.iter_mut().enumerate() {
            entry.rank = i + 1;
        }
    }

    /// Get the minimum Sharpe in the leaderboard (for quick filtering).
    pub fn min_sharpe(&self) -> Option<f64> {
        self.entries.last().map(|e| e.metrics.sharpe)
    }

    /// Check if the leaderboard is full.
    pub fn is_full(&self) -> bool {
        self.entries.len() >= self.max_entries
    }

    /// Get the best Sharpe ratio.
    pub fn best_sharpe(&self) -> Option<f64> {
        self.entries.first().map(|e| e.metrics.sharpe)
    }

    /// Increment iteration counter.
    pub fn increment_iteration(&mut self) {
        self.total_iterations += 1;
    }

    /// Add to total configs tested.
    pub fn add_configs_tested(&mut self, count: usize) {
        self.total_configs_tested += count as u64;
    }

    // =========================================================================
    // Persistence
    // =========================================================================

    /// Save leaderboard to a JSON file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        std::fs::write(path, json)
    }

    /// Load leaderboard from a JSON file.
    pub fn load(path: &Path) -> io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Load leaderboard from file, or create new if file doesn't exist.
    pub fn load_or_new(path: &Path, max_entries: usize) -> Self {
        Self::load(path).unwrap_or_else(|_| Self::new(max_entries))
    }
}

// =============================================================================
// Cross-Symbol Aggregation (YOLO mode)
// =============================================================================

/// Ranking metric for cross-symbol results.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CrossSymbolRankMetric {
    /// Average Sharpe across all symbols (default, balanced)
    #[default]
    AvgSharpe,
    /// Minimum Sharpe (conservative - worst-case performance)
    MinSharpe,
    /// Geometric mean of (1 + CAGR) - 1 (rewards consistency)
    GeoMeanCagr,
    /// Hit rate: fraction of symbols where strategy was profitable
    HitRate,
    /// Mean out-of-sample Sharpe from walk-forward validation (anti-overfit ranking)
    MeanOosSharpe,
}

/// Aggregated metrics computed across multiple symbols.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregatedMetrics {
    /// Average Sharpe ratio across all symbols
    pub avg_sharpe: f64,
    /// Minimum Sharpe (worst-performing symbol)
    pub min_sharpe: f64,
    /// Maximum Sharpe (best-performing symbol)
    pub max_sharpe: f64,
    /// Geometric mean of (1 + CAGR) - 1
    pub geo_mean_cagr: f64,
    /// Arithmetic mean CAGR
    pub avg_cagr: f64,
    /// Worst max drawdown across all symbols
    pub worst_max_drawdown: f64,
    /// Average max drawdown
    pub avg_max_drawdown: f64,
    /// Number of symbols where CAGR > 0
    pub profitable_count: usize,
    /// Total number of symbols tested
    pub total_symbols: usize,
    /// Hit rate (profitable_count / total_symbols)
    pub hit_rate: f64,
    /// Average number of trades
    pub avg_trades: f64,
}

impl AggregatedMetrics {
    /// Compute aggregated metrics from per-symbol metrics.
    pub fn from_per_symbol(per_symbol: &HashMap<String, Metrics>) -> Self {
        if per_symbol.is_empty() {
            return Self::default();
        }

        let n = per_symbol.len();
        let mut sharpes: Vec<f64> = Vec::with_capacity(n);
        let mut cagrs: Vec<f64> = Vec::with_capacity(n);
        let mut drawdowns: Vec<f64> = Vec::with_capacity(n);
        let mut trades: Vec<u32> = Vec::with_capacity(n);
        let mut profitable = 0usize;

        for metrics in per_symbol.values() {
            sharpes.push(metrics.sharpe);
            cagrs.push(metrics.cagr);
            drawdowns.push(metrics.max_drawdown);
            trades.push(metrics.num_trades);
            if metrics.cagr > 0.0 {
                profitable += 1;
            }
        }

        // Compute aggregates
        let avg_sharpe = sharpes.iter().sum::<f64>() / n as f64;
        let min_sharpe = sharpes.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_sharpe = sharpes.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        let avg_cagr = cagrs.iter().sum::<f64>() / n as f64;

        // Geometric mean of (1 + CAGR): product^(1/n) - 1
        // Use log-sum to avoid overflow
        let log_sum: f64 = cagrs.iter().map(|&c| (1.0 + c).max(1e-10).ln()).sum();
        let geo_mean_cagr = (log_sum / n as f64).exp() - 1.0;

        let worst_max_drawdown = drawdowns.iter().cloned().fold(0.0, f64::max);
        let avg_max_drawdown = drawdowns.iter().sum::<f64>() / n as f64;

        let avg_trades = trades.iter().map(|&t| t as f64).sum::<f64>() / n as f64;

        Self {
            avg_sharpe,
            min_sharpe,
            max_sharpe,
            geo_mean_cagr,
            avg_cagr,
            worst_max_drawdown,
            avg_max_drawdown,
            profitable_count: profitable,
            total_symbols: n,
            hit_rate: profitable as f64 / n as f64,
            avg_trades,
        }
    }

    /// Get the ranking value for a given metric.
    pub fn rank_value(&self, metric: CrossSymbolRankMetric) -> f64 {
        match metric {
            CrossSymbolRankMetric::AvgSharpe => self.avg_sharpe,
            CrossSymbolRankMetric::MinSharpe => self.min_sharpe,
            CrossSymbolRankMetric::GeoMeanCagr => self.geo_mean_cagr,
            CrossSymbolRankMetric::HitRate => self.hit_rate,
            // MeanOosSharpe falls back to avg_sharpe for AggregatedMetrics
            // (actual OOS data is in AggregatedConfigResult)
            CrossSymbolRankMetric::MeanOosSharpe => self.avg_sharpe,
        }
    }
}

/// Cross-symbol aggregated result for a single strategy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedConfigResult {
    /// Rank in the cross-symbol leaderboard (1-based)
    pub rank: usize,

    /// Strategy type (Donchian, MACrossover, etc.)
    pub strategy_type: StrategyTypeId,

    /// Full configuration
    pub config_id: StrategyConfigId,

    /// Symbols included in this aggregation
    pub symbols: Vec<String>,

    /// Per-symbol sector mapping (symbol -> sector_id)
    /// Enables sector-level analysis of cross-symbol performance
    #[serde(default)]
    pub per_symbol_sectors: HashMap<String, String>,

    /// Per-symbol metrics for drill-down
    pub per_symbol_metrics: HashMap<String, Metrics>,

    /// Aggregated metrics
    pub aggregate_metrics: AggregatedMetrics,

    /// Combined equity curve (equal-weighted average across symbols, normalized to $100k start)
    pub combined_equity_curve: Vec<f64>,

    /// Timestamps for the equity curve
    pub dates: Vec<DateTime<Utc>>,

    /// When this entry was discovered
    pub discovered_at: DateTime<Utc>,

    /// Which YOLO iteration found this config
    pub iteration: u32,

    /// Session ID that discovered this entry (for tracking session vs all-time)
    #[serde(default)]
    pub session_id: Option<String>,

    /// Statistical confidence grade (computed from bootstrap analysis of combined equity returns)
    /// None if not computed yet or insufficient data
    #[serde(default)]
    pub confidence_grade: Option<ConfidenceGrade>,

    // =========================================================================
    // Walk-Forward Validation Fields (Phase 1)
    // =========================================================================
    /// Walk-forward grade (A-F) aggregated across symbols.
    /// A/B = robust, C = marginal, D/F = likely overfit.
    #[serde(default)]
    pub walk_forward_grade: Option<char>,

    /// Mean out-of-sample Sharpe ratio across all symbols and folds.
    /// This is the primary anti-overfit ranking metric.
    #[serde(default)]
    pub mean_oos_sharpe: Option<f64>,

    /// Standard deviation of OOS Sharpe across folds.
    /// Lower values indicate more consistent performance.
    #[serde(default)]
    pub std_oos_sharpe: Option<f64>,

    /// Sharpe degradation ratio: mean_oos / mean_is.
    /// Values close to 1.0 indicate good generalization.
    #[serde(default)]
    pub sharpe_degradation: Option<f64>,

    /// Percentage of folds with positive OOS Sharpe (aggregated across symbols).
    /// High values (>70%) indicate robustness across time.
    #[serde(default)]
    pub pct_profitable_folds: Option<f64>,

    /// P-value from one-sided test of mean OOS Sharpe > 0.
    #[serde(default)]
    pub oos_p_value: Option<f64>,

    /// FDR-adjusted p-value after Benjamini-Hochberg correction.
    #[serde(default)]
    pub fdr_adjusted_p_value: Option<f64>,
}

impl AggregatedConfigResult {
    /// Create a unique hash for deduplication (strategy_type + config).
    /// Unlike LeaderboardEntry, this is NOT per-symbol.
    pub fn config_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        format!("{:?}", self.strategy_type).hash(&mut hasher);
        format!("{:?}", self.config_id).hash(&mut hasher);
        hasher.finish()
    }

    /// Compute confidence grade from combined equity curve using bootstrap analysis.
    ///
    /// Returns None if there's insufficient data for reliable analysis.
    pub fn compute_confidence_grade(&self) -> Option<ConfidenceGrade> {
        compute_confidence_from_equity(&self.combined_equity_curve)
    }

    /// Get the ranking value for a given metric.
    ///
    /// This handles both traditional metrics (from AggregatedMetrics) and
    /// walk-forward metrics (from the entry itself).
    pub fn rank_value(&self, metric: CrossSymbolRankMetric) -> f64 {
        match metric {
            CrossSymbolRankMetric::AvgSharpe => self.aggregate_metrics.avg_sharpe,
            CrossSymbolRankMetric::MinSharpe => self.aggregate_metrics.min_sharpe,
            CrossSymbolRankMetric::GeoMeanCagr => self.aggregate_metrics.geo_mean_cagr,
            CrossSymbolRankMetric::HitRate => self.aggregate_metrics.hit_rate,
            CrossSymbolRankMetric::MeanOosSharpe => {
                // Use OOS Sharpe if available, otherwise fall back to avg_sharpe
                self.mean_oos_sharpe.unwrap_or(f64::NEG_INFINITY)
            }
        }
    }
}

/// Cross-symbol leaderboard - maintains top N configs ranked by aggregate performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSymbolLeaderboard {
    /// Sorted entries (best aggregate metric first)
    pub entries: Vec<AggregatedConfigResult>,

    /// Maximum number of entries to keep
    pub max_entries: usize,

    /// Ranking metric being used
    pub rank_by: CrossSymbolRankMetric,

    /// Total iterations run
    pub total_iterations: u32,

    /// When the leaderboard was created
    pub started_at: DateTime<Utc>,

    /// When the leaderboard was last updated
    pub last_updated: DateTime<Utc>,

    /// Total configs tested across all iterations
    pub total_configs_tested: u64,
}

impl Default for CrossSymbolLeaderboard {
    fn default() -> Self {
        Self::new(4, CrossSymbolRankMetric::AvgSharpe)
    }
}

impl CrossSymbolLeaderboard {
    /// Create a new empty cross-symbol leaderboard.
    pub fn new(max_entries: usize, rank_by: CrossSymbolRankMetric) -> Self {
        let now = Utc::now();
        Self {
            entries: Vec::with_capacity(max_entries),
            max_entries,
            rank_by,
            total_iterations: 0,
            started_at: now,
            last_updated: now,
            total_configs_tested: 0,
        }
    }

    /// Try to insert an entry. Returns true if the entry was added.
    ///
    /// Deduplication: If an entry with the same config_hash exists:
    /// - If new rank_value > existing: replace it
    /// - Otherwise: skip
    pub fn try_insert(&mut self, entry: AggregatedConfigResult) -> bool {
        let hash = entry.config_hash();
        let new_value = entry.aggregate_metrics.rank_value(self.rank_by);
        let config_desc = format!("{:?}", entry.config_id);
        let num_symbols = entry.symbols.len();

        // Check for existing entry with same config
        if let Some(pos) = self.entries.iter().position(|e| e.config_hash() == hash) {
            let existing_value = self.entries[pos].aggregate_metrics.rank_value(self.rank_by);
            if new_value > existing_value {
                tracing::debug!(
                    config = %config_desc,
                    symbols = num_symbols,
                    old_value = %existing_value,
                    new_value = %new_value,
                    "CrossSymbolLeaderboard: replaced existing entry"
                );
                self.entries[pos] = entry;
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
            tracing::trace!(
                config = %config_desc,
                existing_value = %existing_value,
                new_value = %new_value,
                "CrossSymbolLeaderboard: rejected duplicate (not better)"
            );
            return false;
        }

        // New config
        if self.entries.len() < self.max_entries {
            tracing::debug!(
                config = %config_desc,
                symbols = num_symbols,
                avg_sharpe = %entry.aggregate_metrics.avg_sharpe,
                hit_rate = %entry.aggregate_metrics.hit_rate,
                rank = self.entries.len() + 1,
                "CrossSymbolLeaderboard: added new entry"
            );
            self.entries.push(entry);
            self.sort_and_rerank();
            self.last_updated = Utc::now();
            return true;
        }

        // Full - check if better than worst
        if let Some(worst) = self.entries.last() {
            let worst_value = worst.aggregate_metrics.rank_value(self.rank_by);
            if new_value > worst_value {
                tracing::debug!(
                    config = %config_desc,
                    symbols = num_symbols,
                    new_value = %new_value,
                    replaced_value = %worst_value,
                    "CrossSymbolLeaderboard: replaced worst entry"
                );
                self.entries.pop();
                self.entries.push(entry);
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
        }

        tracing::trace!(
            config = %config_desc,
            new_value = %new_value,
            "CrossSymbolLeaderboard: rejected (worse than worst)"
        );
        false
    }

    /// Sort entries by rank metric (descending) and update ranks.
    fn sort_and_rerank(&mut self) {
        let rank_by = self.rank_by;
        self.entries.sort_by(|a, b| {
            let va = a.aggregate_metrics.rank_value(rank_by);
            let vb = b.aggregate_metrics.rank_value(rank_by);
            vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
        });

        for (i, entry) in self.entries.iter_mut().enumerate() {
            entry.rank = i + 1;
        }
    }

    /// Get the best (highest) rank value in the leaderboard.
    pub fn best_value(&self) -> Option<f64> {
        self.entries
            .first()
            .map(|e| e.aggregate_metrics.rank_value(self.rank_by))
    }

    /// Get the best (highest) average Sharpe ratio in the leaderboard.
    pub fn best_avg_sharpe(&self) -> Option<f64> {
        self.entries.first().map(|e| e.aggregate_metrics.avg_sharpe)
    }

    /// Get the minimum (worst) rank value in the leaderboard.
    pub fn min_value(&self) -> Option<f64> {
        self.entries
            .last()
            .map(|e| e.aggregate_metrics.rank_value(self.rank_by))
    }

    /// Check if the leaderboard is full.
    pub fn is_full(&self) -> bool {
        self.entries.len() >= self.max_entries
    }

    /// Increment iteration counter.
    pub fn increment_iteration(&mut self) {
        self.total_iterations += 1;
    }

    /// Add to total configs tested.
    pub fn add_configs_tested(&mut self, count: usize) {
        self.total_configs_tested += count as u64;
    }

    // =========================================================================
    // Persistence
    // =========================================================================

    /// Save cross-symbol leaderboard to a JSON file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        std::fs::write(path, json)
    }

    /// Load cross-symbol leaderboard from a JSON file.
    pub fn load(path: &Path) -> io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Load from file or create new if file doesn't exist.
    pub fn load_or_new(path: &Path, max_entries: usize, rank_by: CrossSymbolRankMetric) -> Self {
        Self::load(path).unwrap_or_else(|_| Self::new(max_entries, rank_by))
    }

    // =========================================================================
    // FDR Correction
    // =========================================================================

    /// Apply Benjamini-Hochberg FDR correction to all entries with p-values.
    ///
    /// This method:
    /// 1. Collects OOS p-values from entries that have them
    /// 2. Applies Benjamini-Hochberg FDR correction
    /// 3. Updates `fdr_adjusted_p_value` on each entry
    /// 4. Optionally downgrades confidence grades for non-significant results
    ///
    /// # Arguments
    /// * `alpha` - Significance level (typically 0.05)
    /// * `downgrade_confidence` - If true, downgrade confidence grade for entries
    ///   where adjusted p-value >= alpha
    ///
    /// # Returns
    /// Number of entries that remain significant after FDR correction
    pub fn apply_fdr_correction(&mut self, alpha: f64, downgrade_confidence: bool) -> usize {
        // Collect indices and p-values for entries that have OOS p-values
        let entries_with_pvals: Vec<(usize, f64)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| entry.oos_p_value.map(|p| (i, p)))
            .collect();

        if entries_with_pvals.is_empty() {
            return 0;
        }

        let p_values: Vec<f64> = entries_with_pvals.iter().map(|(_, p)| *p).collect();

        // Apply Benjamini-Hochberg correction
        let fdr_result = match benjamini_hochberg(&p_values, alpha) {
            Ok(result) => result,
            Err(_) => return 0, // Should not happen if p_values is non-empty
        };

        let mut n_significant = 0;

        // Update entries with adjusted p-values
        for (local_idx, (entry_idx, _)) in entries_with_pvals.iter().enumerate() {
            let entry = &mut self.entries[*entry_idx];
            let adjusted_p = fdr_result.adjusted_p_values[local_idx];
            let is_significant = fdr_result.rejections[local_idx];

            entry.fdr_adjusted_p_value = Some(adjusted_p);

            if is_significant {
                n_significant += 1;
            } else if downgrade_confidence {
                // Downgrade confidence grade for non-significant results
                entry.confidence_grade = match entry.confidence_grade {
                    Some(ConfidenceGrade::High) => Some(ConfidenceGrade::Medium),
                    Some(ConfidenceGrade::Medium) => Some(ConfidenceGrade::Low),
                    other => other, // Low stays Low, None stays None
                };
            }
        }

        n_significant
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Metrics;

    fn make_entry(sharpe: f64, iteration: u32, symbol: Option<&str>) -> LeaderboardEntry {
        LeaderboardEntry {
            rank: 0,
            strategy_type: StrategyTypeId::Donchian,
            config: StrategyConfigId::Donchian {
                entry_lookback: 20,
                exit_lookback: 10,
            },
            symbol: symbol.map(|s| s.to_string()),
            sector: None,
            metrics: Metrics {
                sharpe,
                ..Default::default()
            },
            equity_curve: vec![100.0, 110.0, 120.0],
            dates: vec![],
            discovered_at: Utc::now(),
            iteration,
            session_id: None,
            confidence_grade: None,
            // Walk-forward / FDR fields (None for basic tests)
            walk_forward_grade: None,
            mean_oos_sharpe: None,
            sharpe_degradation: None,
            pct_profitable_folds: None,
            oos_p_value: None,
            fdr_adjusted_p_value: None,
        }
    }

    #[test]
    fn test_insert_not_full() {
        let mut lb = Leaderboard::new(4);
        assert!(lb.try_insert(make_entry(1.5, 1, Some("AAPL"))));
        assert_eq!(lb.entries.len(), 1);
        assert_eq!(lb.entries[0].rank, 1);
    }

    #[test]
    fn test_insert_maintains_order() {
        let mut lb = Leaderboard::new(4);
        lb.try_insert(make_entry(1.0, 1, Some("AAPL")));
        lb.try_insert(make_entry(2.0, 2, Some("GOOG")));
        lb.try_insert(make_entry(1.5, 3, Some("MSFT")));

        assert_eq!(lb.entries[0].metrics.sharpe, 2.0);
        assert_eq!(lb.entries[0].rank, 1);
        assert_eq!(lb.entries[1].metrics.sharpe, 1.5);
        assert_eq!(lb.entries[1].rank, 2);
        assert_eq!(lb.entries[2].metrics.sharpe, 1.0);
        assert_eq!(lb.entries[2].rank, 3);
    }

    #[test]
    fn test_insert_replaces_worst_when_full() {
        let mut lb = Leaderboard::new(2);
        lb.try_insert(make_entry(1.0, 1, Some("A")));
        lb.try_insert(make_entry(1.5, 2, Some("B")));

        // This should replace the 1.0 entry
        assert!(lb.try_insert(make_entry(2.0, 3, Some("C"))));
        assert_eq!(lb.entries.len(), 2);
        assert_eq!(lb.entries[0].metrics.sharpe, 2.0);
        assert_eq!(lb.entries[1].metrics.sharpe, 1.5);
    }

    #[test]
    fn test_insert_rejects_worse_when_full() {
        let mut lb = Leaderboard::new(2);
        lb.try_insert(make_entry(2.0, 1, Some("A")));
        lb.try_insert(make_entry(1.5, 2, Some("B")));

        // This should be rejected (worse than both)
        assert!(!lb.try_insert(make_entry(1.0, 3, Some("C"))));
        assert_eq!(lb.entries.len(), 2);
    }

    #[test]
    fn test_deduplication_replaces_if_better() {
        let mut lb = Leaderboard::new(4);

        // Same config (Donchian 20/10 on AAPL)
        lb.try_insert(make_entry(1.0, 1, Some("AAPL")));
        assert!(lb.try_insert(make_entry(2.0, 2, Some("AAPL"))));

        // Should still be 1 entry, but with better Sharpe
        assert_eq!(lb.entries.len(), 1);
        assert_eq!(lb.entries[0].metrics.sharpe, 2.0);
        assert_eq!(lb.entries[0].iteration, 2);
    }

    #[test]
    fn test_deduplication_rejects_if_worse() {
        let mut lb = Leaderboard::new(4);

        lb.try_insert(make_entry(2.0, 1, Some("AAPL")));
        assert!(!lb.try_insert(make_entry(1.0, 2, Some("AAPL"))));

        // Should keep original
        assert_eq!(lb.entries.len(), 1);
        assert_eq!(lb.entries[0].metrics.sharpe, 2.0);
        assert_eq!(lb.entries[0].iteration, 1);
    }

    #[test]
    fn test_best_and_min_sharpe() {
        let mut lb = Leaderboard::new(4);
        lb.try_insert(make_entry(1.0, 1, Some("A")));
        lb.try_insert(make_entry(2.0, 2, Some("B")));

        assert_eq!(lb.best_sharpe(), Some(2.0));
        assert_eq!(lb.min_sharpe(), Some(1.0));
    }

    #[test]
    fn test_save_and_load() {
        let mut lb = Leaderboard::new(4);
        lb.try_insert(make_entry(1.5, 1, Some("AAPL")));
        lb.total_iterations = 10;
        lb.total_configs_tested = 500;

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_leaderboard.json");

        lb.save(&path).unwrap();
        let loaded = Leaderboard::load(&path).unwrap();

        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].metrics.sharpe, 1.5);
        assert_eq!(loaded.total_iterations, 10);
        assert_eq!(loaded.total_configs_tested, 500);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    fn make_cross_symbol_entry(sharpe: f64, oos_p_value: Option<f64>) -> AggregatedConfigResult {
        AggregatedConfigResult {
            rank: 0,
            strategy_type: StrategyTypeId::Donchian,
            config_id: StrategyConfigId::Donchian {
                entry_lookback: 20,
                exit_lookback: 10,
            },
            symbols: vec!["AAPL".to_string(), "MSFT".to_string()],
            per_symbol_sectors: HashMap::new(),
            per_symbol_metrics: HashMap::new(),
            aggregate_metrics: AggregatedMetrics {
                avg_sharpe: sharpe,
                min_sharpe: sharpe - 0.5,
                max_sharpe: sharpe + 0.5,
                geo_mean_cagr: 0.15,
                avg_cagr: 0.15,
                worst_max_drawdown: 0.15,
                avg_max_drawdown: 0.10,
                profitable_count: 3,
                total_symbols: 5,
                hit_rate: 0.6,
                avg_trades: 50.0,
            },
            combined_equity_curve: vec![100.0, 110.0, 120.0],
            dates: vec![],
            discovered_at: Utc::now(),
            iteration: 1,
            session_id: None,
            confidence_grade: Some(ConfidenceGrade::High),
            // Walk-forward fields
            walk_forward_grade: Some('B'),
            mean_oos_sharpe: Some(sharpe * 0.8),
            std_oos_sharpe: Some(0.3),
            sharpe_degradation: Some(0.2),
            pct_profitable_folds: Some(0.75),
            oos_p_value,
            fdr_adjusted_p_value: None,
        }
    }

    #[test]
    fn test_apply_fdr_correction() {
        let mut lb = CrossSymbolLeaderboard::new(10, CrossSymbolRankMetric::AvgSharpe);

        // Add entries with varying p-values
        // Entry with significant p-value (should remain significant after FDR)
        let mut e1 = make_cross_symbol_entry(2.0, Some(0.001));
        e1.confidence_grade = Some(ConfidenceGrade::High);
        lb.entries.push(e1);

        // Entry with marginal p-value (should remain significant after FDR at 0.05)
        let mut e2 = make_cross_symbol_entry(1.8, Some(0.02));
        e2.confidence_grade = Some(ConfidenceGrade::High);
        lb.entries.push(e2);

        // Entry with non-significant p-value (should be downgraded)
        let mut e3 = make_cross_symbol_entry(1.5, Some(0.3));
        e3.confidence_grade = Some(ConfidenceGrade::High);
        lb.entries.push(e3);

        // Entry without p-value (should be unchanged)
        let mut e4 = make_cross_symbol_entry(1.2, None);
        e4.confidence_grade = Some(ConfidenceGrade::High);
        lb.entries.push(e4);

        // Apply FDR correction with confidence downgrade
        let n_significant = lb.apply_fdr_correction(0.05, true);

        // Should have some significant results
        assert!(n_significant >= 1, "Expected at least 1 significant result");

        // Entry with 0.001 p-value should still be significant
        assert!(lb.entries[0].fdr_adjusted_p_value.is_some());
        assert!(lb.entries[0].fdr_adjusted_p_value.unwrap() < 0.05);
        assert_eq!(lb.entries[0].confidence_grade, Some(ConfidenceGrade::High));

        // Entry with 0.3 p-value should be downgraded (High -> Medium)
        assert!(lb.entries[2].fdr_adjusted_p_value.is_some());
        assert!(lb.entries[2].fdr_adjusted_p_value.unwrap() >= 0.05);
        assert_eq!(
            lb.entries[2].confidence_grade,
            Some(ConfidenceGrade::Medium)
        );

        // Entry without p-value should be unchanged
        assert!(lb.entries[3].fdr_adjusted_p_value.is_none());
        assert_eq!(lb.entries[3].confidence_grade, Some(ConfidenceGrade::High));
    }
}
