//! YOLO Mode commands - continuous auto-optimization via worker thread.
//!
//! YOLO mode runs parameter sweeps indefinitely with randomized parameters,
//! maintaining top-4 leaderboards. Operations are delegated to the worker thread.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;
use trendlab_core::{AggregatedMetrics, CrossSymbolLeaderboard, Leaderboard};
use trendlab_engine::worker::WorkerCommand;

use crate::error::GuiError;
use crate::state::AppState;

// ============================================================================
// Types (GUI-specific for JSON serialization)
// ============================================================================

/// Phase of YOLO mode operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum YoloPhase {
    #[default]
    Idle,
    Sweeping,
    Stopped,
}

/// Response from get_yolo_state.
#[derive(Debug, Clone, Serialize)]
pub struct YoloStateResponse {
    pub enabled: bool,
    pub phase: YoloPhase,
    pub iteration: u32,
    pub randomization_pct: f64,
    pub total_configs_tested: u64,
}

/// Response from start_yolo_mode.
#[derive(Debug, Clone, Serialize)]
pub struct StartYoloResponse {
    pub total_symbols: usize,
    pub total_strategies: usize,
}

/// Leaderboard entry for frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEntryResponse {
    pub rank: usize,
    pub strategy_type: String,
    pub config_id: String,
    pub symbol: Option<String>,
    pub sharpe: f64,
    pub cagr: f64,
    pub max_drawdown: f64,
    pub discovered_at: String,
    pub iteration: u32,
    pub confidence_grade: Option<String>,
    pub walk_forward_grade: Option<String>,
    pub mean_oos_sharpe: Option<f64>,
    pub sharpe_degradation: Option<f64>,
    pub pct_profitable_folds: Option<f64>,
}

/// Per-symbol leaderboard for frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardResponse {
    pub entries: Vec<LeaderboardEntryResponse>,
    pub max_entries: usize,
    pub total_iterations: u32,
    pub started_at: String,
    pub last_updated: String,
    pub total_configs_tested: u64,
}

/// Aggregated metrics for frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedMetricsResponse {
    pub avg_sharpe: f64,
    pub min_sharpe: f64,
    pub max_sharpe: f64,
    pub geo_mean_cagr: f64,
    pub avg_cagr: f64,
    pub worst_max_drawdown: f64,
    pub avg_max_drawdown: f64,
    pub hit_rate: f64,
    pub profitable_count: usize,
    pub total_symbols: usize,
    pub avg_trades: f64,
}

/// Cross-symbol leaderboard entry for frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregatedConfigResultResponse {
    pub rank: usize,
    pub strategy_type: String,
    pub config_id: String,
    pub symbols: Vec<String>,
    pub aggregate_metrics: AggregatedMetricsResponse,
    pub discovered_at: String,
    pub iteration: u32,
    pub confidence_grade: Option<String>,
    pub walk_forward_grade: Option<String>,
}

/// Cross-symbol leaderboard for frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossSymbolLeaderboardResponse {
    pub entries: Vec<AggregatedConfigResultResponse>,
    pub max_entries: usize,
    pub rank_by: String,
    pub total_iterations: u32,
    pub started_at: String,
    pub last_updated: String,
    pub total_configs_tested: u64,
}

/// Combined leaderboard response.
#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardsResponse {
    pub per_symbol: Option<LeaderboardResponse>,
    pub cross_symbol: Option<CrossSymbolLeaderboardResponse>,
}

// ============================================================================
// Conversion Helpers
// ============================================================================

fn convert_leaderboard(lb: &Leaderboard) -> LeaderboardResponse {
    LeaderboardResponse {
        entries: lb
            .entries
            .iter()
            .map(|e| LeaderboardEntryResponse {
                rank: e.rank,
                strategy_type: format!("{:?}", e.strategy_type),
                config_id: format!("{:?}", e.config),
                symbol: e.symbol.clone(),
                sharpe: e.metrics.sharpe,
                cagr: e.metrics.cagr,
                max_drawdown: e.metrics.max_drawdown,
                discovered_at: e.discovered_at.to_rfc3339(),
                iteration: e.iteration,
                confidence_grade: e.confidence_grade.map(|g| format!("{:?}", g)),
                walk_forward_grade: e.walk_forward_grade.map(|c| c.to_string()),
                mean_oos_sharpe: e.mean_oos_sharpe,
                sharpe_degradation: e.sharpe_degradation,
                pct_profitable_folds: e.pct_profitable_folds,
            })
            .collect(),
        max_entries: lb.max_entries,
        total_iterations: lb.total_iterations,
        started_at: lb.started_at.to_rfc3339(),
        last_updated: lb.last_updated.to_rfc3339(),
        total_configs_tested: lb.total_configs_tested,
    }
}

fn convert_aggregated_metrics(m: &AggregatedMetrics) -> AggregatedMetricsResponse {
    AggregatedMetricsResponse {
        avg_sharpe: m.avg_sharpe,
        min_sharpe: m.min_sharpe,
        max_sharpe: m.max_sharpe,
        geo_mean_cagr: m.geo_mean_cagr,
        avg_cagr: m.avg_cagr,
        worst_max_drawdown: m.worst_max_drawdown,
        avg_max_drawdown: m.avg_max_drawdown,
        hit_rate: m.hit_rate,
        profitable_count: m.profitable_count,
        total_symbols: m.total_symbols,
        avg_trades: m.avg_trades,
    }
}

fn convert_cross_symbol_leaderboard(lb: &CrossSymbolLeaderboard) -> CrossSymbolLeaderboardResponse {
    CrossSymbolLeaderboardResponse {
        entries: lb
            .entries
            .iter()
            .map(|e| AggregatedConfigResultResponse {
                rank: e.rank,
                strategy_type: format!("{:?}", e.strategy_type),
                config_id: format!("{:?}", e.config_id),
                symbols: e.symbols.clone(),
                aggregate_metrics: convert_aggregated_metrics(&e.aggregate_metrics),
                discovered_at: e.discovered_at.to_rfc3339(),
                iteration: e.iteration,
                confidence_grade: e.confidence_grade.map(|g| format!("{:?}", g)),
                walk_forward_grade: e.walk_forward_grade.map(|c| c.to_string()),
            })
            .collect(),
        max_entries: lb.max_entries,
        rank_by: format!("{:?}", lb.rank_by),
        total_iterations: lb.total_iterations,
        started_at: lb.started_at.to_rfc3339(),
        last_updated: lb.last_updated.to_rfc3339(),
        total_configs_tested: lb.total_configs_tested,
    }
}

// ============================================================================
// Commands
// ============================================================================

/// Get current YOLO state.
#[tauri::command]
pub fn get_yolo_state(state: State<'_, AppState>) -> YoloStateResponse {
    let engine_yolo = state.get_yolo_state();
    YoloStateResponse {
        enabled: engine_yolo.enabled,
        phase: if engine_yolo.enabled {
            YoloPhase::Sweeping
        } else {
            YoloPhase::Idle
        },
        iteration: engine_yolo.iteration,
        randomization_pct: 0.15, // Default
        total_configs_tested: 0, // TODO: track in engine
    }
}

/// Get leaderboards.
#[tauri::command]
pub fn get_leaderboard(state: State<'_, AppState>) -> LeaderboardsResponse {
    let per_symbol = state.get_per_symbol_leaderboard().map(|lb| convert_leaderboard(&lb));
    let cross_symbol = state.get_cross_symbol_leaderboard().map(|lb| convert_cross_symbol_leaderboard(&lb));

    LeaderboardsResponse {
        per_symbol,
        cross_symbol,
    }
}

/// Start YOLO mode via worker thread.
#[tauri::command]
pub fn start_yolo_mode(
    state: State<'_, AppState>,
    randomization_pct: f64,
) -> Result<StartYoloResponse, GuiError> {
    // Check if already running
    if state.is_yolo_running() {
        return Err(GuiError::InvalidState(
            "YOLO mode is already running".to_string(),
        ));
    }

    // Get selections
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

    // Clamp randomization
    let randomization_pct = randomization_pct.clamp(0.05, 0.50);

    // Get config from state
    let (start_date, end_date) = state.get_date_range();
    let core_cost_model = state.get_cost_model();

    // Build strategy grid
    let strategy_grid = build_strategy_grid(&strategies);

    // Build backtest config
    let backtest_config = trendlab_core::BacktestConfig {
        initial_cash: 100_000.0,
        fill_model: trendlab_core::FillModel::NextOpen,
        cost_model: core_cost_model,
        qty: 100.0,
        pyramid_config: trendlab_core::PyramidConfig::default(),
    };

    // Load existing leaderboards if any
    let per_symbol_leaderboard = state.get_per_symbol_leaderboard();
    let cross_symbol_leaderboard = state.get_cross_symbol_leaderboard();

    let total_symbols = symbols.len();
    let total_strategies = strategies.len();

    // Clear cancellation flag
    state.clear_cancel();

    // Send command to worker
    state
        .send_command(WorkerCommand::StartYoloMode {
            symbols,
            symbol_sector_ids: HashMap::new(),
            start: start_date,
            end: end_date,
            strategy_grid,
            backtest_config,
            randomization_pct,
            existing_per_symbol_leaderboard: per_symbol_leaderboard,
            existing_cross_symbol_leaderboard: cross_symbol_leaderboard,
            session_id: Some(format!("yolo-{}", chrono::Utc::now().timestamp_millis())),
        })
        .map_err(|e| GuiError::Internal(format!("Failed to start YOLO mode: {}", e)))?;

    Ok(StartYoloResponse {
        total_symbols,
        total_strategies,
    })
}

/// Stop YOLO mode.
#[tauri::command]
pub fn stop_yolo_mode(state: State<'_, AppState>) -> Result<(), GuiError> {
    if !state.is_yolo_running() {
        return Err(GuiError::InvalidState(
            "YOLO mode is not running".to_string(),
        ));
    }

    state.request_cancel();
    let _ = state.send_command(WorkerCommand::Cancel);
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

/// Build a MultiStrategyGrid from selected strategy IDs.
fn build_strategy_grid(strategies: &[String]) -> trendlab_core::MultiStrategyGrid {
    use trendlab_core::{MAType, OpeningPeriod, StrategyGridConfig, StrategyParams, StrategyTypeId};

    let configs: Vec<StrategyGridConfig> = strategies
        .iter()
        .filter_map(|id| {
            let (strategy_type, params) = match id.as_str() {
                "donchian" => (
                    StrategyTypeId::Donchian,
                    StrategyParams::Donchian {
                        entry_lookbacks: vec![10, 20, 30, 40, 55],
                        exit_lookbacks: vec![5, 10, 15, 20],
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
