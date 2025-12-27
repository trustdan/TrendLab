//! YOLO Mode leaderboard for tracking top-performing strategies.
//!
//! This module provides:
//! - LeaderboardEntry: A single discovered winning strategy
//! - Leaderboard: Maintains top N strategies by Sharpe ratio
//! - Persistence to/from JSON

use crate::metrics::Metrics;
use crate::sweep::{StrategyConfigId, StrategyTypeId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::Path;

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

        // Check for existing entry with same config
        if let Some(pos) = self.entries.iter().position(|e| e.config_hash() == hash) {
            // Same config exists - only replace if better Sharpe
            if entry.metrics.sharpe > self.entries[pos].metrics.sharpe {
                self.entries[pos] = entry;
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
            return false;
        }

        // New config - check if we should add it
        if self.entries.len() < self.max_entries {
            // Not full, just add
            self.entries.push(entry);
            self.sort_and_rerank();
            self.last_updated = Utc::now();
            return true;
        }

        // Full - check if better than worst
        if let Some(worst) = self.entries.last() {
            if entry.metrics.sharpe > worst.metrics.sharpe {
                // Replace worst
                self.entries.pop();
                self.entries.push(entry);
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
        }

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
        serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
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
        let log_sum: f64 = cagrs
            .iter()
            .map(|&c| (1.0 + c).max(1e-10).ln())
            .sum();
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

        // Check for existing entry with same config
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

        // New config
        if self.entries.len() < self.max_entries {
            self.entries.push(entry);
            self.sort_and_rerank();
            self.last_updated = Utc::now();
            return true;
        }

        // Full - check if better than worst
        if let Some(worst) = self.entries.last() {
            let worst_value = worst.aggregate_metrics.rank_value(self.rank_by);
            if new_value > worst_value {
                self.entries.pop();
                self.entries.push(entry);
                self.sort_and_rerank();
                self.last_updated = Utc::now();
                return true;
            }
        }

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
        serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Load from file or create new if file doesn't exist.
    pub fn load_or_new(path: &Path, max_entries: usize, rank_by: CrossSymbolRankMetric) -> Self {
        Self::load(path).unwrap_or_else(|_| Self::new(max_entries, rank_by))
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
            metrics: Metrics {
                sharpe,
                ..Default::default()
            },
            equity_curve: vec![100.0, 110.0, 120.0],
            dates: vec![],
            discovered_at: Utc::now(),
            iteration,
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
}
