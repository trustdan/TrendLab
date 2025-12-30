//! Sweep panel commands - parameter sweeps via worker thread.
//!
//! Sweep operations are delegated to the worker thread (same as TUI).
//! This module provides thin wrappers around the engine state.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::State;
use trendlab_core::SweepDepth as CoreSweepDepth;
use trendlab_engine::worker::WorkerCommand;

use crate::error::GuiError;
use crate::jobs::JobStatus;
use crate::state::AppState;

// ============================================================================
// Types (GUI-specific for JSON serialization)
// ============================================================================

/// Sweep depth level (controls parameter grid density).
/// Maps to trendlab_core::SweepDepth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SweepDepth {
    Quick,
    #[default]
    Normal,
    Deep,
    Exhaustive,
}

impl SweepDepth {
    pub fn name(&self) -> &'static str {
        match self {
            SweepDepth::Quick => "Quick",
            SweepDepth::Normal => "Normal",
            SweepDepth::Deep => "Deep",
            SweepDepth::Exhaustive => "Exhaustive",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            SweepDepth::Quick => "3-5 values per param, ~50 configs",
            SweepDepth::Normal => "5-8 values per param, ~200 configs",
            SweepDepth::Deep => "10-15 values per param, ~500 configs",
            SweepDepth::Exhaustive => "20+ values per param, ~2000+ configs",
        }
    }

    pub fn config_multiplier(&self) -> usize {
        match self {
            SweepDepth::Quick => 25,
            SweepDepth::Normal => 100,
            SweepDepth::Deep => 400,
            SweepDepth::Exhaustive => 1600,
        }
    }

    /// Convert to core sweep depth.
    pub fn to_core(&self) -> CoreSweepDepth {
        match self {
            SweepDepth::Quick => CoreSweepDepth::Quick,
            SweepDepth::Normal => CoreSweepDepth::Standard,
            SweepDepth::Deep => CoreSweepDepth::Comprehensive,
            SweepDepth::Exhaustive => CoreSweepDepth::Comprehensive,
        }
    }

    /// Convert from core sweep depth.
    pub fn from_core(depth: &CoreSweepDepth) -> Self {
        match depth {
            CoreSweepDepth::Quick => SweepDepth::Quick,
            CoreSweepDepth::Standard => SweepDepth::Normal,
            CoreSweepDepth::Comprehensive => SweepDepth::Deep,
        }
    }
}

/// Cost model configuration for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostModel {
    pub fees_bps: f64,
    pub slippage_bps: f64,
}

impl Default for CostModel {
    fn default() -> Self {
        Self {
            fees_bps: 5.0,
            slippage_bps: 5.0,
        }
    }
}

impl CostModel {
    pub fn to_core(&self) -> trendlab_core::CostModel {
        trendlab_core::CostModel {
            fees_bps_per_side: self.fees_bps,
            slippage_bps: self.slippage_bps,
        }
    }

    pub fn from_core(core: &trendlab_core::CostModel) -> Self {
        Self {
            fees_bps: core.fees_bps_per_side,
            slippage_bps: core.slippage_bps,
        }
    }
}

/// Date range for sweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: String,
    pub end: String,
}

impl Default for DateRange {
    fn default() -> Self {
        let end = chrono::Local::now().format("%Y-%m-%d").to_string();
        let start = (chrono::Local::now() - chrono::Duration::days(365 * 10))
            .format("%Y-%m-%d")
            .to_string();
        Self { start, end }
    }
}

/// Selection summary for display.
#[derive(Debug, Clone, Serialize)]
pub struct SelectionSummary {
    pub symbols: Vec<String>,
    pub strategies: Vec<String>,
    pub symbol_count: usize,
    pub strategy_count: usize,
    pub estimated_configs: usize,
    pub has_cached_data: bool,
}

/// Depth option for selector.
#[derive(Debug, Clone, Serialize)]
pub struct DepthOption {
    pub id: String,
    pub name: String,
    pub description: String,
    pub estimated_configs: usize,
}

/// Sweep state response for frontend.
#[derive(Debug, Clone, Serialize)]
pub struct SweepStateResponse {
    pub depth: SweepDepth,
    pub cost_model: CostModel,
    pub date_range: DateRange,
    pub is_running: bool,
    pub current_job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartSweepResponse {
    pub job_id: String,
    pub total_configs: usize,
}

// ============================================================================
// Commands
// ============================================================================

/// Get current selection summary for the sweep panel.
#[tauri::command]
pub fn get_selection_summary(state: State<'_, AppState>) -> SelectionSummary {
    let symbols = state.get_selected_tickers();
    let strategies = state.get_selected_strategies();
    let depth = SweepDepth::from_core(&state.get_sweep_depth());

    let symbol_count = symbols.len();
    let strategy_count = strategies.len();
    let base_configs = symbol_count * strategy_count * depth.config_multiplier();

    let cached = state.cached_symbols.read().unwrap();
    let has_cached_data = symbols.iter().all(|s| cached.contains(s));

    SelectionSummary {
        symbols,
        strategies,
        symbol_count,
        strategy_count,
        estimated_configs: base_configs,
        has_cached_data,
    }
}

/// Get available depth options.
#[tauri::command]
pub fn get_depth_options(state: State<'_, AppState>) -> Vec<DepthOption> {
    let symbols = state.get_selected_tickers();
    let strategies = state.get_selected_strategies();
    let base = symbols.len().max(1) * strategies.len().max(1);

    vec![
        DepthOption {
            id: "quick".to_string(),
            name: SweepDepth::Quick.name().to_string(),
            description: SweepDepth::Quick.description().to_string(),
            estimated_configs: base * SweepDepth::Quick.config_multiplier(),
        },
        DepthOption {
            id: "normal".to_string(),
            name: SweepDepth::Normal.name().to_string(),
            description: SweepDepth::Normal.description().to_string(),
            estimated_configs: base * SweepDepth::Normal.config_multiplier(),
        },
        DepthOption {
            id: "deep".to_string(),
            name: SweepDepth::Deep.name().to_string(),
            description: SweepDepth::Deep.description().to_string(),
            estimated_configs: base * SweepDepth::Deep.config_multiplier(),
        },
        DepthOption {
            id: "exhaustive".to_string(),
            name: SweepDepth::Exhaustive.name().to_string(),
            description: SweepDepth::Exhaustive.description().to_string(),
            estimated_configs: base * SweepDepth::Exhaustive.config_multiplier(),
        },
    ]
}

/// Get current sweep depth.
#[tauri::command]
pub fn get_sweep_depth(state: State<'_, AppState>) -> SweepDepth {
    SweepDepth::from_core(&state.get_sweep_depth())
}

/// Set sweep depth.
#[tauri::command]
pub fn set_sweep_depth(state: State<'_, AppState>, depth: SweepDepth) {
    state.set_sweep_depth(depth.to_core());
}

/// Get cost model.
#[tauri::command]
pub fn get_cost_model(state: State<'_, AppState>) -> CostModel {
    CostModel::from_core(&state.get_cost_model())
}

/// Set cost model.
#[tauri::command]
pub fn set_cost_model(state: State<'_, AppState>, cost_model: CostModel) {
    state.set_cost_model(cost_model.to_core());
}

/// Get date range.
#[tauri::command]
pub fn get_date_range(state: State<'_, AppState>) -> DateRange {
    let (start, end) = state.get_date_range();
    DateRange {
        start: start.format("%Y-%m-%d").to_string(),
        end: end.format("%Y-%m-%d").to_string(),
    }
}

/// Set date range.
#[tauri::command]
pub fn set_date_range(state: State<'_, AppState>, date_range: DateRange) -> Result<(), GuiError> {
    let start = parse_date(&date_range.start)?;
    let end = parse_date(&date_range.end)?;
    state.set_date_range(start, end);
    Ok(())
}

/// Get sweep state (is running, current progress).
#[tauri::command]
pub fn get_sweep_state(state: State<'_, AppState>) -> SweepStateResponse {
    let (start, end) = state.get_date_range();
    SweepStateResponse {
        depth: SweepDepth::from_core(&state.get_sweep_depth()),
        cost_model: CostModel::from_core(&state.get_cost_model()),
        date_range: DateRange {
            start: start.format("%Y-%m-%d").to_string(),
            end: end.format("%Y-%m-%d").to_string(),
        },
        is_running: state.is_sweep_running(),
        current_job_id: None,
    }
}

/// Start a parameter sweep via the worker thread.
#[tauri::command]
pub fn start_sweep(state: State<'_, AppState>) -> Result<StartSweepResponse, GuiError> {
    // Check if already running
    if state.is_sweep_running() {
        return Err(GuiError::InvalidState(
            "A sweep is already running".to_string(),
        ));
    }

    let symbols = state.get_selected_tickers();
    let strategies = state.get_selected_strategies();

    if symbols.is_empty() {
        return Err(GuiError::InvalidInput {
            message: "No symbols selected. Go to Data panel to select symbols.".to_string(),
        });
    }

    if strategies.is_empty() {
        return Err(GuiError::InvalidInput {
            message: "No strategies selected. Go to Strategy panel to select strategies."
                .to_string(),
        });
    }

    // Get sweep config from state
    let (start_date, end_date) = state.get_date_range();
    let core_cost_model = state.get_cost_model();

    // Build strategy grid from selected strategies
    let strategy_grid = build_strategy_grid(&strategies, &state.get_sweep_depth());

    // Build backtest config
    let backtest_config = trendlab_core::BacktestConfig {
        initial_cash: 100_000.0,
        fill_model: trendlab_core::FillModel::NextOpen,
        cost_model: core_cost_model,
        qty: 100.0,
        pyramid_config: trendlab_core::PyramidConfig::default(),
    };

    // Register job and set running status (GUI-side tracking)
    let (job_id, _token) = state.jobs.create_new("sweep");
    state.jobs.set_status(&job_id, JobStatus::Running);

    // Estimate total configs for UI progress (same multiplier as UI uses)
    let depth_multiplier = SweepDepth::from_core(&state.get_sweep_depth()).config_multiplier();
    let estimated_total = symbols.len() * strategies.len() * depth_multiplier;

    // Clear cancellation flag
    state.clear_cancel();

    // Send command to worker
    state
        .send_command(WorkerCommand::StartMultiStrategySweepFromParquet {
            symbols,
            start: start_date,
            end: end_date,
            strategy_grid,
            backtest_config,
        })
        .map_err(|e| GuiError::Internal(format!("Failed to start sweep: {}", e)))?;

    Ok(StartSweepResponse {
        job_id,
        total_configs: estimated_total,
    })
}

/// Cancel a running sweep.
#[tauri::command]
pub fn cancel_sweep(state: State<'_, AppState>) -> Result<(), GuiError> {
    if !state.is_sweep_running() {
        return Err(GuiError::InvalidState("No sweep is running".to_string()));
    }

    state.request_cancel();
    let _ = state.send_command(WorkerCommand::Cancel);
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn parse_date(s: &str) -> Result<NaiveDate, GuiError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| GuiError::InvalidInput {
        message: format!("Invalid date '{}': {}", s, e),
    })
}

/// Build a MultiStrategyGrid from selected strategy IDs and depth.
fn build_strategy_grid(
    strategies: &[String],
    depth: &CoreSweepDepth,
) -> trendlab_core::MultiStrategyGrid {
    use trendlab_core::{MAType, OpeningPeriod, StrategyGridConfig, StrategyParams, StrategyTypeId};

    let configs: Vec<StrategyGridConfig> = strategies
        .iter()
        .filter_map(|id| {
            let (strategy_type, params) = match id.as_str() {
                "donchian" => (
                    StrategyTypeId::Donchian,
                    StrategyParams::Donchian {
                        entry_lookbacks: match depth {
                            CoreSweepDepth::Quick => vec![20, 40, 55],
                            CoreSweepDepth::Standard => vec![10, 20, 30, 40, 55],
                            CoreSweepDepth::Comprehensive => {
                                vec![10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60]
                            }
                        },
                        exit_lookbacks: match depth {
                            CoreSweepDepth::Quick => vec![10, 20],
                            CoreSweepDepth::Standard => vec![5, 10, 15, 20],
                            CoreSweepDepth::Comprehensive => vec![5, 10, 15, 20, 25, 30],
                        },
                    },
                ),
                "keltner" => (
                    StrategyTypeId::Keltner,
                    StrategyParams::Keltner {
                        ema_periods: vec![10, 20, 30],
                        atr_periods: vec![10, 14, 20],
                        multipliers: vec![1.5, 2.0, 2.5],
                    },
                ),
                "starc" => (
                    StrategyTypeId::STARC,
                    StrategyParams::STARC {
                        sma_periods: vec![5, 10, 20],
                        atr_periods: vec![14, 20],
                        multipliers: vec![1.5, 2.0, 2.5],
                    },
                ),
                "supertrend" => (
                    StrategyTypeId::Supertrend,
                    StrategyParams::Supertrend {
                        atr_periods: vec![10, 14, 20],
                        multipliers: vec![2.0, 3.0, 4.0],
                    },
                ),
                "ma_crossover" => (
                    StrategyTypeId::MACrossover,
                    StrategyParams::MACrossover {
                        fast_periods: vec![10, 20, 50],
                        slow_periods: vec![50, 100, 200],
                        ma_types: vec![MAType::SMA, MAType::EMA],
                    },
                ),
                "tsmom" => (
                    StrategyTypeId::Tsmom,
                    StrategyParams::Tsmom {
                        lookbacks: vec![21, 63, 126, 252],
                    },
                ),
                "turtle_s1" => (StrategyTypeId::TurtleS1, StrategyParams::TurtleS1),
                "turtle_s2" => (StrategyTypeId::TurtleS2, StrategyParams::TurtleS2),
                "parabolic_sar" => (
                    StrategyTypeId::ParabolicSar,
                    StrategyParams::ParabolicSar {
                        af_starts: vec![0.01, 0.02, 0.03],
                        af_steps: vec![0.02],
                        af_maxs: vec![0.15, 0.20, 0.25],
                    },
                ),
                "opening_range" => (
                    StrategyTypeId::OpeningRangeBreakout,
                    StrategyParams::OpeningRangeBreakout {
                        range_bars: vec![3, 5, 10],
                        periods: vec![OpeningPeriod::Weekly, OpeningPeriod::Monthly],
                    },
                ),
                _ => return None,
            };

            Some(StrategyGridConfig {
                strategy_type,
                enabled: true,
                params,
            })
        })
        .collect();

    trendlab_core::MultiStrategyGrid { strategies: configs }
}
