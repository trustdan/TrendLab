//! Exploration tracking for YOLO mode - enables history-informed coverage.
//!
//! This module tracks which parameter configurations have been explored across sessions,
//! enabling YOLO mode to maximize coverage of the parameter space over time.

use crate::leaderboard::HistoryEntry;
use crate::sweep::{StrategyConfigId, StrategyTypeId};
use chrono::{DateTime, NaiveDate, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::Path;

// =============================================================================
// Cell Granularity Constants
// =============================================================================

/// Default cell size for coverage tracking (50 bins per dimension).
/// Changed from 0.1 (10 bins) to 0.02 (50 bins) for finer-grained tracking.
pub const DEFAULT_CELL_SIZE: f64 = 0.02;

/// Minimum sample size for find_least_visited_cell().
const MIN_SAMPLE_SIZE: usize = 1000;

/// Maximum sample size for find_least_visited_cell().
const MAX_SAMPLE_SIZE: usize = 10000;

/// Reference cell count for effective_coverage_ratio normalization.
/// Based on the old 3D system with 10 bins: 10^3 = 1000 cells.
const REFERENCE_CELL_COUNT: f64 = 1000.0;

/// Parameter bounds for a single parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamBounds {
    /// Parameter name for debugging
    pub name: &'static str,
    /// Minimum valid value
    pub min: f64,
    /// Maximum valid value
    pub max: f64,
    /// Step size for discretization
    pub step: f64,
}

impl ParamBounds {
    pub const fn new(name: &'static str, min: f64, max: f64, step: f64) -> Self {
        Self {
            name,
            min,
            max,
            step,
        }
    }

    /// Normalize a value to [0, 1] range based on bounds.
    pub fn normalize(&self, value: f64) -> f64 {
        if (self.max - self.min).abs() < f64::EPSILON {
            0.5
        } else {
            ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }

    /// Denormalize a [0, 1] value back to the actual range.
    pub fn denormalize(&self, normalized: f64) -> f64 {
        let value = self.min + normalized * (self.max - self.min);
        // Round to step
        if self.step > 0.0 {
            (value / self.step).round() * self.step
        } else {
            value
        }
    }
}

/// Exploration mode determines how seed configs are generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExplorationMode {
    /// Jitter around the base grid (current behavior)
    LocalJitter,
    /// Jitter around a random historical winner
    ExploitWinner,
    /// Generate a completely random valid config
    PureRandom,
    /// Target the least-explored parameter region
    MaximizeCoverage,
}

impl ExplorationMode {
    /// Short name for display in status bar.
    pub fn short_name(&self) -> &'static str {
        match self {
            ExplorationMode::LocalJitter => "LOCAL",
            ExplorationMode::ExploitWinner => "EXPLOIT",
            ExplorationMode::PureRandom => "RANDOM",
            ExplorationMode::MaximizeCoverage => "COVER",
        }
    }
}

/// A normalized config in [0, 1]^N space for tracking coverage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedConfig {
    /// Strategy type this config belongs to
    pub strategy_type: StrategyTypeId,
    /// Normalized parameter values (each in [0, 1])
    pub params: Vec<f64>,
}

impl NormalizedConfig {
    /// Get the cell index for this config given a cell size.
    /// Cell size determines the granularity of coverage tracking.
    pub fn cell_index(&self, cell_size: f64) -> u64 {
        let mut index = 0u64;
        let cells_per_dim = (1.0 / cell_size).ceil() as u64;
        let mut multiplier = 1u64;

        for &p in &self.params {
            let cell = ((p / cell_size).floor() as u64).min(cells_per_dim - 1);
            index += cell * multiplier;
            multiplier *= cells_per_dim;
        }
        index
    }
}

/// Coverage statistics for a single strategy type.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StrategyCoverage {
    /// Grid cell size (e.g., 0.02 = 50 bins per dimension)
    pub cell_size: f64,
    /// Number of dimensions (parameters) for this strategy.
    /// Used for accurate coverage ratio calculation.
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
    /// Which cells have been visited (cell_index -> visit count)
    pub visited_cells: HashMap<u64, u32>,
    /// Total configs tested for this strategy
    pub total_tested: u64,
    /// Winner configs (positive Sharpe) for exploitation
    pub winner_configs: Vec<NormalizedConfig>,
    /// Maximum number of winners to track (to bound memory)
    #[serde(default = "default_max_winners")]
    pub max_winners: usize,
}

fn default_max_winners() -> usize {
    500
}

fn default_dimensions() -> usize {
    3 // Fallback for legacy data without dimensions field
}

impl StrategyCoverage {
    pub fn new(cell_size: f64, dimensions: usize) -> Self {
        Self {
            cell_size,
            dimensions: dimensions.max(1), // At least 1 dimension
            visited_cells: HashMap::new(),
            total_tested: 0,
            winner_configs: Vec::new(),
            max_winners: default_max_winners(),
        }
    }

    /// Record a tested configuration.
    pub fn record_test(&mut self, config: &NormalizedConfig, is_winner: bool) {
        let cell = config.cell_index(self.cell_size);
        *self.visited_cells.entry(cell).or_insert(0) += 1;
        self.total_tested += 1;

        if is_winner && self.winner_configs.len() < self.max_winners {
            self.winner_configs.push(config.clone());
        }
    }

    /// Calculate coverage ratio (fraction of cells visited at least once).
    /// Uses the actual dimension count for this strategy.
    pub fn coverage_ratio(&self) -> f64 {
        let cells_per_dim = (1.0 / self.cell_size).ceil() as u64;
        let total_cells = cells_per_dim.pow(self.dimensions as u32);
        if total_cells == 0 {
            return 1.0;
        }
        self.visited_cells.len() as f64 / total_cells as f64
    }

    /// Calculate effective coverage ratio for exploration mode selection.
    ///
    /// This normalizes coverage across different dimension counts so that
    /// exploration mode thresholds (0.3, 0.6) work consistently regardless
    /// of whether the strategy has 1, 2, or 3 parameters.
    ///
    /// We scale relative to the old 3D 10-bin reference (1000 cells) so that
    /// visiting the same absolute number of unique cells produces similar
    /// mode selection behavior.
    pub fn effective_coverage_ratio(&self) -> f64 {
        let visited = self.visited_cells.len() as f64;
        (visited / REFERENCE_CELL_COUNT).min(1.0)
    }

    /// Find a random least-visited cell.
    pub fn find_least_visited_cell(&self, rng: &mut impl Rng, dimensions: usize) -> Vec<f64> {
        let cells_per_dim = (1.0 / self.cell_size).ceil() as u64;
        let total_cells = cells_per_dim.pow(dimensions as u32);

        // Find unvisited or least-visited cells
        let mut min_visits = u32::MAX;
        let mut candidates = Vec::new();

        // Sample random cells to find least-visited.
        // Scale sample size with total cells: ~10% coverage, capped between MIN and MAX.
        let sample_size = (total_cells as usize / 10)
            .clamp(MIN_SAMPLE_SIZE, MAX_SAMPLE_SIZE)
            .min(total_cells as usize); // Never sample more than exists
        for _ in 0..sample_size {
            let cell_idx = rng.gen_range(0..total_cells);
            let visits = self.visited_cells.get(&cell_idx).copied().unwrap_or(0);

            if visits < min_visits {
                min_visits = visits;
                candidates.clear();
                candidates.push(cell_idx);
            } else if visits == min_visits {
                candidates.push(cell_idx);
            }
        }

        // Pick a random candidate and convert to normalized coordinates
        let chosen_cell = candidates[rng.gen_range(0..candidates.len())];
        cell_to_normalized(chosen_cell, cells_per_dim, dimensions, self.cell_size, rng)
    }
}

/// Convert a cell index back to normalized coordinates (with random position within cell).
fn cell_to_normalized(
    cell_idx: u64,
    cells_per_dim: u64,
    dimensions: usize,
    cell_size: f64,
    rng: &mut impl Rng,
) -> Vec<f64> {
    let mut result = Vec::with_capacity(dimensions);
    let mut remaining = cell_idx;

    for _ in 0..dimensions {
        let cell = remaining % cells_per_dim;
        remaining /= cells_per_dim;

        // Random position within the cell
        let base = cell as f64 * cell_size;
        let offset = rng.gen::<f64>() * cell_size;
        result.push((base + offset).clamp(0.0, 1.0));
    }
    result
}

/// Global exploration state persisted across sessions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExplorationState {
    /// Coverage tracking per strategy type
    pub coverage: HashMap<StrategyTypeId, StrategyCoverage>,
    /// Session IDs that contributed to this state
    pub contributing_sessions: Vec<String>,
    /// When this state was last updated
    pub last_updated: DateTime<Utc>,
    /// Schema version for future evolution
    pub version: u32,
}

impl ExplorationState {
    /// Schema version for exploration state.
    /// v1: Original with 0.1 cell size (10 bins)
    /// v2: Updated to 0.02 cell size (50 bins) + dimensions field
    pub const CURRENT_VERSION: u32 = 2;

    pub fn new() -> Self {
        Self {
            coverage: HashMap::new(),
            contributing_sessions: Vec::new(),
            last_updated: Utc::now(),
            version: Self::CURRENT_VERSION,
        }
    }

    /// Load exploration state from a file.
    pub fn load(path: &Path) -> io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Load or create a new exploration state.
    /// Automatically discards outdated versions (the state will be rebuilt from yolo_history).
    pub fn load_or_default(path: &Path) -> Self {
        match Self::load(path) {
            Ok(state) if state.version >= Self::CURRENT_VERSION => state,
            Ok(state) => {
                // Outdated version - discard and start fresh.
                // The state will be rebuilt from yolo_history/*.jsonl files anyway.
                tracing::info!(
                    old_version = state.version,
                    new_version = Self::CURRENT_VERSION,
                    "Discarding outdated exploration state (will rebuild from history)"
                );
                Self::new()
            }
            Err(_) => Self::new(),
        }
    }

    /// Save exploration state to a file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(path, json)
    }

    /// Record a session contribution.
    pub fn add_session(&mut self, session_id: &str) {
        if !self.contributing_sessions.contains(&session_id.to_string()) {
            self.contributing_sessions.push(session_id.to_string());
            // Keep only last 100 sessions
            if self.contributing_sessions.len() > 100 {
                self.contributing_sessions.remove(0);
            }
        }
        self.last_updated = Utc::now();
    }

    /// Get or create coverage for a strategy type.
    /// Uses DEFAULT_CELL_SIZE (50 bins) and the actual dimension count for the strategy.
    pub fn get_coverage_mut(&mut self, strategy_type: StrategyTypeId) -> &mut StrategyCoverage {
        let dimensions = get_param_bounds(strategy_type).len();
        self.coverage
            .entry(strategy_type)
            .or_insert_with(|| StrategyCoverage::new(DEFAULT_CELL_SIZE, dimensions.max(1)))
    }

    /// Record a tested configuration.
    pub fn record_test(
        &mut self,
        strategy_type: StrategyTypeId,
        config: &NormalizedConfig,
        is_winner: bool,
    ) {
        self.get_coverage_mut(strategy_type)
            .record_test(config, is_winner);
        self.last_updated = Utc::now();
    }

    /// Check if we have any winners for exploitation.
    pub fn has_winners(&self, strategy_type: StrategyTypeId) -> bool {
        self.coverage
            .get(&strategy_type)
            .is_some_and(|c| !c.winner_configs.is_empty())
    }

    /// Get a random winner config for exploitation.
    pub fn random_winner(
        &self,
        strategy_type: StrategyTypeId,
        rng: &mut impl Rng,
    ) -> Option<&NormalizedConfig> {
        self.coverage.get(&strategy_type).and_then(|c| {
            if c.winner_configs.is_empty() {
                None
            } else {
                Some(&c.winner_configs[rng.gen_range(0..c.winner_configs.len())])
            }
        })
    }

    /// Get coverage ratio for a strategy type.
    pub fn coverage_ratio(&self, strategy_type: StrategyTypeId) -> f64 {
        self.coverage
            .get(&strategy_type)
            .map_or(0.0, |c| c.coverage_ratio())
    }
}

/// Get parameter bounds for a strategy type.
/// These are WIDENED to encourage more diverse exploration.
/// The bounds define the full explorable parameter space.
pub fn get_param_bounds(strategy_type: StrategyTypeId) -> Vec<ParamBounds> {
    match strategy_type {
        StrategyTypeId::Donchian => vec![
            // WIDENED: was 5-200, now 5-500 for longer-term breakouts
            ParamBounds::new("entry_lookback", 5.0, 500.0, 5.0),
            // WIDENED: was 2-100, now 2-200
            ParamBounds::new("exit_lookback", 2.0, 200.0, 5.0),
        ],
        StrategyTypeId::MACrossover => vec![
            // WIDENED: was 5-100, now 3-200 for very fast crossovers
            ParamBounds::new("fast_period", 3.0, 200.0, 1.0),
            // WIDENED: was 20-500, now 10-1000 for very slow MAs
            ParamBounds::new("slow_period", 10.0, 1000.0, 5.0),
        ],
        // WIDENED: was 5-500, now 5-750 for multi-year momentum
        StrategyTypeId::Tsmom => vec![ParamBounds::new("lookback", 5.0, 750.0, 5.0)],
        StrategyTypeId::Supertrend => vec![
            // WIDENED: was 5-50, now 3-100 for more extreme periods
            ParamBounds::new("atr_period", 3.0, 100.0, 1.0),
            // WIDENED: was 1.0-5.0, now 0.5-10.0 for tighter/wider bands
            ParamBounds::new("multiplier", 0.5, 10.0, 0.1),
        ],
        StrategyTypeId::FiftyTwoWeekHigh => vec![
            // WIDENED: was 50-500, now 20-1000 (from ~1 month to 4 years)
            ParamBounds::new("period", 20.0, 1000.0, 5.0),
            // WIDENED: was 0.70-1.0, now 0.50-1.0 for earlier entries
            ParamBounds::new("entry_pct", 0.50, 1.0, 0.01),
            // WIDENED: was 0.40-0.95, now 0.30-0.99 for more exit flexibility
            ParamBounds::new("exit_pct", 0.30, 0.99, 0.01),
        ],
        StrategyTypeId::ParabolicSar => vec![
            // WIDENED: was 0.01-0.1, now 0.005-0.2 for slower/faster starts
            ParamBounds::new("af_start", 0.005, 0.20, 0.005),
            // WIDENED: was 0.01-0.1, now 0.005-0.2
            ParamBounds::new("af_step", 0.005, 0.20, 0.005),
            // WIDENED: was 0.1-0.5, now 0.1-1.0 for higher max acceleration
            ParamBounds::new("af_max", 0.1, 1.0, 0.01),
        ],
        StrategyTypeId::LarryWilliams => vec![
            // WIDENED: was 0.5-5.0, now 0.3-8.0
            ParamBounds::new("range_multiplier", 0.3, 8.0, 0.1),
            // WIDENED: was 0.5-5.0, now 0.3-8.0
            ParamBounds::new("atr_stop_mult", 0.3, 8.0, 0.1),
        ],
        StrategyTypeId::STARC => vec![
            // WIDENED: was 5-100, now 3-200
            ParamBounds::new("sma_period", 3.0, 200.0, 1.0),
            // WIDENED: was 5-50, now 3-100
            ParamBounds::new("atr_period", 3.0, 100.0, 1.0),
            // WIDENED: was 0.5-5.0, now 0.3-8.0
            ParamBounds::new("multiplier", 0.3, 8.0, 0.1),
        ],
        StrategyTypeId::Keltner => vec![
            // WIDENED: was 5-100, now 3-200
            ParamBounds::new("ema_period", 3.0, 200.0, 1.0),
            // WIDENED: was 5-50, now 3-100
            ParamBounds::new("atr_period", 3.0, 100.0, 1.0),
            // WIDENED: was 0.5-5.0, now 0.3-8.0
            ParamBounds::new("multiplier", 0.3, 8.0, 0.1),
        ],
        StrategyTypeId::DmiAdx => vec![
            // WIDENED: was 5-50, now 3-100
            ParamBounds::new("di_period", 3.0, 100.0, 1.0),
            // WIDENED: was 5-50, now 3-100
            ParamBounds::new("adx_period", 3.0, 100.0, 1.0),
            // WIDENED: was 10-50, now 5-70
            ParamBounds::new("adx_threshold", 5.0, 70.0, 1.0),
        ],
        // WIDENED: was 5-100, now 3-200
        StrategyTypeId::Aroon => vec![ParamBounds::new("period", 3.0, 200.0, 1.0)],
        StrategyTypeId::BollingerSqueeze => vec![
            // WIDENED: was 5-100, now 3-200
            ParamBounds::new("period", 3.0, 200.0, 1.0),
            // WIDENED: was 1.0-4.0, now 0.5-6.0
            ParamBounds::new("std_mult", 0.5, 6.0, 0.1),
            // WIDENED: was 0.01-0.5, now 0.005-0.8
            ParamBounds::new("squeeze_threshold", 0.005, 0.8, 0.01),
        ],
        StrategyTypeId::DarvasBox => {
            // WIDENED: was 2-20, now 1-30
            vec![ParamBounds::new("box_confirmation_bars", 1.0, 30.0, 1.0)]
        }
        // WIDENED: was 1-10, now 1-20
        StrategyTypeId::HeikinAshi => vec![ParamBounds::new("confirmation_bars", 1.0, 20.0, 1.0)],
        // Variants (use same WIDENED bounds as base)
        StrategyTypeId::SupertrendVolume
        | StrategyTypeId::SupertrendConfirmed
        | StrategyTypeId::SupertrendAsymmetric
        | StrategyTypeId::SupertrendCooldown => vec![
            // WIDENED: was 5-50, now 3-100
            ParamBounds::new("atr_period", 3.0, 100.0, 1.0),
            // WIDENED: was 1.0-5.0, now 0.5-10.0
            ParamBounds::new("multiplier", 0.5, 10.0, 0.1),
        ],
        StrategyTypeId::FiftyTwoWeekHighMomentum | StrategyTypeId::FiftyTwoWeekHighTrailing => {
            // Use same WIDENED bounds as base FiftyTwoWeekHigh
            vec![
                // WIDENED: was 20-260, now 10-300
                ParamBounds::new("period", 10.0, 300.0, 5.0),
                // WIDENED: was 0.85-1.0, now 0.80-1.0
                ParamBounds::new("entry_pct", 0.80, 1.0, 0.01),
                // WIDENED: was 0.60-0.95, now 0.50-0.95
                ParamBounds::new("exit_pct", 0.50, 0.95, 0.01),
            ]
        }
        StrategyTypeId::ParabolicSarFiltered | StrategyTypeId::ParabolicSarDelayed => vec![
            // Use same WIDENED bounds as base ParabolicSar
            // WIDENED: was 0.01-0.05, now 0.005-0.1
            ParamBounds::new("af_start", 0.005, 0.1, 0.005),
            // WIDENED: was 0.01-0.05, now 0.005-0.1
            ParamBounds::new("af_step", 0.005, 0.1, 0.005),
            // WIDENED: was 0.1-0.5, now 0.1-0.5 (already wide)
            ParamBounds::new("af_max", 0.1, 0.5, 0.01),
        ],
        // Strategies without clear parameter bounds or fixed params
        StrategyTypeId::TurtleS1 | StrategyTypeId::TurtleS2 => {
            vec![] // Fixed strategies
        }
        _ => vec![], // Other strategies not yet supported
    }
}

/// Normalize a StrategyConfigId to a NormalizedConfig.
pub fn normalize_config(config: &StrategyConfigId) -> Option<NormalizedConfig> {
    let strategy_type = config.strategy_type();
    let bounds = get_param_bounds(strategy_type);

    if bounds.is_empty() {
        return None;
    }

    let params = extract_config_params(config)?;
    if params.len() != bounds.len() {
        return None;
    }

    let normalized_params: Vec<f64> = params
        .iter()
        .zip(bounds.iter())
        .map(|(value, bound)| bound.normalize(*value))
        .collect();

    Some(NormalizedConfig {
        strategy_type,
        params: normalized_params,
    })
}

/// Extract raw parameter values from a StrategyConfigId.
fn extract_config_params(config: &StrategyConfigId) -> Option<Vec<f64>> {
    match config {
        StrategyConfigId::Donchian {
            entry_lookback,
            exit_lookback,
        } => Some(vec![*entry_lookback as f64, *exit_lookback as f64]),

        StrategyConfigId::MACrossover { fast, slow, .. } => Some(vec![*fast as f64, *slow as f64]),

        StrategyConfigId::Tsmom { lookback } => Some(vec![*lookback as f64]),

        StrategyConfigId::Supertrend {
            atr_period,
            multiplier,
        } => Some(vec![*atr_period as f64, *multiplier]),

        StrategyConfigId::FiftyTwoWeekHigh {
            period,
            entry_pct,
            exit_pct,
        } => Some(vec![*period as f64, *entry_pct, *exit_pct]),

        StrategyConfigId::ParabolicSar {
            af_start,
            af_step,
            af_max,
        } => Some(vec![*af_start, *af_step, *af_max]),

        StrategyConfigId::LarryWilliams {
            range_multiplier,
            atr_stop_mult,
        } => Some(vec![*range_multiplier, *atr_stop_mult]),

        StrategyConfigId::STARC {
            sma_period,
            atr_period,
            multiplier,
        } => Some(vec![*sma_period as f64, *atr_period as f64, *multiplier]),

        StrategyConfigId::Keltner {
            ema_period,
            atr_period,
            multiplier,
        } => Some(vec![*ema_period as f64, *atr_period as f64, *multiplier]),

        StrategyConfigId::DmiAdx {
            di_period,
            adx_period,
            adx_threshold,
        } => Some(vec![*di_period as f64, *adx_period as f64, *adx_threshold]),

        StrategyConfigId::Aroon { period } => Some(vec![*period as f64]),

        StrategyConfigId::BollingerSqueeze {
            period,
            std_mult,
            squeeze_threshold,
        } => Some(vec![*period as f64, *std_mult, *squeeze_threshold]),

        StrategyConfigId::DarvasBox {
            box_confirmation_bars,
        } => Some(vec![*box_confirmation_bars as f64]),

        StrategyConfigId::HeikinAshi { confirmation_bars } => Some(vec![*confirmation_bars as f64]),

        // Fixed strategies
        StrategyConfigId::TurtleS1 | StrategyConfigId::TurtleS2 => None,

        // Variants - extract same params as base
        StrategyConfigId::SupertrendVolume {
            atr_period,
            multiplier,
            ..
        }
        | StrategyConfigId::SupertrendConfirmed {
            atr_period,
            multiplier,
            ..
        }
        | StrategyConfigId::SupertrendCooldown {
            atr_period,
            multiplier,
            ..
        } => Some(vec![*atr_period as f64, *multiplier]),

        StrategyConfigId::SupertrendAsymmetric {
            atr_period,
            entry_multiplier,
            ..
        } => Some(vec![*atr_period as f64, *entry_multiplier]),

        StrategyConfigId::FiftyTwoWeekHighMomentum {
            period,
            entry_pct,
            exit_pct,
            ..
        } => Some(vec![*period as f64, *entry_pct, *exit_pct]),

        StrategyConfigId::FiftyTwoWeekHighTrailing {
            period,
            entry_pct,
            trailing_stop_pct,
        } => Some(vec![*period as f64, *entry_pct, *trailing_stop_pct]),

        StrategyConfigId::ParabolicSarFiltered {
            af_start,
            af_step,
            af_max,
            ..
        }
        | StrategyConfigId::ParabolicSarDelayed {
            af_start,
            af_step,
            af_max,
            ..
        } => Some(vec![*af_start, *af_step, *af_max]),

        _ => None, // Other strategies not yet supported
    }
}

/// Generate a random valid config for a strategy type.
pub fn generate_random_config(
    strategy_type: StrategyTypeId,
    rng: &mut impl Rng,
) -> Option<NormalizedConfig> {
    let bounds = get_param_bounds(strategy_type);
    if bounds.is_empty() {
        return None;
    }

    let params: Vec<f64> = bounds.iter().map(|_| rng.gen::<f64>()).collect();

    Some(NormalizedConfig {
        strategy_type,
        params,
    })
}

/// Denormalize a NormalizedConfig back to actual parameter values.
pub fn denormalize_to_params(config: &NormalizedConfig) -> Vec<f64> {
    let bounds = get_param_bounds(config.strategy_type);
    config
        .params
        .iter()
        .zip(bounds.iter())
        .map(|(normalized, bound)| bound.denormalize(*normalized))
        .collect()
}

/// Configuration for exploration mode selection.
#[derive(Debug, Clone, Copy)]
pub struct ExplorationConfig {
    /// Force pure random mode every N iterations (0 = disabled)
    pub force_random_every_n: u32,
    /// Probability of non-local jump (0.0-1.0) during jitter operations
    pub nonlocal_jump_probability: f64,
    /// Number of warmup iterations before winner exploitation begins (0 = no warmup)
    pub warmup_iterations: u32,

    // ExploitWinner decay parameters
    /// Initial probability of ExploitWinner mode (e.g., 0.25 = 25%)
    pub initial_exploit_pct: f64,
    /// Decay factor applied every `exploit_decay_interval` iterations (e.g., 0.9)
    pub exploit_decay_factor: f64,
    /// Number of iterations between decay steps (e.g., 100)
    pub exploit_decay_interval: u32,
    /// Minimum exploitation probability floor (e.g., 0.05 = 5%)
    pub exploit_floor_pct: f64,
}

impl Default for ExplorationConfig {
    fn default() -> Self {
        Self {
            force_random_every_n: 5,         // Force random every 5 iterations
            nonlocal_jump_probability: 0.15, // 15% chance of non-local jump
            warmup_iterations: 50,           // 50 iterations before exploitation begins

            // ExploitWinner decay: starts at 25%, decays by 0.9× every 100 iterations, floor at 5%
            initial_exploit_pct: 0.25,
            exploit_decay_factor: 0.9,
            exploit_decay_interval: 100,
            exploit_floor_pct: 0.05,
        }
    }
}

/// Calculate the current exploitation probability based on iteration count.
///
/// The exploitation probability decays over time to encourage broader exploration
/// as the search progresses. The formula is:
///
/// ```text
/// prob = max(floor, initial * decay_factor^((iteration - warmup) / interval))
/// ```
///
/// During warmup (iteration < warmup_iterations), returns 0.0.
///
/// Example decay schedule with defaults (initial=0.25, decay=0.9, interval=100, floor=0.05):
/// - Iterations 0-49: 0% (warmup)
/// - Iterations 50-149: 25%
/// - Iterations 150-249: 22.5%
/// - Iterations 250-349: 20.25%
/// - Iterations 1000+: ~5% (floor)
pub fn calculate_exploit_probability(iteration: u32, config: &ExplorationConfig) -> f64 {
    if iteration < config.warmup_iterations {
        return 0.0;
    }

    if config.exploit_decay_interval == 0 {
        return config.initial_exploit_pct;
    }

    let periods_elapsed = (iteration - config.warmup_iterations) / config.exploit_decay_interval;
    let decayed =
        config.initial_exploit_pct * config.exploit_decay_factor.powi(periods_elapsed as i32);
    decayed.max(config.exploit_floor_pct)
}

/// Select exploration mode based on coverage state.
/// Now accepts iteration number to periodically force pure random mode.
pub fn select_exploration_mode(
    rng: &mut impl Rng,
    state: &ExplorationState,
    strategy_type: StrategyTypeId,
) -> ExplorationMode {
    select_exploration_mode_with_config(rng, state, strategy_type, 0, &ExplorationConfig::default())
}

/// Select exploration mode with iteration awareness and configuration.
/// Forces pure random mode every `force_random_every_n` iterations.
/// During warmup phase, disables winner exploitation to allow broader exploration.
/// When nonlocal_jump_probability is very high (>=0.8), also forces more pure exploration.
pub fn select_exploration_mode_with_config(
    rng: &mut impl Rng,
    state: &ExplorationState,
    strategy_type: StrategyTypeId,
    iteration: u32,
    config: &ExplorationConfig,
) -> ExplorationMode {
    // High randomization mode: when user sets 80%+ randomization, force more exploration
    // This prevents tight clustering even after winners establish themselves
    if config.nonlocal_jump_probability >= 0.8 {
        let roll = rng.gen::<f64>();
        // 70% PureRandom, 20% MaximizeCoverage, 10% LocalJitter - never exploit
        return if roll < 0.70 {
            ExplorationMode::PureRandom
        } else if roll < 0.90 {
            ExplorationMode::MaximizeCoverage
        } else {
            ExplorationMode::LocalJitter
        };
    }

    // During warmup: only exploration, no exploitation
    // This prevents locking in early winners that may not be statistically significant
    if iteration < config.warmup_iterations {
        let roll = rng.gen::<f64>();
        return if roll < 0.50 {
            ExplorationMode::MaximizeCoverage
        } else if roll < 0.90 {
            ExplorationMode::PureRandom
        } else {
            ExplorationMode::LocalJitter
        };
    }

    // Force pure random mode every N iterations to break out of local optima
    if config.force_random_every_n > 0
        && iteration > 0
        && iteration.is_multiple_of(config.force_random_every_n)
    {
        return ExplorationMode::PureRandom;
    }

    // Use effective_coverage_ratio() for consistent mode selection across different
    // dimension counts. This normalizes to a 1000-cell reference (old 3D 10-bin grid).
    let coverage_ratio = state
        .coverage
        .get(&strategy_type)
        .map_or(0.0, |c| c.effective_coverage_ratio());
    let has_winners = state.has_winners(strategy_type);

    // Calculate decaying exploitation probability
    // Starts at initial_exploit_pct and decays toward exploit_floor_pct over time
    let exploit_prob = if has_winners {
        calculate_exploit_probability(iteration, config)
    } else {
        0.0
    };

    // Distribute remaining probability among other modes based on coverage
    // Format: (local, random, coverage) - exploit is handled separately
    let remaining = 1.0 - exploit_prob;
    let (local_ratio, random_ratio, coverage_ratio_target) = if coverage_ratio < 0.3 {
        // Early exploration: heavy random + coverage
        (0.15, 0.45, 0.40)
    } else if coverage_ratio < 0.6 {
        // Mid exploration: more balanced
        (0.20, 0.45, 0.35)
    } else {
        // Late exploration: still maintain significant random component
        (0.25, 0.40, 0.35)
    };

    // Scale to remaining probability
    let local = remaining * local_ratio;
    let random = remaining * random_ratio;
    let _coverage = remaining * coverage_ratio_target;

    let roll = rng.gen::<f64>();
    if roll < local {
        ExplorationMode::LocalJitter
    } else if roll < local + exploit_prob {
        ExplorationMode::ExploitWinner
    } else if roll < local + exploit_prob + random {
        ExplorationMode::PureRandom
    } else {
        ExplorationMode::MaximizeCoverage
    }
}

/// Build exploration state from existing YOLO history files.
///
/// Parses JSONL files from `artifacts/yolo_history/` directory and builds
/// coverage tracking from historical test results.
pub fn build_exploration_state_from_history(history_dir: &Path) -> io::Result<ExplorationState> {
    use std::io::BufRead;

    let mut state = ExplorationState::new();

    // Read all .jsonl files in the history directory
    if !history_dir.exists() {
        return Ok(state);
    }

    let entries = std::fs::read_dir(history_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            // Extract session ID from filename
            if let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) {
                state.add_session(session_id);

                // Parse the JSONL file
                let file = std::fs::File::open(&path)?;
                let reader = std::io::BufReader::new(file);

                for line in reader.lines().map_while(Result::ok) {
                    if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                        // Normalize the config and record the test
                        if let Some(normalized) = normalize_config(&entry.config_id) {
                            let is_winner = entry.avg_sharpe > 0.0;
                            state.record_test(entry.strategy_type, &normalized, is_winner);
                        }
                    }
                }
            }
        }
    }

    state.last_updated = Utc::now();
    Ok(state)
}

/// Record a single history entry into the exploration state.
pub fn record_history_entry(state: &mut ExplorationState, entry: &HistoryEntry) {
    if let Some(normalized) = normalize_config(&entry.config_id) {
        let is_winner = entry.avg_sharpe > 0.0;
        state.record_test(entry.strategy_type, &normalized, is_winner);
    }
}

// =============================================================================
// Config Deduplication Index
// =============================================================================

/// A date range when a config was tested.
#[derive(Debug, Clone)]
struct TestedDateRange {
    start: NaiveDate,
    end: NaiveDate,
}

/// Index of previously tested configs with their date ranges.
///
/// Used to skip configs that have already been tested with a "similar" date range.
/// Two date ranges are considered similar if |start1-start2| + |end1-end2| ≤ threshold_days.
#[derive(Debug, Default)]
pub struct TestedConfigsIndex {
    /// Map from config_hash to list of tested date ranges
    entries: HashMap<u64, Vec<TestedDateRange>>,
    /// Threshold for "similar" date ranges (combined start+end difference in days)
    /// Default is 180 days (6 months combined)
    threshold_days: i64,
}

impl TestedConfigsIndex {
    /// Create a new index with the default threshold (180 days).
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            threshold_days: 180,
        }
    }

    /// Create a new index with a custom threshold.
    pub fn with_threshold(threshold_days: i64) -> Self {
        Self {
            entries: HashMap::new(),
            threshold_days,
        }
    }

    /// Check if a config was already tested with a similar date range.
    ///
    /// Returns true if there exists a previous test where:
    /// |prev_start - start| + |prev_end - end| <= threshold_days
    pub fn was_tested_with_similar_range(
        &self,
        config_hash: u64,
        start: NaiveDate,
        end: NaiveDate,
    ) -> bool {
        if let Some(ranges) = self.entries.get(&config_hash) {
            for range in ranges {
                let start_diff = (range.start - start).num_days().abs();
                let end_diff = (range.end - end).num_days().abs();
                if start_diff + end_diff <= self.threshold_days {
                    return true;
                }
            }
        }
        false
    }

    /// Add a tested config with its date range to the index.
    pub fn add(&mut self, config_hash: u64, start: NaiveDate, end: NaiveDate) {
        self.entries
            .entry(config_hash)
            .or_default()
            .push(TestedDateRange { start, end });
    }

    /// Get the number of unique configs in the index.
    pub fn unique_configs(&self) -> usize {
        self.entries.len()
    }

    /// Get the total number of test records (including duplicates).
    pub fn total_records(&self) -> usize {
        self.entries.values().map(|v| v.len()).sum()
    }
}

/// Build a TestedConfigsIndex from existing YOLO history files.
///
/// Parses JSONL files from `artifacts/yolo_history/` directory and builds
/// an index of all previously tested configs with their date ranges.
pub fn build_tested_configs_index(history_dir: &Path) -> io::Result<TestedConfigsIndex> {
    use std::io::BufRead;

    let mut index = TestedConfigsIndex::new();

    if !history_dir.exists() {
        return Ok(index);
    }

    let entries = std::fs::read_dir(history_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            let file = std::fs::File::open(&path)?;
            let reader = std::io::BufReader::new(file);

            for line in reader.lines().map_while(Result::ok) {
                if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                    // Only add if we have date information
                    if let (Some(start), Some(end)) = (entry.tested_start, entry.tested_end) {
                        index.add(entry.config_hash, start, end);
                    }
                }
            }
        }
    }

    tracing::info!(
        unique_configs = index.unique_configs(),
        total_records = index.total_records(),
        "Built tested configs index from history"
    );

    Ok(index)
}

// =============================================================================
// LHS Integration Helpers
// =============================================================================

use crate::latin_hypercube::generate_lhs_samples;

/// Generate Latin Hypercube Samples for a strategy type.
///
/// Returns samples in actual parameter space (not normalized), suitable for
/// direct use in building strategy configs.
///
/// # Arguments
/// * `strategy_type` - The strategy type to generate samples for
/// * `n_samples` - Number of samples to generate (typically 10-15)
/// * `rng` - Random number generator
///
/// # Returns
/// Vec of samples, where each sample is a Vec<f64> of parameter values
/// in the same order as get_param_bounds() returns.
/// Returns empty Vec if strategy has no defined bounds.
pub fn generate_lhs_for_strategy<R: Rng>(
    strategy_type: StrategyTypeId,
    n_samples: usize,
    rng: &mut R,
) -> Vec<Vec<f64>> {
    let bounds = get_param_bounds(strategy_type);
    if bounds.is_empty() {
        return Vec::new();
    }

    // Convert ParamBounds to (min, max, step) tuples for LHS
    let lhs_bounds: Vec<(f64, f64, f64)> = bounds.iter().map(|b| (b.min, b.max, b.step)).collect();

    generate_lhs_samples(n_samples, lhs_bounds, rng)
}

/// Convert LHS samples to NormalizedConfigs for exploration tracking.
///
/// Takes raw parameter values from LHS and normalizes them to [0,1] space.
pub fn lhs_samples_to_normalized(
    strategy_type: StrategyTypeId,
    samples: &[Vec<f64>],
) -> Vec<NormalizedConfig> {
    let bounds = get_param_bounds(strategy_type);
    if bounds.is_empty() {
        return Vec::new();
    }

    samples
        .iter()
        .filter_map(|sample| {
            if sample.len() != bounds.len() {
                return None;
            }
            let normalized_params: Vec<f64> = sample
                .iter()
                .zip(bounds.iter())
                .map(|(value, bound)| bound.normalize(*value))
                .collect();
            Some(NormalizedConfig {
                strategy_type,
                params: normalized_params,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_param_bounds_normalize_denormalize() {
        let bounds = ParamBounds::new("test", 10.0, 50.0, 5.0);

        // Test boundary values
        assert!((bounds.normalize(10.0) - 0.0).abs() < 0.001);
        assert!((bounds.normalize(50.0) - 1.0).abs() < 0.001);
        assert!((bounds.normalize(30.0) - 0.5).abs() < 0.001);

        // Test round-trip
        let original = 25.0;
        let normalized = bounds.normalize(original);
        let denormalized = bounds.denormalize(normalized);
        assert!((denormalized - original).abs() < 0.001);
    }

    #[test]
    fn test_normalized_config_cell_index() {
        let config = NormalizedConfig {
            strategy_type: StrategyTypeId::Supertrend,
            params: vec![0.15, 0.75], // Two parameters
        };

        // With cell_size=0.1, param 0.15 is in cell 1, param 0.75 is in cell 7
        // Index = 1 + 7*10 = 71
        let index = config.cell_index(0.1);
        assert_eq!(index, 71);
    }

    #[test]
    fn test_exploration_mode_selection() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let mut rng = StdRng::seed_from_u64(42);
        let state = ExplorationState::new();

        // With empty state, should never get ExploitWinner
        for _ in 0..100 {
            let mode = select_exploration_mode(&mut rng, &state, StrategyTypeId::Supertrend);
            assert_ne!(mode, ExplorationMode::ExploitWinner);
        }
    }

    #[test]
    fn test_supertrend_bounds() {
        let bounds = get_param_bounds(StrategyTypeId::Supertrend);
        assert_eq!(bounds.len(), 2);
        assert_eq!(bounds[0].name, "atr_period");
        assert_eq!(bounds[1].name, "multiplier");
    }

    #[test]
    fn test_exploit_probability_decay() {
        let config = ExplorationConfig::default();

        // During warmup (0-49): 0%
        assert!((calculate_exploit_probability(0, &config) - 0.0).abs() < 0.001);
        assert!((calculate_exploit_probability(49, &config) - 0.0).abs() < 0.001);

        // First period after warmup (50-149): 25%
        assert!((calculate_exploit_probability(50, &config) - 0.25).abs() < 0.001);
        assert!((calculate_exploit_probability(149, &config) - 0.25).abs() < 0.001);

        // Second period (150-249): 25% * 0.9 = 22.5%
        assert!((calculate_exploit_probability(150, &config) - 0.225).abs() < 0.001);

        // Third period (250-349): 25% * 0.9^2 = 20.25%
        assert!((calculate_exploit_probability(250, &config) - 0.2025).abs() < 0.001);

        // Very late (floor at 5%)
        let late_prob = calculate_exploit_probability(10000, &config);
        assert!(
            (late_prob - 0.05).abs() < 0.001,
            "Expected floor of 5%, got {}",
            late_prob
        );
    }

    #[test]
    fn test_effective_coverage_ratio() {
        let mut coverage = StrategyCoverage::new(DEFAULT_CELL_SIZE, 2);

        // Empty coverage
        assert!((coverage.effective_coverage_ratio() - 0.0).abs() < 0.001);

        // Add 500 unique cells -> 500 / 1000 = 0.5
        for i in 0..500u64 {
            coverage.visited_cells.insert(i, 1);
        }
        assert!((coverage.effective_coverage_ratio() - 0.5).abs() < 0.001);

        // Add 600 more cells -> 1100 / 1000 = capped at 1.0
        for i in 500..1100u64 {
            coverage.visited_cells.insert(i, 1);
        }
        assert!((coverage.effective_coverage_ratio() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_tested_configs_index() {
        let mut index = TestedConfigsIndex::with_threshold(180); // 6 months combined

        let date1 = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();

        // Not tested yet
        assert!(!index.was_tested_with_similar_range(12345, date1, date2));

        // Add to index
        index.add(12345, date1, date2);

        // Exact match -> should be similar
        assert!(index.was_tested_with_similar_range(12345, date1, date2));

        // Within threshold (start +30, end -30 = 60 total diff < 180)
        let date1_shifted = NaiveDate::from_ymd_opt(2020, 2, 1).unwrap();
        let date2_shifted = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        assert!(index.was_tested_with_similar_range(12345, date1_shifted, date2_shifted));

        // Outside threshold (start +100, end -100 = 200 total diff > 180)
        let date1_far = NaiveDate::from_ymd_opt(2020, 4, 11).unwrap(); // +100 days
        let date2_far = NaiveDate::from_ymd_opt(2024, 9, 22).unwrap(); // -100 days
        assert!(!index.was_tested_with_similar_range(12345, date1_far, date2_far));

        // Different config hash
        assert!(!index.was_tested_with_similar_range(99999, date1, date2));
    }
}
