//! Exploration tracking for YOLO mode - enables history-informed coverage.
//!
//! This module tracks which parameter configurations have been explored across sessions,
//! enabling YOLO mode to maximize coverage of the parameter space over time.

use crate::leaderboard::HistoryEntry;
use crate::sweep::{StrategyConfigId, StrategyTypeId};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::Path;

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
    /// Grid cell size (e.g., 0.1 = 10 bins per dimension)
    pub cell_size: f64,
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

impl StrategyCoverage {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cell_size,
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
    pub fn coverage_ratio(&self) -> f64 {
        let dimensions = 3; // Typical strategy has 2-4 params, use 3 as average
        let cells_per_dim = (1.0 / self.cell_size).ceil() as u64;
        let total_cells = cells_per_dim.pow(dimensions as u32);
        if total_cells == 0 {
            return 1.0;
        }
        self.visited_cells.len() as f64 / total_cells as f64
    }

    /// Find a random least-visited cell.
    pub fn find_least_visited_cell(&self, rng: &mut impl Rng, dimensions: usize) -> Vec<f64> {
        let cells_per_dim = (1.0 / self.cell_size).ceil() as u64;
        let total_cells = cells_per_dim.pow(dimensions as u32);

        // Find unvisited or least-visited cells
        let mut min_visits = u32::MAX;
        let mut candidates = Vec::new();

        // Sample random cells to find least-visited (more efficient than checking all)
        let sample_size = (total_cells as usize).min(1000);
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
    pub const CURRENT_VERSION: u32 = 1;

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
    pub fn load_or_default(path: &Path) -> Self {
        Self::load(path).unwrap_or_default()
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
    pub fn get_coverage_mut(&mut self, strategy_type: StrategyTypeId) -> &mut StrategyCoverage {
        self.coverage
            .entry(strategy_type)
            .or_insert_with(|| StrategyCoverage::new(0.1)) // 10% cell size = 10 bins per dimension
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
            ParamBounds::new("atr_period", 3.0, 100.0, 1.0),
            ParamBounds::new("multiplier", 0.5, 10.0, 0.1),
        ],
        StrategyTypeId::FiftyTwoWeekHighMomentum | StrategyTypeId::FiftyTwoWeekHighTrailing => {
            vec![
                ParamBounds::new("period", 20.0, 1000.0, 5.0),
                ParamBounds::new("entry_pct", 0.50, 1.0, 0.01),
                ParamBounds::new("exit_pct", 0.30, 0.99, 0.01),
            ]
        }
        StrategyTypeId::ParabolicSarFiltered | StrategyTypeId::ParabolicSarDelayed => vec![
            ParamBounds::new("af_start", 0.005, 0.20, 0.005),
            ParamBounds::new("af_step", 0.005, 0.20, 0.005),
            ParamBounds::new("af_max", 0.1, 1.0, 0.01),
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
}

impl Default for ExplorationConfig {
    fn default() -> Self {
        Self {
            force_random_every_n: 5,           // Force random every 5 iterations
            nonlocal_jump_probability: 0.15,   // 15% chance of non-local jump
            warmup_iterations: 50,             // 50 iterations before exploitation begins
        }
    }
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
pub fn select_exploration_mode_with_config(
    rng: &mut impl Rng,
    state: &ExplorationState,
    strategy_type: StrategyTypeId,
    iteration: u32,
    config: &ExplorationConfig,
) -> ExplorationMode {
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
    if config.force_random_every_n > 0 && iteration > 0 && iteration.is_multiple_of(config.force_random_every_n) {
        return ExplorationMode::PureRandom;
    }

    let coverage_ratio = state.coverage_ratio(strategy_type);
    let has_winners = state.has_winners(strategy_type);

    // NEW: More aggressive exploration probabilities
    // Reduced exploitation, increased random/coverage at all stages
    // Format: (local, exploit, random, coverage)
    let (local, exploit, random, coverage) = if coverage_ratio < 0.3 {
        // Early exploration: heavy random + coverage
        (0.15, 0.05, 0.40, 0.40)
    } else if coverage_ratio < 0.6 {
        // Mid exploration: still favor exploration over exploitation
        (0.20, 0.15, 0.35, 0.30)
    } else {
        // Late exploration: still maintain significant random component
        // OLD: (0.30, 0.35, 0.15, 0.20) - too much exploitation!
        // NEW: Balance exploitation with continued exploration
        (0.20, 0.25, 0.30, 0.25)
    };

    // If no winners, redistribute exploit probability to random
    let (local, exploit, random, _coverage) = if !has_winners {
        (local, 0.0, random + exploit, coverage)
    } else {
        (local, exploit, random, coverage)
    };

    let roll = rng.gen::<f64>();
    if roll < local {
        ExplorationMode::LocalJitter
    } else if roll < local + exploit {
        ExplorationMode::ExploitWinner
    } else if roll < local + exploit + random {
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
}
