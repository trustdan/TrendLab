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
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::Path;

/// Deserialize a field that may be null as the default value.
fn deserialize_null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

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

    /// Which YOLO iteration found this config (all-time counter)
    pub iteration: u32,

    /// Session-relative iteration number (starts at 1 each session)
    #[serde(default)]
    pub session_iteration: Option<u32>,

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
/// Uses block bootstrap resampling on daily returns to assess:
/// - Whether the Sharpe ratio is significantly positive
/// - The width of the confidence interval (narrower = more confident)
///
/// Block bootstrap is used instead of IID bootstrap because financial returns
/// exhibit autocorrelation. This produces more realistic (wider) confidence intervals
/// that account for serial dependence in the time-series.
///
/// Returns None if there's insufficient data (< 30 days).
pub fn compute_confidence_from_equity(equity_curve: &[f64]) -> Option<ConfidenceGrade> {
    use crate::statistics::{block_bootstrap_sharpe, BlockBootstrapConfig};

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

    // Use quick block bootstrap (fewer iterations for YOLO mode responsiveness)
    // Stationary bootstrap with adaptive block length preserves autocorrelation structure
    let config = BlockBootstrapConfig::quick_time_series(returns.len());

    // Use 252 trading days per year for annualization
    match block_bootstrap_sharpe(&returns, 252.0, &config) {
        Ok(result) => {
            // Grade based on:
            // 1. Is Sharpe significantly positive? (ci_lower > 0)
            // 2. Is CI narrow? (ci_width < 1.0)
            // Note: Block bootstrap CIs are typically wider than IID, so thresholds
            // may need adjustment in practice. These are conservative starting points.

            let ci_width = result.ci_width();
            let is_positive = result.ci_lower > 0.0;
            let is_strongly_positive = result.ci_lower > 0.5;

            if is_strongly_positive && ci_width < 1.5 {
                Some(ConfidenceGrade::High)
            } else if is_positive && ci_width < 3.0 {
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

    /// Sort entries by Sharpe (descending), truncate to max_entries, and update ranks.
    pub fn sort_and_rerank(&mut self) {
        // Sort by Sharpe descending
        self.entries.sort_by(|a, b| {
            b.metrics
                .sharpe
                .partial_cmp(&a.metrics.sharpe)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to max_entries to prevent unbounded growth
        if self.entries.len() > self.max_entries {
            self.entries.truncate(self.max_entries);
        }

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
    /// Composite score using weighted percentile ranking with a specific risk profile
    CompositeScore(RiskProfile),
}

// =============================================================================
// Risk Profiles for Weighted Ranking
// =============================================================================

/// Risk profile presets for weighted percentile ranking.
///
/// Each profile emphasizes different metrics based on trading style:
/// - Balanced: Equal consideration of performance, risk, and robustness
/// - Conservative: Emphasizes tail risk, drawdown, and consistency
/// - Aggressive: Prioritizes returns and Sharpe over risk metrics
/// - TrendOptions: For options traders using trend signals - emphasizes hit rate
///   and consecutive loss metrics (for premium budgeting)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RiskProfile {
    /// Balanced - default weights across all metrics
    #[default]
    Balanced,
    /// Conservative - emphasizes tail risk, drawdown, consistency
    Conservative,
    /// Aggressive - emphasizes returns, Sharpe, hit rate
    Aggressive,
    /// TrendOptions - for options traders using trend signals
    /// High weight on hit rate (false signals waste premium),
    /// consecutive losses (premium budgeting), OOS Sharpe (anti-overfit)
    TrendOptions,
}

impl RiskProfile {
    /// Get the ranking weights for this profile.
    pub fn weights(&self) -> RankingWeights {
        match self {
            Self::Balanced => RankingWeights::balanced(),
            Self::Conservative => RankingWeights::conservative(),
            Self::Aggressive => RankingWeights::aggressive(),
            Self::TrendOptions => RankingWeights::trend_options(),
        }
    }

    /// Cycle to the next profile.
    pub fn next(&self) -> Self {
        match self {
            Self::Balanced => Self::Conservative,
            Self::Conservative => Self::Aggressive,
            Self::Aggressive => Self::TrendOptions,
            Self::TrendOptions => Self::Balanced,
        }
    }

    /// Display name for UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Balanced => "Balanced",
            Self::Conservative => "Conservative",
            Self::Aggressive => "Aggressive",
            Self::TrendOptions => "TrendOptions",
        }
    }
}

/// Weights for computing a composite percentile ranking score.
///
/// Each weight represents the relative importance of that metric.
/// Weights should sum to 1.0 for interpretable scores.
/// Negative metrics (where lower is better) are automatically inverted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingWeights {
    // Performance metrics
    /// Weight for average Sharpe ratio
    pub w_avg_sharpe: f64,
    /// Weight for out-of-sample Sharpe (anti-overfit)
    pub w_oos_sharpe: f64,
    /// Weight for hit rate (fraction of profitable symbols)
    pub w_hit_rate: f64,

    // Risk metrics (lower is better - will be inverted in scoring)
    /// Weight for minimum Sharpe (consistency)
    pub w_min_sharpe: f64,
    /// Weight for maximum drawdown penalty
    pub w_max_drawdown: f64,
    /// Weight for CVaR 95% (tail risk)
    pub w_cvar_95: f64,

    // Trade quality metrics
    /// Weight for average holding duration (useful for expiry selection)
    pub w_avg_duration: f64,
    /// Weight for max consecutive losses (premium budgeting)
    pub w_max_consecutive_losses: f64,

    // Robustness metrics
    /// Weight for walk-forward grade
    pub w_wf_grade: f64,
    /// Weight for regime concentration penalty
    pub w_regime_concentration: f64,
}

impl Default for RankingWeights {
    fn default() -> Self {
        Self::balanced()
    }
}

impl RankingWeights {
    /// Balanced weights - equal consideration across metrics.
    pub fn balanced() -> Self {
        Self {
            w_avg_sharpe: 0.15,
            w_oos_sharpe: 0.15,
            w_hit_rate: 0.15,
            w_min_sharpe: 0.10,
            w_max_drawdown: 0.10,
            w_cvar_95: 0.10,
            w_avg_duration: 0.05,
            w_max_consecutive_losses: 0.05,
            w_wf_grade: 0.10,
            w_regime_concentration: 0.05,
        }
    }

    /// Conservative weights - emphasizes tail risk, drawdown, consistency.
    pub fn conservative() -> Self {
        Self {
            w_avg_sharpe: 0.10,
            w_oos_sharpe: 0.15,
            w_hit_rate: 0.10,
            w_min_sharpe: 0.15,
            w_max_drawdown: 0.15,
            w_cvar_95: 0.15,
            w_avg_duration: 0.00,
            w_max_consecutive_losses: 0.05,
            w_wf_grade: 0.10,
            w_regime_concentration: 0.05,
        }
    }

    /// Aggressive weights - prioritizes returns and Sharpe.
    pub fn aggressive() -> Self {
        Self {
            w_avg_sharpe: 0.25,
            w_oos_sharpe: 0.15,
            w_hit_rate: 0.20,
            w_min_sharpe: 0.05,
            w_max_drawdown: 0.10,
            w_cvar_95: 0.05,
            w_avg_duration: 0.00,
            w_max_consecutive_losses: 0.05,
            w_wf_grade: 0.10,
            w_regime_concentration: 0.05,
        }
    }

    /// TrendOptions weights - for options traders using trend signals.
    ///
    /// Emphasizes:
    /// - Hit rate (25%): False signals waste premium
    /// - OOS Sharpe (20%): Anti-overfit is critical
    /// - Max consecutive losses (10%): Premium budgeting
    /// - Avg duration (5%): Useful for expiry selection
    pub fn trend_options() -> Self {
        Self {
            w_avg_sharpe: 0.15,
            w_oos_sharpe: 0.20,
            w_hit_rate: 0.25,
            w_min_sharpe: 0.05,
            w_max_drawdown: 0.10,
            w_cvar_95: 0.05,
            w_avg_duration: 0.05,
            w_max_consecutive_losses: 0.10,
            w_wf_grade: 0.05,
            w_regime_concentration: 0.00,
        }
    }

    /// Validate that weights sum to approximately 1.0.
    pub fn validate(&self) -> Result<(), String> {
        let sum = self.w_avg_sharpe
            + self.w_oos_sharpe
            + self.w_hit_rate
            + self.w_min_sharpe
            + self.w_max_drawdown
            + self.w_cvar_95
            + self.w_avg_duration
            + self.w_max_consecutive_losses
            + self.w_wf_grade
            + self.w_regime_concentration;

        if (sum - 1.0).abs() > 0.01 {
            Err(format!("Weights sum to {:.3}, expected ~1.0", sum))
        } else {
            Ok(())
        }
    }

    /// Convert to RobustScoreConfig for compatibility with existing robust_score().
    ///
    /// Maps RankingWeights to the 7-component RobustScoreConfig:
    /// - sharpe <- avg_sharpe
    /// - min_sharpe <- min_sharpe
    /// - hitrate <- hit_rate
    /// - drawdown <- max_drawdown
    /// - tail_risk <- cvar_95
    /// - kurtosis <- (split between duration and consecutive losses)
    /// - regime_concentration <- regime_concentration
    pub fn to_robust_score_config(&self) -> RobustScoreConfig {
        // Combine the extra weights (oos_sharpe, duration, consecutive_losses, wf_grade)
        // into the 7 components of RobustScoreConfig.
        // This is a lossy conversion - full percentile ranking uses all weights directly.
        RobustScoreConfig {
            w_sharpe: self.w_avg_sharpe + (self.w_oos_sharpe / 2.0),
            w_min_sharpe: self.w_min_sharpe,
            w_hitrate: self.w_hit_rate,
            w_drawdown: self.w_max_drawdown,
            w_tail_risk: self.w_cvar_95,
            // Combine duration + consecutive losses + wf_grade into kurtosis weight
            w_kurtosis: self.w_avg_duration + self.w_max_consecutive_losses + self.w_wf_grade,
            w_regime_concentration: self.w_regime_concentration + (self.w_oos_sharpe / 2.0),
        }
    }
}

/// Aggregated metrics computed across multiple symbols.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AggregatedMetrics {
    /// Average Sharpe ratio across all symbols
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub avg_sharpe: f64,
    /// Minimum Sharpe (worst-performing symbol)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub min_sharpe: f64,
    /// Maximum Sharpe (best-performing symbol)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub max_sharpe: f64,
    /// Geometric mean of (1 + CAGR) - 1
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub geo_mean_cagr: f64,
    /// Arithmetic mean CAGR
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub avg_cagr: f64,
    /// Worst max drawdown across all symbols
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub worst_max_drawdown: f64,
    /// Average max drawdown
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub avg_max_drawdown: f64,
    /// Number of symbols where CAGR > 0
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub profitable_count: usize,
    /// Total number of symbols tested
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub total_symbols: usize,
    /// Hit rate (profitable_count / total_symbols)
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub hit_rate: f64,
    /// Average number of trades
    #[serde(default, deserialize_with = "deserialize_null_as_default")]
    pub avg_trades: f64,

    // =========================================================================
    // Tail Risk Metrics (Phase 2)
    // =========================================================================
    /// Average CVaR (Conditional Value at Risk) at 95% across symbols.
    /// Represents the expected loss in the worst 5% of days.
    #[serde(default)]
    pub avg_cvar_95: Option<f64>,

    /// Worst CVaR across all symbols (most extreme tail risk).
    #[serde(default)]
    pub worst_cvar_95: Option<f64>,

    /// Average skewness of daily returns across symbols.
    /// Negative skewness indicates more extreme left-tail (bad) events.
    #[serde(default)]
    pub avg_skewness: Option<f64>,

    /// Worst (most negative) skewness across symbols.
    #[serde(default)]
    pub worst_skewness: Option<f64>,

    /// Average excess kurtosis of daily returns across symbols.
    /// High kurtosis indicates fat tails (more extreme events than normal).
    #[serde(default)]
    pub avg_kurtosis: Option<f64>,

    /// Maximum (worst) kurtosis across symbols.
    #[serde(default)]
    pub max_kurtosis: Option<f64>,

    /// Downside deviation ratio (downside std / total std).
    /// Lower values indicate less downside volatility relative to total.
    #[serde(default)]
    pub downside_ratio: Option<f64>,

    // =========================================================================
    // Regime Concentration (Phase 3)
    // =========================================================================
    /// Regime concentration penalty factor (0-1).
    /// 0 = no concentration (balanced across regimes), 1 = all returns from one regime.
    /// Computed from `RegimeConcentrationScore::penalty_factor()`.
    #[serde(default)]
    pub regime_concentration_penalty: Option<f64>,
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
            // Tail risk metrics not computed in basic constructor
            avg_cvar_95: None,
            worst_cvar_95: None,
            avg_skewness: None,
            worst_skewness: None,
            avg_kurtosis: None,
            max_kurtosis: None,
            downside_ratio: None,
            regime_concentration_penalty: None,
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
            // For composite score, use robust_score with the profile's weights
            // This is a fallback - full percentile ranking is computed in CrossSymbolLeaderboard
            CrossSymbolRankMetric::CompositeScore(profile) => {
                let robust_config = profile.weights().to_robust_score_config();
                self.robust_score(&robust_config)
            }
        }
    }

    /// Compute aggregated metrics from per-symbol metrics AND equity curves.
    ///
    /// This extends `from_per_symbol` with tail risk metrics computed from
    /// the equity curve returns.
    ///
    /// # Arguments
    /// * `per_symbol` - Metrics per symbol
    /// * `equity_curves` - Per-symbol equity curves (symbol -> Vec<f64>)
    pub fn from_per_symbol_with_tail_risk(
        per_symbol: &HashMap<String, Metrics>,
        equity_curves: &HashMap<String, Vec<f64>>,
    ) -> Self {
        // Start with base metrics
        let mut metrics = Self::from_per_symbol(per_symbol);

        if equity_curves.is_empty() {
            return metrics;
        }

        // Compute tail risk per symbol from equity curve returns
        let mut cvars: Vec<f64> = Vec::new();
        let mut skewnesses: Vec<f64> = Vec::new();
        let mut kurtoses: Vec<f64> = Vec::new();
        let mut all_downside_ratios: Vec<f64> = Vec::new();

        for equity in equity_curves.values() {
            if equity.len() < 30 {
                continue; // Need enough data for meaningful stats
            }

            // Compute daily returns from equity curve
            let returns: Vec<f64> = equity
                .windows(2)
                .map(|w| (w[1] - w[0]) / w[0].max(1e-10))
                .collect();

            if returns.len() < 20 {
                continue;
            }

            // CVaR at 95% (expected loss in worst 5% of days)
            let mut sorted_returns = returns.clone();
            sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let cutoff = (sorted_returns.len() as f64 * 0.05).ceil() as usize;
            if cutoff > 0 {
                let tail: Vec<f64> = sorted_returns[..cutoff].to_vec();
                let cvar = -(tail.iter().sum::<f64>() / tail.len() as f64);
                cvars.push(cvar);
            }

            // Skewness and kurtosis
            let n = returns.len() as f64;
            let mean = returns.iter().sum::<f64>() / n;
            let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
            let std = variance.sqrt();

            if std > 1e-10 {
                // Skewness: E[(X - μ)^3] / σ^3
                let m3 = returns.iter().map(|r| (r - mean).powi(3)).sum::<f64>() / n;
                let skew = m3 / std.powi(3);
                skewnesses.push(skew);

                // Excess kurtosis: E[(X - μ)^4] / σ^4 - 3
                let m4 = returns.iter().map(|r| (r - mean).powi(4)).sum::<f64>() / n;
                let kurt = m4 / std.powi(4) - 3.0;
                kurtoses.push(kurt);

                // Downside deviation ratio
                let downside_variance = returns
                    .iter()
                    .filter(|&&r| r < 0.0)
                    .map(|r| r.powi(2))
                    .sum::<f64>()
                    / n;
                let downside_std = downside_variance.sqrt();
                all_downside_ratios.push(downside_std / std);
            }
        }

        // Aggregate tail risk metrics
        if !cvars.is_empty() {
            let n = cvars.len() as f64;
            metrics.avg_cvar_95 = Some(cvars.iter().sum::<f64>() / n);
            metrics.worst_cvar_95 = cvars
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max)
                .into();
        }

        if !skewnesses.is_empty() {
            let n = skewnesses.len() as f64;
            metrics.avg_skewness = Some(skewnesses.iter().sum::<f64>() / n);
            metrics.worst_skewness = skewnesses
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min)
                .into();
        }

        if !kurtoses.is_empty() {
            let n = kurtoses.len() as f64;
            metrics.avg_kurtosis = Some(kurtoses.iter().sum::<f64>() / n);
            metrics.max_kurtosis = kurtoses
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max)
                .into();
        }

        if !all_downside_ratios.is_empty() {
            let n = all_downside_ratios.len() as f64;
            metrics.downside_ratio = Some(all_downside_ratios.iter().sum::<f64>() / n);
        }

        metrics
    }

    /// Set the regime concentration penalty from a RegimeConcentrationScore.
    ///
    /// This should be called after computing regime analysis to integrate
    /// the concentration penalty into the robust score calculation.
    pub fn with_regime_concentration_penalty(mut self, penalty: f64) -> Self {
        self.regime_concentration_penalty = Some(penalty.clamp(0.0, 1.0));
        self
    }

    /// Set the regime concentration penalty in-place.
    pub fn set_regime_concentration_penalty(&mut self, penalty: f64) {
        self.regime_concentration_penalty = Some(penalty.clamp(0.0, 1.0));
    }

    /// Compute a composite robust score incorporating multiple metrics.
    ///
    /// Higher score = better. The score penalizes:
    /// - Low Sharpe
    /// - Poor hit rate
    /// - Large drawdowns
    /// - Fat tails (high CVaR, negative skewness, high kurtosis)
    pub fn robust_score(&self, config: &RobustScoreConfig) -> f64 {
        // Normalize each metric to roughly [0, 1] range before weighting
        // Sharpe: typical range [-1, 3], normalize by dividing by 2
        let sharpe_score = (self.avg_sharpe / 2.0).clamp(0.0, 1.5);

        // Min Sharpe penalty (want min_sharpe close to avg)
        let min_sharpe_score = (self.min_sharpe / 2.0).clamp(-0.5, 1.0);

        // Hit rate: already in [0, 1]
        let hit_rate_score = self.hit_rate;

        // Drawdown penalty: max_dd in [0, 1], want lower
        let drawdown_score = 1.0 - self.worst_max_drawdown.clamp(0.0, 1.0);

        // Tail risk penalties (only if available)
        let tail_risk_score = if let Some(cvar) = self.avg_cvar_95 {
            // CVaR typically 0.01-0.05 (1-5% daily loss)
            // Penalize high CVaR
            1.0 - (cvar * 20.0).clamp(0.0, 1.0)
        } else {
            0.5 // Neutral if not computed
        };

        let kurtosis_score = if let Some(kurt) = self.avg_kurtosis {
            // Excess kurtosis: 0 = normal, >3 is fat-tailed
            // Penalize high kurtosis
            1.0 - (kurt / 6.0).clamp(0.0, 1.0)
        } else {
            0.5 // Neutral if not computed
        };

        // Regime concentration score (0-1 penalty -> 1-0 score)
        let regime_score = if let Some(penalty) = self.regime_concentration_penalty {
            // penalty is 0-1 where higher = worse
            // score should be 1-0 where higher = better
            1.0 - penalty.clamp(0.0, 1.0)
        } else {
            0.5 // Neutral if not computed
        };

        // Weighted sum
        config.w_sharpe * sharpe_score
            + config.w_min_sharpe * min_sharpe_score
            + config.w_hitrate * hit_rate_score
            + config.w_drawdown * drawdown_score
            + config.w_tail_risk * tail_risk_score
            + config.w_kurtosis * kurtosis_score
            + config.w_regime_concentration * regime_score
    }
}

/// Configuration for computing a composite RobustScore.
///
/// Weights control the relative importance of each metric component.
/// All weights should sum to approximately 1.0 for interpretable scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobustScoreConfig {
    /// Weight for average Sharpe ratio
    pub w_sharpe: f64,
    /// Weight for minimum Sharpe (consistency penalty)
    pub w_min_sharpe: f64,
    /// Weight for hit rate (profitable symbol count)
    pub w_hitrate: f64,
    /// Weight for drawdown penalty
    pub w_drawdown: f64,
    /// Weight for tail risk (CVaR) penalty
    pub w_tail_risk: f64,
    /// Weight for kurtosis (fat tails) penalty
    pub w_kurtosis: f64,
    /// Weight for regime concentration penalty (penalizes strategies that only work in one regime)
    pub w_regime_concentration: f64,
}

impl Default for RobustScoreConfig {
    /// Balanced weights that emphasize Sharpe and hit rate while penalizing tail risk.
    fn default() -> Self {
        Self {
            w_sharpe: 0.25,
            w_min_sharpe: 0.10,
            w_hitrate: 0.20,
            w_drawdown: 0.15,
            w_tail_risk: 0.10,
            w_kurtosis: 0.10,
            w_regime_concentration: 0.10,
        }
    }
}

impl RobustScoreConfig {
    /// Conservative preset: heavy penalty on tail risk, drawdown, and regime concentration.
    pub fn conservative() -> Self {
        Self {
            w_sharpe: 0.15,
            w_min_sharpe: 0.15,
            w_hitrate: 0.15,
            w_drawdown: 0.20,
            w_tail_risk: 0.15,
            w_kurtosis: 0.05,
            w_regime_concentration: 0.15,
        }
    }

    /// Aggressive preset: prioritize Sharpe and hit rate, lower risk penalties.
    pub fn aggressive() -> Self {
        Self {
            w_sharpe: 0.40,
            w_min_sharpe: 0.05,
            w_hitrate: 0.25,
            w_drawdown: 0.10,
            w_tail_risk: 0.10,
            w_kurtosis: 0.05,
            w_regime_concentration: 0.05,
        }
    }
}

// =============================================================================
// Phase 3A: Combined Equity Realism
// =============================================================================

/// Weighting scheme for combining multiple equity curves.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CombinedEquityWeighting {
    /// Equal weight for all symbols (1/N)
    #[default]
    Equal,
    /// Weight by inverse volatility (lower vol gets higher weight)
    InverseVolatility,
    /// Risk parity: weight so each symbol contributes equal risk
    RiskParity,
    /// Weight by Sharpe ratio (better performers get higher weight)
    SharpeWeighted,
}

/// Aggregation method for combining returns across symbols.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CombinedEquityAggregation {
    /// Simple arithmetic mean of returns (additive)
    #[default]
    Arithmetic,
    /// Geometric mean of (1 + return) - 1 (multiplicative, better for compounding)
    Geometric,
}

/// Configuration for combining multiple equity curves into a portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedEquityConfig {
    /// How to weight different symbols
    pub weighting: CombinedEquityWeighting,
    /// How to aggregate returns
    pub aggregation: CombinedEquityAggregation,
    /// Initial capital for the combined portfolio
    pub initial_capital: f64,
    /// Minimum overlap days required between symbols (0 = use intersection)
    pub min_overlap_days: usize,
    /// Lookback period for volatility calculation (for InverseVol/RiskParity)
    pub volatility_lookback: usize,
}

impl Default for CombinedEquityConfig {
    fn default() -> Self {
        Self {
            weighting: CombinedEquityWeighting::Equal,
            aggregation: CombinedEquityAggregation::Arithmetic,
            initial_capital: 100_000.0,
            min_overlap_days: 20,
            volatility_lookback: 60, // ~3 months
        }
    }
}

impl CombinedEquityConfig {
    /// Risk-parity configuration with geometric aggregation.
    pub fn risk_parity() -> Self {
        Self {
            weighting: CombinedEquityWeighting::RiskParity,
            aggregation: CombinedEquityAggregation::Geometric,
            ..Default::default()
        }
    }

    /// Inverse volatility weighting with geometric aggregation.
    pub fn inverse_vol() -> Self {
        Self {
            weighting: CombinedEquityWeighting::InverseVolatility,
            aggregation: CombinedEquityAggregation::Geometric,
            ..Default::default()
        }
    }
}

/// Result of combining multiple equity curves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedEquityResult {
    /// Combined portfolio equity curve
    pub equity_curve: Vec<f64>,
    /// Aligned dates (intersection of all symbols)
    pub dates: Vec<DateTime<Utc>>,
    /// Per-symbol weights used
    pub weights: HashMap<String, f64>,
    /// Combined portfolio max drawdown (captures correlation effects)
    pub max_drawdown: f64,
    /// Combined portfolio Sharpe ratio
    pub sharpe: f64,
    /// Combined portfolio CAGR
    pub cagr: f64,
    /// Number of symbols included
    pub num_symbols: usize,
    /// Days of overlap used
    pub overlap_days: usize,
}

/// Combine multiple equity curves with proper date alignment and weighting.
///
/// This function implements Phase 3A "Combined Equity Realism":
/// - Proper date intersection handling (not just min length)
/// - Volatility-weighted or risk-parity weighting
/// - Geometric returns combination
/// - Combined equity drawdown (captures correlation)
///
/// # Arguments
/// * `per_symbol_equity` - Map of symbol -> equity curve values
/// * `per_symbol_dates` - Map of symbol -> timestamps for each equity point
/// * `config` - Configuration for weighting, aggregation, etc.
///
/// # Returns
/// Combined equity result with aligned dates and computed metrics
pub fn combine_equity_curves_realistic(
    per_symbol_equity: &HashMap<String, Vec<f64>>,
    per_symbol_dates: &HashMap<String, Vec<DateTime<Utc>>>,
    config: &CombinedEquityConfig,
) -> Option<CombinedEquityResult> {
    if per_symbol_equity.is_empty() {
        return None;
    }

    // Step 1: Find date intersection across all symbols
    let (aligned_indices, common_dates) =
        find_date_intersection(per_symbol_dates, config.min_overlap_days)?;

    if common_dates.len() < config.min_overlap_days.max(20) {
        return None; // Not enough overlap
    }

    // Step 2: Extract aligned equity curves and compute returns
    let mut aligned_equity: HashMap<String, Vec<f64>> = HashMap::new();
    let mut aligned_returns: HashMap<String, Vec<f64>> = HashMap::new();

    for (symbol, indices) in &aligned_indices {
        let equity = per_symbol_equity.get(symbol)?;
        let aligned: Vec<f64> = indices.iter().map(|&i| equity[i]).collect();

        // Compute daily returns
        let returns: Vec<f64> = aligned
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0].max(1e-10))
            .collect();

        aligned_equity.insert(symbol.clone(), aligned);
        aligned_returns.insert(symbol.clone(), returns);
    }

    // Step 3: Compute weights based on weighting scheme
    let weights = compute_weights(&aligned_returns, config);

    if weights.is_empty() {
        return None;
    }

    // Step 4: Combine returns using the specified aggregation method
    let combined_returns = combine_returns(&aligned_returns, &weights, config);

    // Step 5: Reconstruct equity curve from combined returns
    let mut combined_equity = vec![config.initial_capital; common_dates.len()];
    for (i, &ret) in combined_returns.iter().enumerate() {
        combined_equity[i + 1] = combined_equity[i] * (1.0 + ret);
    }

    // Step 6: Compute combined portfolio metrics
    let max_drawdown = compute_max_drawdown(&combined_equity);
    let sharpe = compute_sharpe_from_returns(&combined_returns, 252.0);
    let cagr = compute_cagr(&combined_equity, common_dates.len());

    Some(CombinedEquityResult {
        equity_curve: combined_equity,
        dates: common_dates,
        weights,
        max_drawdown,
        sharpe,
        cagr,
        num_symbols: aligned_equity.len(),
        overlap_days: aligned_indices
            .values()
            .next()
            .map(|v| v.len())
            .unwrap_or(0),
    })
}

/// Type alias for date intersection result to reduce type complexity.
type DateIntersectionResult = (HashMap<String, Vec<usize>>, Vec<DateTime<Utc>>);

/// Find the intersection of dates across all symbols.
///
/// Returns a map of symbol -> indices into their original equity curves,
/// plus the common dates vector.
fn find_date_intersection(
    per_symbol_dates: &HashMap<String, Vec<DateTime<Utc>>>,
    min_overlap: usize,
) -> Option<DateIntersectionResult> {
    if per_symbol_dates.is_empty() {
        return None;
    }

    // Build a set of dates that appear in ALL symbols
    let mut date_sets: Vec<std::collections::HashSet<DateTime<Utc>>> = per_symbol_dates
        .values()
        .map(|dates| dates.iter().cloned().collect())
        .collect();

    if date_sets.is_empty() {
        return None;
    }

    // Intersect all date sets
    let mut common_dates: std::collections::HashSet<DateTime<Utc>> = date_sets.remove(0);
    for set in date_sets {
        common_dates = common_dates.intersection(&set).cloned().collect();
    }

    if common_dates.len() < min_overlap {
        return None;
    }

    // Sort common dates
    let mut common_dates_vec: Vec<DateTime<Utc>> = common_dates.into_iter().collect();
    common_dates_vec.sort();

    // Build index maps for each symbol
    let common_set: std::collections::HashSet<DateTime<Utc>> =
        common_dates_vec.iter().cloned().collect();

    let mut aligned_indices: HashMap<String, Vec<usize>> = HashMap::new();

    for (symbol, dates) in per_symbol_dates {
        let mut indices: Vec<(DateTime<Utc>, usize)> = dates
            .iter()
            .enumerate()
            .filter(|(_, d)| common_set.contains(d))
            .map(|(i, d)| (*d, i))
            .collect();

        indices.sort_by_key(|(d, _)| *d);
        aligned_indices.insert(
            symbol.clone(),
            indices.into_iter().map(|(_, i)| i).collect(),
        );
    }

    Some((aligned_indices, common_dates_vec))
}

/// Compute weights for each symbol based on the weighting scheme.
fn compute_weights(
    aligned_returns: &HashMap<String, Vec<f64>>,
    config: &CombinedEquityConfig,
) -> HashMap<String, f64> {
    let n = aligned_returns.len() as f64;
    if n == 0.0 {
        return HashMap::new();
    }

    match config.weighting {
        CombinedEquityWeighting::Equal => {
            let w = 1.0 / n;
            aligned_returns.keys().map(|s| (s.clone(), w)).collect()
        }

        CombinedEquityWeighting::InverseVolatility => {
            // Compute volatility for each symbol (std of returns)
            let mut vols: HashMap<String, f64> = HashMap::new();
            for (symbol, returns) in aligned_returns {
                let vol = compute_volatility(returns, config.volatility_lookback);
                vols.insert(symbol.clone(), vol.max(1e-10)); // Avoid division by zero
            }

            // Weight by inverse volatility
            let total_inv_vol: f64 = vols.values().map(|v| 1.0 / v).sum();
            vols.into_iter()
                .map(|(s, v)| (s, (1.0 / v) / total_inv_vol))
                .collect()
        }

        CombinedEquityWeighting::RiskParity => {
            // Risk parity: weight so each symbol contributes equal marginal risk
            // Simplified version: inverse of volatility squared (variance)
            let mut vars: HashMap<String, f64> = HashMap::new();
            for (symbol, returns) in aligned_returns {
                let vol = compute_volatility(returns, config.volatility_lookback);
                vars.insert(symbol.clone(), (vol * vol).max(1e-10));
            }

            let total_inv_var: f64 = vars.values().map(|v| 1.0 / v).sum();
            vars.into_iter()
                .map(|(s, v)| (s, (1.0 / v) / total_inv_var))
                .collect()
        }

        CombinedEquityWeighting::SharpeWeighted => {
            // Weight by Sharpe ratio (zero or negative Sharpe gets small weight)
            let mut sharpes: HashMap<String, f64> = HashMap::new();
            for (symbol, returns) in aligned_returns {
                let sharpe = compute_sharpe_from_returns(returns, 252.0);
                // Shift to positive range: use max(sharpe, 0.1) to avoid zero weights
                sharpes.insert(symbol.clone(), sharpe.max(0.1));
            }

            let total_sharpe: f64 = sharpes.values().sum();
            if total_sharpe < 1e-10 {
                // Fallback to equal weight
                let w = 1.0 / n;
                return aligned_returns.keys().map(|s| (s.clone(), w)).collect();
            }

            sharpes
                .into_iter()
                .map(|(s, sh)| (s, sh / total_sharpe))
                .collect()
        }
    }
}

/// Combine returns from multiple symbols using specified aggregation method.
fn combine_returns(
    aligned_returns: &HashMap<String, Vec<f64>>,
    weights: &HashMap<String, f64>,
    config: &CombinedEquityConfig,
) -> Vec<f64> {
    if aligned_returns.is_empty() || weights.is_empty() {
        return vec![];
    }

    // Get the length of returns (should be same for all symbols after alignment)
    let return_len = aligned_returns
        .values()
        .next()
        .map(|r| r.len())
        .unwrap_or(0);
    if return_len == 0 {
        return vec![];
    }

    let mut combined = vec![0.0; return_len];

    match config.aggregation {
        CombinedEquityAggregation::Arithmetic => {
            // Weighted sum of returns
            for (symbol, returns) in aligned_returns {
                let w = weights.get(symbol).copied().unwrap_or(0.0);
                for (i, &ret) in returns.iter().enumerate() {
                    combined[i] += w * ret;
                }
            }
        }

        CombinedEquityAggregation::Geometric => {
            // Weighted geometric mean: exp(sum(w * log(1 + r))) - 1
            for (i, combined_val) in combined.iter_mut().enumerate().take(return_len) {
                let mut log_sum = 0.0;
                for (symbol, returns) in aligned_returns {
                    let w = weights.get(symbol).copied().unwrap_or(0.0);
                    let ret = returns.get(i).copied().unwrap_or(0.0);
                    // Clamp to avoid log of zero/negative
                    log_sum += w * (1.0 + ret).max(1e-10).ln();
                }
                *combined_val = log_sum.exp() - 1.0;
            }
        }
    }

    combined
}

/// Compute rolling volatility (standard deviation) of returns.
fn compute_volatility(returns: &[f64], lookback: usize) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    // Use the last `lookback` returns, or all if shorter
    let start = returns.len().saturating_sub(lookback);
    let window = &returns[start..];

    if window.len() < 2 {
        return 0.0;
    }

    let n = window.len() as f64;
    let mean = window.iter().sum::<f64>() / n;
    let variance = window.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    variance.sqrt()
}

/// Compute max drawdown from an equity curve.
fn compute_max_drawdown(equity: &[f64]) -> f64 {
    if equity.is_empty() {
        return 0.0;
    }

    let mut peak = equity[0];
    let mut max_dd = 0.0;

    for &val in equity {
        if val > peak {
            peak = val;
        }
        let dd = (peak - val) / peak;
        if dd > max_dd {
            max_dd = dd;
        }
    }

    max_dd
}

/// Compute annualized Sharpe ratio from daily returns.
fn compute_sharpe_from_returns(returns: &[f64], annualization: f64) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std = variance.sqrt();

    if std < 1e-10 {
        return 0.0;
    }

    (mean / std) * annualization.sqrt()
}

/// Compute CAGR from an equity curve.
fn compute_cagr(equity: &[f64], num_days: usize) -> f64 {
    if equity.len() < 2 || num_days == 0 {
        return 0.0;
    }

    let start = equity[0];
    let end = equity[equity.len() - 1];

    if start <= 0.0 {
        return 0.0;
    }

    let years = num_days as f64 / 252.0;
    if years < 0.01 {
        return 0.0;
    }

    (end / start).powf(1.0 / years) - 1.0
}

/// Simple equal-weighted combination for backward compatibility.
///
/// This is a convenience function that wraps `combine_equity_curves_realistic`
/// with default (equal weight, arithmetic) settings.
pub fn combine_equity_curves_simple(
    per_symbol_equity: &HashMap<String, Vec<f64>>,
    per_symbol_dates: &HashMap<String, Vec<DateTime<Utc>>>,
    initial_capital: f64,
) -> (Vec<f64>, Vec<DateTime<Utc>>) {
    let config = CombinedEquityConfig {
        initial_capital,
        ..Default::default()
    };

    match combine_equity_curves_realistic(per_symbol_equity, per_symbol_dates, &config) {
        Some(result) => (result.equity_curve, result.dates),
        None => {
            // Fallback to simple min-length approach for backward compatibility
            combine_equity_curves_legacy(per_symbol_equity, per_symbol_dates, initial_capital)
        }
    }
}

/// Legacy implementation for backward compatibility.
/// Uses min-length truncation without proper date alignment.
fn combine_equity_curves_legacy(
    per_symbol_equity: &HashMap<String, Vec<f64>>,
    per_symbol_dates: &HashMap<String, Vec<DateTime<Utc>>>,
    initial_capital: f64,
) -> (Vec<f64>, Vec<DateTime<Utc>>) {
    if per_symbol_equity.is_empty() {
        return (vec![], vec![]);
    }

    let min_len = per_symbol_equity
        .values()
        .map(|v| v.len())
        .min()
        .unwrap_or(0);

    if min_len == 0 {
        return (vec![], vec![]);
    }

    let dates: Vec<DateTime<Utc>> = per_symbol_dates
        .values()
        .next()
        .map(|d| d.iter().take(min_len).cloned().collect())
        .unwrap_or_default();

    let n_symbols = per_symbol_equity.len() as f64;
    let mut combined = vec![initial_capital; min_len];

    for equity in per_symbol_equity.values() {
        if equity.is_empty() {
            continue;
        }
        let start_value = equity[0];
        for (i, &val) in equity.iter().take(min_len).enumerate() {
            let return_pct = val / start_value;
            combined[i] += (initial_capital * return_pct - initial_capital) / n_symbols;
        }
    }

    (combined, dates)
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

    /// Which YOLO iteration found this config (all-time counter)
    pub iteration: u32,

    /// Session-relative iteration number (starts at 1 each session)
    #[serde(default)]
    pub session_iteration: Option<u32>,

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
            CrossSymbolRankMetric::CompositeScore(profile) => {
                // Delegate to aggregate_metrics.rank_value for composite score
                self.aggregate_metrics
                    .rank_value(CrossSymbolRankMetric::CompositeScore(profile))
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

    /// Maximum configs to keep per strategy type (default: 5)
    /// When set, leaderboard keeps top N configs for EACH strategy type
    #[serde(default = "default_max_per_strategy")]
    pub max_per_strategy: usize,

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

    /// Requested start date (user-configured) - may differ from actual data range
    #[serde(default)]
    pub requested_start: Option<chrono::NaiveDate>,

    /// Requested end date (user-configured) - may differ from actual data range
    #[serde(default)]
    pub requested_end: Option<chrono::NaiveDate>,
}

fn default_max_per_strategy() -> usize {
    5
}

impl Default for CrossSymbolLeaderboard {
    fn default() -> Self {
        Self::new(4, CrossSymbolRankMetric::AvgSharpe)
    }
}

impl CrossSymbolLeaderboard {
    /// Create a new empty cross-symbol leaderboard.
    pub fn new(max_entries: usize, rank_by: CrossSymbolRankMetric) -> Self {
        Self::with_max_per_strategy(max_entries, rank_by, 5)
    }

    /// Create a new leaderboard with custom max_per_strategy.
    pub fn with_max_per_strategy(
        max_entries: usize,
        rank_by: CrossSymbolRankMetric,
        max_per_strategy: usize,
    ) -> Self {
        let now = Utc::now();
        // Cap pre-allocation to avoid capacity overflow with usize::MAX
        let initial_capacity = max_entries.min(10_000);
        Self {
            entries: Vec::with_capacity(initial_capacity),
            max_entries,
            max_per_strategy,
            rank_by,
            total_iterations: 0,
            started_at: now,
            last_updated: now,
            total_configs_tested: 0,
            requested_start: None,
            requested_end: None,
        }
    }

    /// Set the requested date range (user-configured).
    /// This is the range the user asked for, which may differ from actual data coverage.
    pub fn with_requested_range(
        mut self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> Self {
        self.requested_start = Some(start);
        self.requested_end = Some(end);
        self
    }

    /// Set the requested date range on an existing leaderboard.
    pub fn set_requested_range(&mut self, start: chrono::NaiveDate, end: chrono::NaiveDate) {
        self.requested_start = Some(start);
        self.requested_end = Some(end);
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

    /// Try to insert an entry, keeping top N configs per strategy type.
    ///
    /// Unlike `try_insert`, this method allows multiple configs per strategy type,
    /// keeping up to `max_per_strategy` configs for each strategy. This is useful
    /// in YOLO mode to understand parameter sensitivity within each strategy family.
    ///
    /// Returns true if the entry was added or replaced an existing entry.
    pub fn try_insert_top_n_per_strategy(&mut self, entry: AggregatedConfigResult) -> bool {
        let strategy_type = entry.strategy_type;
        let hash = entry.config_hash();
        let new_value = entry.aggregate_metrics.rank_value(self.rank_by);

        // Check for duplicate config (same config_hash)
        if let Some(pos) = self.entries.iter().position(|e| e.config_hash() == hash) {
            let existing_value = self.entries[pos].aggregate_metrics.rank_value(self.rank_by);
            if new_value > existing_value {
                self.entries[pos] = entry;
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
            return false;
        }

        // Count entries for this strategy type
        let count_for_strategy = self
            .entries
            .iter()
            .filter(|e| e.strategy_type == strategy_type)
            .count();

        if count_for_strategy < self.max_per_strategy {
            // Room for more entries of this strategy type
            self.entries.push(entry);
            self.sort_and_rerank();
            self.last_updated = Utc::now();
            return true;
        }

        // Strategy has max entries - find the worst one for this strategy
        let worst_for_strategy = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.strategy_type == strategy_type)
            .min_by(|(_, a), (_, b)| {
                let va = a.aggregate_metrics.rank_value(self.rank_by);
                let vb = b.aggregate_metrics.rank_value(self.rank_by);
                va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some((worst_idx, worst_entry)) = worst_for_strategy {
            let worst_value = worst_entry.aggregate_metrics.rank_value(self.rank_by);
            if new_value > worst_value {
                self.entries.remove(worst_idx);
                self.entries.push(entry);
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
        }

        false
    }

    /// Get entries grouped by strategy type, sorted by rank within each group.
    pub fn entries_by_strategy(
        &self,
    ) -> std::collections::HashMap<crate::StrategyTypeId, Vec<&AggregatedConfigResult>> {
        use std::collections::HashMap;
        let mut grouped: HashMap<crate::StrategyTypeId, Vec<&AggregatedConfigResult>> =
            HashMap::new();
        for entry in &self.entries {
            grouped.entry(entry.strategy_type).or_default().push(entry);
        }
        // Sort each group by rank
        for entries in grouped.values_mut() {
            entries.sort_by_key(|e| e.rank);
        }
        grouped
    }

    /// Sort entries by rank metric (descending), truncate to max_entries, and update ranks.
    pub fn sort_and_rerank(&mut self) {
        let rank_by = self.rank_by;
        self.entries.sort_by(|a, b| {
            let va = a.aggregate_metrics.rank_value(rank_by);
            let vb = b.aggregate_metrics.rank_value(rank_by);
            vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to max_entries to prevent unbounded growth
        if self.entries.len() > self.max_entries {
            self.entries.truncate(self.max_entries);
        }

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
// Append-Only History Logger (YOLO Mode)
// =============================================================================

/// A single history entry for append-only logging.
/// Contains minimal but complete info about each tested config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Timestamp when this config was tested
    pub tested_at: DateTime<Utc>,
    /// YOLO iteration number
    pub iteration: u32,
    /// Strategy type
    pub strategy_type: StrategyTypeId,
    /// Configuration identifier
    pub config_id: StrategyConfigId,
    /// Hash for deduplication tracking
    pub config_hash: u64,
    /// Number of symbols tested
    pub symbol_count: usize,
    /// Average Sharpe ratio across symbols
    pub avg_sharpe: f64,
    /// Minimum Sharpe ratio across symbols
    pub min_sharpe: f64,
    /// Maximum Sharpe ratio across symbols
    pub max_sharpe: f64,
    /// Hit rate (% of symbols with positive CAGR)
    pub hit_rate: f64,
    /// Average CAGR across symbols
    pub avg_cagr: f64,
    /// Average max drawdown across symbols
    pub avg_max_drawdown: f64,

    // Date range for deduplication (added in v2)
    /// Start date of the backtest period
    #[serde(default)]
    pub tested_start: Option<NaiveDate>,
    /// End date of the backtest period
    #[serde(default)]
    pub tested_end: Option<NaiveDate>,
}

impl HistoryEntry {
    /// Create a history entry from an aggregated config result (without date range).
    pub fn from_aggregated(result: &AggregatedConfigResult, iteration: u32) -> Self {
        Self::from_aggregated_with_dates(result, iteration, None, None)
    }

    /// Create a history entry from an aggregated config result with date range.
    ///
    /// The date range is used for deduplication - configs tested with similar date
    /// ranges (±6 months combined) are considered duplicates.
    pub fn from_aggregated_with_dates(
        result: &AggregatedConfigResult,
        iteration: u32,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Self {
        Self {
            tested_at: Utc::now(),
            iteration,
            strategy_type: result.strategy_type,
            config_id: result.config_id.clone(),
            config_hash: result.config_hash(),
            symbol_count: result.symbols.len(),
            avg_sharpe: result.aggregate_metrics.avg_sharpe,
            min_sharpe: result.aggregate_metrics.min_sharpe,
            max_sharpe: result.aggregate_metrics.max_sharpe,
            hit_rate: result.aggregate_metrics.hit_rate,
            avg_cagr: result.aggregate_metrics.avg_cagr,
            avg_max_drawdown: result.aggregate_metrics.avg_max_drawdown,
            tested_start: start_date,
            tested_end: end_date,
        }
    }
}

/// Append-only logger for YOLO mode history.
///
/// Logs every tested config (not just winners) to a JSONL file.
/// Each line is a complete JSON object for easy analysis.
#[derive(Debug)]
pub struct HistoryLogger {
    /// Path to the JSONL log file
    path: std::path::PathBuf,
    /// Session ID for this run
    session_id: String,
    /// Number of entries logged
    entries_logged: u64,
}

impl HistoryLogger {
    /// Create a new history logger.
    ///
    /// Creates the parent directory if it doesn't exist.
    /// The file is opened in append mode for each write.
    pub fn new(path: impl AsRef<Path>, session_id: &str) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            path,
            session_id: session_id.to_string(),
            entries_logged: 0,
        })
    }

    /// Log a single config result (without date range).
    ///
    /// Appends a single JSON line to the log file.
    /// Thread-safe through OS-level atomic appends for small writes.
    pub fn log(&mut self, result: &AggregatedConfigResult, iteration: u32) -> io::Result<()> {
        self.log_with_dates(result, iteration, None, None)
    }

    /// Log a single config result with the backtest date range.
    ///
    /// The date range is used for deduplication - configs tested with similar date
    /// ranges (±6 months combined) can be skipped on subsequent runs.
    pub fn log_with_dates(
        &mut self,
        result: &AggregatedConfigResult,
        iteration: u32,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> io::Result<()> {
        use std::io::Write;

        let entry =
            HistoryEntry::from_aggregated_with_dates(result, iteration, start_date, end_date);
        let json = serde_json::to_string(&entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        writeln!(file, "{}", json)?;
        self.entries_logged += 1;

        Ok(())
    }

    /// Log multiple config results in a batch.
    ///
    /// More efficient than individual calls for many results.
    pub fn log_batch(
        &mut self,
        results: &[AggregatedConfigResult],
        iteration: u32,
    ) -> io::Result<()> {
        use std::io::Write;

        if results.is_empty() {
            return Ok(());
        }

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        for result in results {
            let entry = HistoryEntry::from_aggregated(result, iteration);
            let json = serde_json::to_string(&entry)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            writeln!(file, "{}", json)?;
            self.entries_logged += 1;
        }

        Ok(())
    }

    /// Get the number of entries logged so far.
    pub fn entries_logged(&self) -> u64 {
        self.entries_logged
    }

    /// Get the path to the log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
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
            session_iteration: None,
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
                // Tail risk fields
                avg_cvar_95: None,
                worst_cvar_95: None,
                avg_skewness: None,
                worst_skewness: None,
                avg_kurtosis: None,
                max_kurtosis: None,
                downside_ratio: None,
                regime_concentration_penalty: None,
            },
            combined_equity_curve: vec![100.0, 110.0, 120.0],
            dates: vec![],
            discovered_at: Utc::now(),
            iteration: 1,
            session_iteration: None,
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

    // =========================================================================
    // Phase 3A: Combined Equity Realism Tests
    // =========================================================================

    /// Helper to create test dates starting from a base date.
    fn make_dates(n: usize, start_offset_days: i64) -> Vec<DateTime<Utc>> {
        use chrono::Duration;
        let base = Utc::now() - Duration::days(start_offset_days);
        (0..n).map(|i| base + Duration::days(i as i64)).collect()
    }

    /// Helper to create a simple upward trending equity curve.
    fn make_equity_curve(n: usize, start: f64, daily_return: f64) -> Vec<f64> {
        let mut equity = Vec::with_capacity(n);
        let mut val = start;
        for _ in 0..n {
            equity.push(val);
            val *= 1.0 + daily_return;
        }
        equity
    }

    #[test]
    fn test_combine_equity_curves_simple() {
        let mut per_symbol_equity: HashMap<String, Vec<f64>> = HashMap::new();
        let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        // Create two symbols with identical date ranges
        let dates = make_dates(100, 100);
        per_symbol_equity.insert("AAPL".to_string(), make_equity_curve(100, 100_000.0, 0.001));
        per_symbol_equity.insert(
            "MSFT".to_string(),
            make_equity_curve(100, 100_000.0, 0.0005),
        );
        per_symbol_dates.insert("AAPL".to_string(), dates.clone());
        per_symbol_dates.insert("MSFT".to_string(), dates);

        let (combined, combined_dates) =
            combine_equity_curves_simple(&per_symbol_equity, &per_symbol_dates, 100_000.0);

        assert_eq!(combined.len(), 100);
        assert_eq!(combined_dates.len(), 100);
        // Starting value should be initial_capital
        assert!((combined[0] - 100_000.0).abs() < 1.0);
        // Should be trending up (average of two upward curves)
        assert!(combined[99] > combined[0]);
    }

    #[test]
    fn test_combine_equity_curves_realistic_equal_weight() {
        let mut per_symbol_equity: HashMap<String, Vec<f64>> = HashMap::new();
        let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        let dates = make_dates(100, 100);
        per_symbol_equity.insert("A".to_string(), make_equity_curve(100, 100_000.0, 0.001));
        per_symbol_equity.insert("B".to_string(), make_equity_curve(100, 100_000.0, 0.002));
        per_symbol_dates.insert("A".to_string(), dates.clone());
        per_symbol_dates.insert("B".to_string(), dates);

        let config = CombinedEquityConfig::default();
        let result =
            combine_equity_curves_realistic(&per_symbol_equity, &per_symbol_dates, &config)
                .expect("Should produce result");

        assert_eq!(result.num_symbols, 2);
        assert_eq!(result.weights.len(), 2);

        // Equal weights
        for weight in result.weights.values() {
            assert!((weight - 0.5).abs() < 0.01);
        }

        // Check metrics are computed
        assert!(result.max_drawdown >= 0.0);
        assert!(result.sharpe.is_finite());
        assert!(result.cagr.is_finite());
    }

    #[test]
    fn test_combine_equity_curves_inverse_vol_weighting() {
        let mut per_symbol_equity: HashMap<String, Vec<f64>> = HashMap::new();
        let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        let dates = make_dates(100, 100);
        // Low vol symbol (steady 0.1% daily)
        per_symbol_equity.insert(
            "LOW_VOL".to_string(),
            make_equity_curve(100, 100_000.0, 0.001),
        );
        // High vol symbol (noisy)
        let mut high_vol_equity: Vec<f64> = Vec::with_capacity(100);
        let mut val = 100_000.0;
        for i in 0..100 {
            high_vol_equity.push(val);
            // Alternate between +2% and -1.5% for high volatility
            val *= if i % 2 == 0 { 1.02 } else { 0.985 };
        }
        per_symbol_equity.insert("HIGH_VOL".to_string(), high_vol_equity);
        per_symbol_dates.insert("LOW_VOL".to_string(), dates.clone());
        per_symbol_dates.insert("HIGH_VOL".to_string(), dates);

        let config = CombinedEquityConfig::inverse_vol();
        let result =
            combine_equity_curves_realistic(&per_symbol_equity, &per_symbol_dates, &config)
                .expect("Should produce result");

        // Low vol should have higher weight
        let low_vol_weight = result.weights.get("LOW_VOL").unwrap();
        let high_vol_weight = result.weights.get("HIGH_VOL").unwrap();
        assert!(
            low_vol_weight > high_vol_weight,
            "Low vol should get higher weight: {} vs {}",
            low_vol_weight,
            high_vol_weight
        );
    }

    #[test]
    fn test_combine_equity_curves_risk_parity() {
        let mut per_symbol_equity: HashMap<String, Vec<f64>> = HashMap::new();
        let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        let dates = make_dates(100, 100);
        per_symbol_equity.insert("A".to_string(), make_equity_curve(100, 100_000.0, 0.001));
        per_symbol_equity.insert("B".to_string(), make_equity_curve(100, 100_000.0, 0.002));
        per_symbol_dates.insert("A".to_string(), dates.clone());
        per_symbol_dates.insert("B".to_string(), dates);

        let config = CombinedEquityConfig::risk_parity();
        let result =
            combine_equity_curves_realistic(&per_symbol_equity, &per_symbol_dates, &config)
                .expect("Should produce result");

        // Weights should sum to 1.0
        let total_weight: f64 = result.weights.values().sum();
        assert!(
            (total_weight - 1.0).abs() < 0.001,
            "Weights should sum to 1.0, got {}",
            total_weight
        );

        // With risk parity, lower variance gets higher weight
        assert!(result.weights.len() == 2);
    }

    #[test]
    fn test_combine_equity_curves_geometric_aggregation() {
        let mut per_symbol_equity: HashMap<String, Vec<f64>> = HashMap::new();
        let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        let dates = make_dates(50, 50);
        per_symbol_equity.insert("A".to_string(), make_equity_curve(50, 100_000.0, 0.01));
        per_symbol_equity.insert("B".to_string(), make_equity_curve(50, 100_000.0, 0.01));
        per_symbol_dates.insert("A".to_string(), dates.clone());
        per_symbol_dates.insert("B".to_string(), dates);

        // Arithmetic aggregation
        let arith_config = CombinedEquityConfig {
            aggregation: CombinedEquityAggregation::Arithmetic,
            ..Default::default()
        };
        let arith_result =
            combine_equity_curves_realistic(&per_symbol_equity, &per_symbol_dates, &arith_config)
                .expect("Should produce result");

        // Geometric aggregation
        let geo_config = CombinedEquityConfig {
            aggregation: CombinedEquityAggregation::Geometric,
            ..Default::default()
        };
        let geo_result =
            combine_equity_curves_realistic(&per_symbol_equity, &per_symbol_dates, &geo_config)
                .expect("Should produce result");

        // Both should have valid results
        assert!(!arith_result.equity_curve.is_empty());
        assert!(!geo_result.equity_curve.is_empty());

        // For identical returns, arithmetic and geometric should be similar
        // (they differ more when returns are volatile)
        let arith_final = arith_result.equity_curve.last().unwrap();
        let geo_final = geo_result.equity_curve.last().unwrap();
        // Within 5% of each other for this test case
        assert!(
            (arith_final - geo_final).abs() / arith_final < 0.05,
            "Expected similar results for identical returns"
        );
    }

    #[test]
    fn test_date_intersection_handles_different_ranges() {
        use chrono::Duration;

        let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        let base = Utc::now();
        // Symbol A: days 0-99
        per_symbol_dates.insert(
            "A".to_string(),
            (0..100).map(|i| base + Duration::days(i)).collect(),
        );
        // Symbol B: days 50-149 (overlaps days 50-99)
        per_symbol_dates.insert(
            "B".to_string(),
            (50..150).map(|i| base + Duration::days(i)).collect(),
        );

        let (aligned_indices, common_dates) =
            find_date_intersection(&per_symbol_dates, 10).expect("Should find intersection");

        // Should have 50 days of overlap (days 50-99)
        assert_eq!(common_dates.len(), 50);
        assert_eq!(aligned_indices.len(), 2);

        // Indices for A should be 50-99
        assert_eq!(aligned_indices.get("A").unwrap().len(), 50);
        assert_eq!(aligned_indices.get("A").unwrap()[0], 50);

        // Indices for B should be 0-49
        assert_eq!(aligned_indices.get("B").unwrap().len(), 50);
        assert_eq!(aligned_indices.get("B").unwrap()[0], 0);
    }

    #[test]
    fn test_date_intersection_returns_none_for_no_overlap() {
        use chrono::Duration;

        let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        let base = Utc::now();
        // Symbol A: days 0-49
        per_symbol_dates.insert(
            "A".to_string(),
            (0..50).map(|i| base + Duration::days(i)).collect(),
        );
        // Symbol B: days 100-149 (no overlap)
        per_symbol_dates.insert(
            "B".to_string(),
            (100..150).map(|i| base + Duration::days(i)).collect(),
        );

        let result = find_date_intersection(&per_symbol_dates, 10);
        assert!(result.is_none(), "Should return None for no overlap");
    }

    #[test]
    fn test_combined_drawdown_captures_correlation() {
        let mut per_symbol_equity: HashMap<String, Vec<f64>> = HashMap::new();
        let mut per_symbol_dates: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

        let dates = make_dates(100, 100);

        // Create two symbols that crash at the same time (correlated)
        let mut equity_a: Vec<f64> = Vec::with_capacity(100);
        let mut equity_b: Vec<f64> = Vec::with_capacity(100);
        let mut val_a = 100_000.0;
        let mut val_b = 100_000.0;
        for i in 0..100 {
            equity_a.push(val_a);
            equity_b.push(val_b);
            if (40..60).contains(&i) {
                // Both crash together (20% over 20 days)
                val_a *= 0.99;
                val_b *= 0.99;
            } else {
                val_a *= 1.002;
                val_b *= 1.002;
            }
        }

        per_symbol_equity.insert("A".to_string(), equity_a);
        per_symbol_equity.insert("B".to_string(), equity_b);
        per_symbol_dates.insert("A".to_string(), dates.clone());
        per_symbol_dates.insert("B".to_string(), dates);

        let config = CombinedEquityConfig::default();
        let result =
            combine_equity_curves_realistic(&per_symbol_equity, &per_symbol_dates, &config)
                .expect("Should produce result");

        // Combined drawdown should be significant (~18% for this correlated crash)
        assert!(
            result.max_drawdown > 0.15,
            "Combined drawdown {} should capture correlation",
            result.max_drawdown
        );
    }

    #[test]
    fn test_compute_volatility() {
        // Constant returns = zero volatility
        let constant_returns: Vec<f64> = vec![0.01; 50];
        let vol = compute_volatility(&constant_returns, 60);
        assert!(vol < 1e-10, "Constant returns should have near-zero vol");

        // Mixed returns should have non-zero volatility
        let mixed_returns: Vec<f64> = (0..50)
            .map(|i| if i % 2 == 0 { 0.02 } else { -0.01 })
            .collect();
        let vol = compute_volatility(&mixed_returns, 60);
        assert!(vol > 0.01, "Mixed returns should have positive vol");
    }

    #[test]
    fn test_compute_max_drawdown() {
        // No drawdown: monotonically increasing
        let increasing: Vec<f64> = (1..=100).map(|i| i as f64 * 100.0).collect();
        let dd = compute_max_drawdown(&increasing);
        assert!(
            dd < 1e-10,
            "Monotonically increasing should have no drawdown"
        );

        // 50% drawdown
        let with_drawdown: Vec<f64> = vec![100.0, 120.0, 60.0, 80.0];
        let dd = compute_max_drawdown(&with_drawdown);
        assert!((dd - 0.5).abs() < 0.01, "Expected 50% drawdown, got {}", dd);
    }

    #[test]
    fn test_compute_sharpe_from_returns() {
        // All positive returns should have positive Sharpe
        let positive_returns: Vec<f64> = vec![0.01; 100];
        let sharpe = compute_sharpe_from_returns(&positive_returns, 252.0);
        // With constant returns, Sharpe is infinite (std = 0), so this tests edge case
        assert!(
            sharpe == 0.0 || sharpe.is_nan() || sharpe.is_infinite(),
            "Constant returns result in degenerate Sharpe"
        );

        // Mixed but net positive returns
        let mixed: Vec<f64> = (0..252)
            .map(|i| if i % 3 == 0 { -0.005 } else { 0.008 })
            .collect();
        let sharpe = compute_sharpe_from_returns(&mixed, 252.0);
        assert!(
            sharpe > 0.0,
            "Net positive returns should have positive Sharpe"
        );
    }

    #[test]
    fn test_combined_equity_config_presets() {
        let default = CombinedEquityConfig::default();
        assert_eq!(default.weighting, CombinedEquityWeighting::Equal);
        assert_eq!(default.aggregation, CombinedEquityAggregation::Arithmetic);

        let risk_parity = CombinedEquityConfig::risk_parity();
        assert_eq!(risk_parity.weighting, CombinedEquityWeighting::RiskParity);
        assert_eq!(
            risk_parity.aggregation,
            CombinedEquityAggregation::Geometric
        );

        let inverse_vol = CombinedEquityConfig::inverse_vol();
        assert_eq!(
            inverse_vol.weighting,
            CombinedEquityWeighting::InverseVolatility
        );
        assert_eq!(
            inverse_vol.aggregation,
            CombinedEquityAggregation::Geometric
        );
    }

    #[test]
    fn test_robust_score_with_regime_concentration() {
        // Create metrics with different regime concentration penalties
        let low_concentration = AggregatedMetrics {
            avg_sharpe: 1.5,
            min_sharpe: 1.0,
            max_sharpe: 2.0,
            geo_mean_cagr: 0.15,
            avg_cagr: 0.15,
            worst_max_drawdown: 0.15,
            avg_max_drawdown: 0.10,
            profitable_count: 4,
            total_symbols: 5,
            hit_rate: 0.8,
            avg_trades: 50.0,
            avg_cvar_95: Some(0.02),
            worst_cvar_95: Some(0.03),
            avg_skewness: Some(-0.5),
            worst_skewness: Some(-1.0),
            avg_kurtosis: Some(1.0),
            max_kurtosis: Some(2.0),
            downside_ratio: Some(0.6),
            regime_concentration_penalty: Some(0.0), // No concentration
        };

        let mut high_concentration = low_concentration.clone();
        high_concentration.regime_concentration_penalty = Some(1.0); // Full concentration

        let config = RobustScoreConfig::default();

        let score_low = low_concentration.robust_score(&config);
        let score_high = high_concentration.robust_score(&config);

        // Low concentration should score higher than high concentration
        assert!(
            score_low > score_high,
            "Low concentration ({}) should score higher than high concentration ({})",
            score_low,
            score_high
        );

        // The difference should be approximately the weight * (1.0 - 0.0) = w_regime_concentration
        let expected_diff = config.w_regime_concentration;
        let actual_diff = score_low - score_high;
        assert!(
            (actual_diff - expected_diff).abs() < 0.01,
            "Score difference ({}) should be close to regime concentration weight ({})",
            actual_diff,
            expected_diff
        );
    }

    #[test]
    fn test_robust_score_config_presets_include_regime_concentration() {
        let default = RobustScoreConfig::default();
        let conservative = RobustScoreConfig::conservative();
        let aggressive = RobustScoreConfig::aggressive();

        // All presets should have regime concentration weight
        assert!(default.w_regime_concentration > 0.0);
        assert!(conservative.w_regime_concentration > 0.0);
        assert!(aggressive.w_regime_concentration > 0.0);

        // Conservative should weight regime concentration higher than aggressive
        assert!(
            conservative.w_regime_concentration > aggressive.w_regime_concentration,
            "Conservative ({}) should weight regime concentration higher than aggressive ({})",
            conservative.w_regime_concentration,
            aggressive.w_regime_concentration
        );

        // All weights should sum to approximately 1.0
        let default_sum = default.w_sharpe
            + default.w_min_sharpe
            + default.w_hitrate
            + default.w_drawdown
            + default.w_tail_risk
            + default.w_kurtosis
            + default.w_regime_concentration;
        assert!(
            (default_sum - 1.0).abs() < 0.01,
            "Default weights should sum to ~1.0, got {}",
            default_sum
        );
    }

    #[test]
    fn test_aggregated_metrics_regime_concentration_helpers() {
        let metrics = AggregatedMetrics::default();
        assert!(metrics.regime_concentration_penalty.is_none());

        // Test builder pattern
        let with_penalty = metrics.clone().with_regime_concentration_penalty(0.7);
        assert_eq!(with_penalty.regime_concentration_penalty, Some(0.7));

        // Test setter
        let mut mutable = AggregatedMetrics::default();
        mutable.set_regime_concentration_penalty(0.3);
        assert_eq!(mutable.regime_concentration_penalty, Some(0.3));

        // Test clamping
        let clamped = AggregatedMetrics::default().with_regime_concentration_penalty(1.5);
        assert_eq!(clamped.regime_concentration_penalty, Some(1.0));

        let clamped_low = AggregatedMetrics::default().with_regime_concentration_penalty(-0.5);
        assert_eq!(clamped_low.regime_concentration_penalty, Some(0.0));
    }

    // =========================================================================
    // RiskProfile and RankingWeights tests
    // =========================================================================

    #[test]
    fn test_risk_profile_cycling() {
        assert_eq!(RiskProfile::Balanced.next(), RiskProfile::Conservative);
        assert_eq!(RiskProfile::Conservative.next(), RiskProfile::Aggressive);
        assert_eq!(RiskProfile::Aggressive.next(), RiskProfile::TrendOptions);
        assert_eq!(RiskProfile::TrendOptions.next(), RiskProfile::Balanced);
    }

    #[test]
    fn test_risk_profile_display_names() {
        assert_eq!(RiskProfile::Balanced.display_name(), "Balanced");
        assert_eq!(RiskProfile::Conservative.display_name(), "Conservative");
        assert_eq!(RiskProfile::Aggressive.display_name(), "Aggressive");
        assert_eq!(RiskProfile::TrendOptions.display_name(), "TrendOptions");
    }

    #[test]
    fn test_ranking_weights_all_presets_sum_to_one() {
        let presets = [
            ("balanced", RankingWeights::balanced()),
            ("conservative", RankingWeights::conservative()),
            ("aggressive", RankingWeights::aggressive()),
            ("trend_options", RankingWeights::trend_options()),
        ];

        for (name, weights) in presets {
            assert!(
                weights.validate().is_ok(),
                "{} weights should sum to 1.0: {:?}",
                name,
                weights.validate()
            );
        }
    }

    #[test]
    fn test_ranking_weights_trend_options_emphasizes_hit_rate() {
        let trend = RankingWeights::trend_options();
        let balanced = RankingWeights::balanced();

        assert!(
            trend.w_hit_rate > balanced.w_hit_rate,
            "TrendOptions should emphasize hit rate more than balanced"
        );
        assert!(
            trend.w_oos_sharpe > balanced.w_oos_sharpe,
            "TrendOptions should emphasize OOS Sharpe more than balanced"
        );
        assert!(
            trend.w_max_consecutive_losses > balanced.w_max_consecutive_losses,
            "TrendOptions should emphasize consecutive losses more than balanced"
        );
    }

    #[test]
    fn test_ranking_weights_conservative_emphasizes_risk() {
        let conservative = RankingWeights::conservative();
        let aggressive = RankingWeights::aggressive();

        assert!(
            conservative.w_max_drawdown > aggressive.w_max_drawdown,
            "Conservative should weight drawdown higher than aggressive"
        );
        assert!(
            conservative.w_cvar_95 > aggressive.w_cvar_95,
            "Conservative should weight CVaR higher than aggressive"
        );
        assert!(
            conservative.w_min_sharpe > aggressive.w_min_sharpe,
            "Conservative should weight min Sharpe higher than aggressive"
        );
    }

    #[test]
    fn test_ranking_weights_to_robust_score_config() {
        let weights = RankingWeights::balanced();
        let config = weights.to_robust_score_config();

        // Verify the sum is approximately 1.0 (lossy conversion but should preserve total)
        let sum = config.w_sharpe
            + config.w_min_sharpe
            + config.w_hitrate
            + config.w_drawdown
            + config.w_tail_risk
            + config.w_kurtosis
            + config.w_regime_concentration;

        assert!(
            (sum - 1.0).abs() < 0.01,
            "Converted RobustScoreConfig weights should sum to ~1.0, got {}",
            sum
        );
    }

    #[test]
    fn test_composite_score_ranking() {
        let metrics = AggregatedMetrics {
            avg_sharpe: 1.5,
            min_sharpe: 1.0,
            max_sharpe: 2.0,
            geo_mean_cagr: 0.15,
            avg_cagr: 0.15,
            worst_max_drawdown: 0.15,
            avg_max_drawdown: 0.10,
            profitable_count: 4,
            total_symbols: 5,
            hit_rate: 0.8,
            avg_trades: 50.0,
            avg_cvar_95: Some(0.02),
            worst_cvar_95: Some(0.03),
            avg_skewness: Some(-0.5),
            worst_skewness: Some(-1.0),
            avg_kurtosis: Some(1.0),
            max_kurtosis: Some(2.0),
            downside_ratio: Some(0.6),
            regime_concentration_penalty: Some(0.1),
        };

        // Different profiles should produce different scores
        let balanced_score =
            metrics.rank_value(CrossSymbolRankMetric::CompositeScore(RiskProfile::Balanced));
        let trend_score = metrics.rank_value(CrossSymbolRankMetric::CompositeScore(
            RiskProfile::TrendOptions,
        ));
        let conservative_score = metrics.rank_value(CrossSymbolRankMetric::CompositeScore(
            RiskProfile::Conservative,
        ));

        // All scores should be positive for good metrics
        assert!(balanced_score > 0.0, "Balanced score should be positive");
        assert!(trend_score > 0.0, "TrendOptions score should be positive");
        assert!(
            conservative_score > 0.0,
            "Conservative score should be positive"
        );

        // Scores should differ based on profile weights
        // (exact ordering depends on metrics, just verify they're different)
        assert!(
            (balanced_score - trend_score).abs() > 0.001
                || (balanced_score - conservative_score).abs() > 0.001,
            "Different profiles should generally produce different scores"
        );
    }
}
