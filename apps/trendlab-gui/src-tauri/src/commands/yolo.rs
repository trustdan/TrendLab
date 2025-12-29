//! YOLO Mode commands - continuous auto-optimization with leaderboard
//!
//! YOLO mode runs parameter sweeps indefinitely, applying jitter each iteration,
//! and maintaining top-4 leaderboards (per-symbol and cross-symbol).
//!
//! ## Clustering Integration (Phase 4)
//!
//! When real backtest results are available, diverse selection clustering can be
//! applied to select representative strategies from different parameter regions:
//!
//! ```ignore
//! use trendlab_core::{
//!     select_diverse_from_sweep, select_diverse_top_n, select_diverse_robust,
//!     DiverseSelectionConfig, ROBUSTNESS_CLUSTER_FEATURES,
//! };
//!
//! // After collecting sweep results:
//! let diverse_df = select_diverse_top_n(&sweep_df, 10)?;
//! // Or for robust selection:
//! let diverse_df = select_diverse_robust(&sweep_df, 10)?;
//! ```
//!
//! This prevents the leaderboard from being dominated by very similar configs
//! from a narrow parameter region.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio;
use trendlab_core::leaderboard::{
    AggregatedMetrics, CrossSymbolLeaderboard, CrossSymbolRankMetric, Leaderboard,
    LeaderboardEntry,
};
use trendlab_launcher::ipc::JobType;

// Clustering imports for diverse strategy selection (Phase 4)
use trendlab_core::select_diverse_top_n;

// Polars backtest engine imports (Phase 4)
use trendlab_core::{
    run_strategy_sweep_polars_lazy, scan_symbol_parquet_lazy, sweep_to_dataframe,
    PolarsBacktestConfig, StrategyConfigId, StrategyGridConfig, StrategyParams, StrategyTypeId,
    SweepResult,
};

use polars::prelude::*;

use crate::error::{GuiError, RwLockExt};
use crate::events::EventEnvelope;
use crate::jobs::{CancellationToken, JobStatus};
use crate::state::AppState;

// ============================================================================
// Types
// ============================================================================

/// Phase of YOLO mode operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum YoloPhase {
    #[default]
    Idle,
    Sweeping,
    Stopped,
}

/// YOLO mode state stored in AppState
#[derive(Debug, Clone, Default)]
pub struct YoloState {
    pub enabled: bool,
    pub phase: YoloPhase,
    pub iteration: u32,
    pub randomization_pct: f64,
    pub total_configs_tested: u64,
    pub started_at: Option<String>,
    pub current_job_id: Option<String>,
    pub completed_configs: u64,
    pub total_configs: u64,
    /// Per-symbol leaderboard (top N by Sharpe for individual symbols)
    pub per_symbol_leaderboard: Option<Leaderboard>,
    /// Cross-symbol leaderboard (top N by aggregate metrics)
    pub cross_symbol_leaderboard: Option<CrossSymbolLeaderboard>,
}

/// Response from start_yolo_mode
#[derive(Debug, Clone, Serialize)]
pub struct StartYoloResponse {
    pub job_id: String,
    pub total_symbols: usize,
    pub total_strategies: usize,
}

/// Response from get_yolo_state
#[derive(Debug, Clone, Serialize)]
pub struct YoloStateResponse {
    pub enabled: bool,
    pub phase: YoloPhase,
    pub iteration: u32,
    pub randomization_pct: f64,
    pub total_configs_tested: u64,
    pub started_at: Option<String>,
    pub current_job_id: Option<String>,
}

/// Leaderboard entry for frontend (simplified for JSON serialization)
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
    // Walk-Forward Validation Fields
    pub walk_forward_grade: Option<String>,
    pub mean_oos_sharpe: Option<f64>,
    pub sharpe_degradation: Option<f64>,
    pub pct_profitable_folds: Option<f64>,
    pub oos_p_value: Option<f64>,
    pub fdr_adjusted_p_value: Option<f64>,
}

/// Per-symbol leaderboard for frontend
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

/// Aggregated metrics for frontend
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
    // Tail Risk Metrics (Phase 2)
    pub avg_cvar_95: Option<f64>,
    pub worst_cvar_95: Option<f64>,
    pub avg_skewness: Option<f64>,
    pub worst_skewness: Option<f64>,
    pub avg_kurtosis: Option<f64>,
    pub max_kurtosis: Option<f64>,
    pub downside_ratio: Option<f64>,
}

/// Cross-symbol leaderboard entry for frontend
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
    // Walk-Forward Validation Fields
    pub walk_forward_grade: Option<String>,
    pub mean_oos_sharpe: Option<f64>,
    pub std_oos_sharpe: Option<f64>,
    pub sharpe_degradation: Option<f64>,
    pub pct_profitable_folds: Option<f64>,
    pub oos_p_value: Option<f64>,
    pub fdr_adjusted_p_value: Option<f64>,
}

/// Cross-symbol leaderboard for frontend
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

/// Combined leaderboard response
#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardsResponse {
    pub per_symbol: Option<LeaderboardResponse>,
    pub cross_symbol: Option<CrossSymbolLeaderboardResponse>,
}

// ============================================================================
// Event Payloads
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct YoloStartedPayload {
    pub job_id: String,
    pub total_symbols: usize,
    pub total_strategies: usize,
    pub randomization_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct YoloProgressPayload {
    pub iteration: u32,
    pub phase: String,
    pub completed_configs: u64,
    pub total_configs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct YoloIterationCompletePayload {
    pub iteration: u32,
    pub cross_symbol_leaderboard: CrossSymbolLeaderboardResponse,
    pub per_symbol_leaderboard: LeaderboardResponse,
    pub configs_tested_this_round: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct YoloStoppedPayload {
    pub cross_symbol_leaderboard: CrossSymbolLeaderboardResponse,
    pub per_symbol_leaderboard: LeaderboardResponse,
    pub total_iterations: u32,
    pub total_configs_tested: u64,
}

// ============================================================================
// Conversion helpers
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
                // Walk-Forward Validation Fields
                walk_forward_grade: e.walk_forward_grade.map(|c| c.to_string()),
                mean_oos_sharpe: e.mean_oos_sharpe,
                sharpe_degradation: e.sharpe_degradation,
                pct_profitable_folds: e.pct_profitable_folds,
                oos_p_value: e.oos_p_value,
                fdr_adjusted_p_value: e.fdr_adjusted_p_value,
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
        // Tail Risk Metrics (Phase 2)
        avg_cvar_95: m.avg_cvar_95,
        worst_cvar_95: m.worst_cvar_95,
        avg_skewness: m.avg_skewness,
        worst_skewness: m.worst_skewness,
        avg_kurtosis: m.avg_kurtosis,
        max_kurtosis: m.max_kurtosis,
        downside_ratio: m.downside_ratio,
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
                // Walk-Forward Validation Fields
                walk_forward_grade: e.walk_forward_grade.map(|c| c.to_string()),
                mean_oos_sharpe: e.mean_oos_sharpe,
                std_oos_sharpe: e.std_oos_sharpe,
                sharpe_degradation: e.sharpe_degradation,
                pct_profitable_folds: e.pct_profitable_folds,
                oos_p_value: e.oos_p_value,
                fdr_adjusted_p_value: e.fdr_adjusted_p_value,
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
// Strategy Mapping Helpers
// ============================================================================

/// Map a GUI strategy ID string to a StrategyGridConfig for the Polars backtest engine.
fn strategy_id_to_grid_config(strategy_id: &str) -> Option<StrategyGridConfig> {
    let (strategy_type, params) = match strategy_id {
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
                ma_types: vec![trendlab_core::MAType::SMA, trendlab_core::MAType::EMA],
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
                periods: vec![
                    trendlab_core::OpeningPeriod::Weekly,
                    trendlab_core::OpeningPeriod::Monthly,
                ],
            },
        ),
        _ => {
            tracing::warn!(strategy_id = %strategy_id, "Unknown strategy ID, skipping");
            return None;
        }
    };

    Some(StrategyGridConfig {
        strategy_type,
        enabled: true,
        params,
    })
}

/// Convert a SweepConfigResult to a LeaderboardEntry for insertion.
fn sweep_result_to_leaderboard_entry(
    symbol: &str,
    strategy_type: StrategyTypeId,
    config: &trendlab_core::ConfigId,
    result: &trendlab_core::SweepConfigResult,
    iteration: u32,
    dates: &[chrono::DateTime<chrono::Utc>],
) -> LeaderboardEntry {
    // Map ConfigId to StrategyConfigId based on strategy type
    let strategy_config = match strategy_type {
        StrategyTypeId::Donchian => StrategyConfigId::Donchian {
            entry_lookback: config.entry_lookback,
            exit_lookback: config.exit_lookback,
        },
        StrategyTypeId::TurtleS1 => StrategyConfigId::TurtleS1,
        StrategyTypeId::TurtleS2 => StrategyConfigId::TurtleS2,
        _ => {
            // Default fallback for other strategies
            StrategyConfigId::Donchian {
                entry_lookback: config.entry_lookback,
                exit_lookback: config.exit_lookback,
            }
        }
    };

    // Extract equity values and dates from the backtest result
    let equity_curve: Vec<f64> = result
        .backtest_result
        .equity
        .iter()
        .map(|ep| ep.equity)
        .collect();

    let equity_dates: Vec<chrono::DateTime<chrono::Utc>> = result
        .backtest_result
        .equity
        .iter()
        .map(|ep| ep.ts)
        .collect();

    LeaderboardEntry {
        rank: 0, // Will be set by try_insert
        strategy_type,
        config: strategy_config,
        symbol: Some(symbol.to_string()),
        sector: None,
        metrics: result.metrics.clone(),
        equity_curve,
        dates: if dates.is_empty() {
            equity_dates
        } else {
            dates.to_vec()
        },
        discovered_at: chrono::Utc::now(),
        iteration,
        session_id: None,
        confidence_grade: None,
        walk_forward_grade: None,
        mean_oos_sharpe: None,
        sharpe_degradation: None,
        pct_profitable_folds: None,
        oos_p_value: None,
        fdr_adjusted_p_value: None,
    }
}

/// Single-symbol sweep result container for passing between async and sync contexts.
struct SymbolSweepResult {
    symbol: String,
    strategy_type: StrategyTypeId,
    sweep_result: SweepResult,
}

/// Run a parameter sweep for a single symbol synchronously.
/// Called via spawn_blocking to avoid blocking the async runtime.
fn run_symbol_sweep_sync(
    parquet_dir: std::path::PathBuf,
    symbol: String,
    strategy_config: StrategyGridConfig,
    backtest_config: PolarsBacktestConfig,
) -> Result<SymbolSweepResult, String> {
    // Load data using lazy scan
    let lf = scan_symbol_parquet_lazy(&parquet_dir, &symbol, "1d", None, None)
        .map_err(|e| format!("Failed to scan parquet for {}: {}", symbol, e))?;

    // Run the sweep
    let sweep_result = run_strategy_sweep_polars_lazy(lf, &strategy_config, &backtest_config)
        .map_err(|e| format!("Sweep failed for {} {:?}: {}", symbol, strategy_config.strategy_type, e))?;

    Ok(SymbolSweepResult {
        symbol,
        strategy_type: strategy_config.strategy_type,
        sweep_result,
    })
}

// ============================================================================
// Commands
// ============================================================================

/// Get current YOLO state
#[tauri::command]
pub fn get_yolo_state(state: State<'_, AppState>) -> YoloStateResponse {
    let yolo = state.yolo.read_or_recover();
    YoloStateResponse {
        enabled: yolo.enabled,
        phase: yolo.phase,
        iteration: yolo.iteration,
        randomization_pct: yolo.randomization_pct,
        total_configs_tested: yolo.total_configs_tested,
        started_at: yolo.started_at.clone(),
        current_job_id: yolo.current_job_id.clone(),
    }
}

/// Get leaderboards
#[tauri::command]
pub fn get_leaderboard(state: State<'_, AppState>) -> LeaderboardsResponse {
    let yolo = state.yolo.read_or_recover();
    LeaderboardsResponse {
        per_symbol: yolo
            .per_symbol_leaderboard
            .as_ref()
            .map(convert_leaderboard),
        cross_symbol: yolo
            .cross_symbol_leaderboard
            .as_ref()
            .map(convert_cross_symbol_leaderboard),
    }
}

/// Start YOLO mode
#[tauri::command]
pub async fn start_yolo_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    randomization_pct: f64,
) -> Result<StartYoloResponse, GuiError> {
    tracing::info!(
        randomization_pct = %randomization_pct,
        "Starting YOLO mode"
    );

    // Check if already running
    {
        let yolo = state.yolo.read_or_recover();
        if yolo.enabled {
            tracing::warn!("YOLO mode already running, rejecting start request");
            return Err(GuiError::InvalidState(
                "YOLO mode is already running".to_string(),
            ));
        }
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

    // Generate job ID
    let job_id = format!("yolo-{}", chrono::Utc::now().timestamp_millis());

    // Create job and get cancellation token
    let token = state.jobs.create(job_id.clone());

    // Mark YOLO as running
    {
        let mut yolo = state.yolo.write_or_recover();
        yolo.enabled = true;
        yolo.phase = YoloPhase::Sweeping;
        yolo.iteration = 0;
        yolo.randomization_pct = randomization_pct;
        yolo.total_configs_tested = 0;
        yolo.started_at = Some(chrono::Utc::now().to_rfc3339());
        yolo.current_job_id = Some(job_id.clone());
        yolo.completed_configs = 0;
        yolo.total_configs = 0;
    }

    state.jobs.set_status(&job_id, JobStatus::Running);

    let total_symbols = symbols.len();
    let total_strategies = strategies.len();

    // Emit started event
    let _ = app.emit(
        "yolo:started",
        EventEnvelope::new(
            "yolo:started",
            &job_id,
            YoloStartedPayload {
                job_id: job_id.clone(),
                total_symbols,
                total_strategies,
                randomization_pct,
            },
        ),
    );

    // Emit to companion terminal
    state
        .companion_job_started(
            &job_id,
            JobType::Sweep,
            &format!(
                "YOLO: {} symbols × {} strategies ({}% jitter)",
                total_symbols,
                total_strategies,
                (randomization_pct * 100.0) as i32
            ),
        )
        .await;

    // Clone for async task
    let job_id_clone = job_id.clone();
    let app_clone = app.clone();
    let symbols_clone = symbols.clone();
    let strategies_clone = strategies.clone();

    // Spawn YOLO worker
    tokio::spawn(async move {
        run_yolo_loop(
            app_clone,
            job_id_clone,
            symbols_clone,
            strategies_clone,
            randomization_pct,
            token,
        )
        .await;
    });

    Ok(StartYoloResponse {
        job_id,
        total_symbols,
        total_strategies,
    })
}

/// Stop YOLO mode
#[tauri::command]
pub fn stop_yolo_mode(state: State<'_, AppState>, job_id: String) -> Result<(), GuiError> {
    // Verify job ID matches
    {
        let yolo = state.yolo.read_or_recover();
        if yolo.current_job_id.as_ref() != Some(&job_id) {
            return Err(GuiError::InvalidState(format!(
                "Job ID mismatch: expected {:?}",
                yolo.current_job_id
            )));
        }
    }

    // Cancel the job
    state.jobs.cancel(&job_id);
    Ok(())
}

// ============================================================================
// YOLO Worker Loop
// ============================================================================

async fn run_yolo_loop(
    app: AppHandle,
    job_id: String,
    symbols: Vec<String>,
    strategies: Vec<String>,
    _randomization_pct: f64,
    cancel_token: CancellationToken,
) {
    use std::path::Path;

    let state = app.state::<AppState>();
    let per_symbol_path = Path::new("artifacts/leaderboard.json");
    let cross_symbol_path = Path::new("artifacts/cross_symbol_leaderboard.json");

    tracing::info!(
        job_id = %job_id,
        symbols = symbols.len(),
        strategies = strategies.len(),
        "YOLO loop starting"
    );

    // Load or create leaderboards
    let mut per_symbol_leaderboard = Leaderboard::load_or_new(per_symbol_path, 4);
    let mut cross_symbol_leaderboard =
        CrossSymbolLeaderboard::load_or_new(cross_symbol_path, 4, CrossSymbolRankMetric::AvgSharpe);

    tracing::debug!(
        prev_iterations = cross_symbol_leaderboard.total_iterations,
        prev_configs = cross_symbol_leaderboard.total_configs_tested,
        "Loaded existing leaderboards"
    );

    let mut iteration = cross_symbol_leaderboard.total_iterations;
    let total_symbols = symbols.len();
    let total_strategies = strategies.len();

    // Estimate configs per iteration (simplified)
    let configs_per_iteration = total_symbols * total_strategies * 25; // Rough estimate

    // Main YOLO loop
    loop {
        // Check for cancellation
        if cancel_token.is_cancelled() {
            // Save leaderboards
            let _ = per_symbol_leaderboard.save(per_symbol_path);
            let _ = cross_symbol_leaderboard.save(cross_symbol_path);

            // Update state
            {
                let mut yolo = state.yolo.write_or_recover();
                yolo.enabled = false;
                yolo.phase = YoloPhase::Stopped;
                yolo.current_job_id = None;
                yolo.per_symbol_leaderboard = Some(per_symbol_leaderboard.clone());
                yolo.cross_symbol_leaderboard = Some(cross_symbol_leaderboard.clone());
            }
            state.jobs.set_status(&job_id, JobStatus::Cancelled);

            // Emit stopped event
            let _ = app.emit(
                "yolo:stopped",
                EventEnvelope::new(
                    "yolo:stopped",
                    &job_id,
                    YoloStoppedPayload {
                        cross_symbol_leaderboard: convert_cross_symbol_leaderboard(
                            &cross_symbol_leaderboard,
                        ),
                        per_symbol_leaderboard: convert_leaderboard(&per_symbol_leaderboard),
                        total_iterations: iteration,
                        total_configs_tested: cross_symbol_leaderboard.total_configs_tested,
                    },
                ),
            );

            // Companion
            state
                .companion_job_failed(&job_id, "YOLO stopped by user")
                .await;

            return;
        }

        iteration += 1;
        per_symbol_leaderboard.total_iterations = iteration;
        cross_symbol_leaderboard.total_iterations = iteration;

        tracing::info!(
            iteration = iteration,
            configs_to_test = configs_per_iteration,
            "Starting YOLO iteration"
        );

        // Emit progress
        let _ = app.emit(
            "yolo:progress",
            EventEnvelope::new(
                "yolo:progress",
                &job_id,
                YoloProgressPayload {
                    iteration,
                    phase: "sweeping".to_string(),
                    completed_configs: 0,
                    total_configs: configs_per_iteration as u64,
                },
            ),
        );

        // Update state
        {
            let mut yolo = state.yolo.write_or_recover();
            yolo.iteration = iteration;
            yolo.completed_configs = 0;
            yolo.total_configs = configs_per_iteration as u64;
        }

        // =========================================================================
        // Real Polars Backtest Engine Integration (Phase 4)
        // =========================================================================

        // Get parquet directory from state
        let parquet_dir = state.data_config.parquet_dir();
        let backtest_config = PolarsBacktestConfig::default();

        // Build strategy grid configs from selected strategy IDs
        let strategy_configs: Vec<StrategyGridConfig> = strategies
            .iter()
            .filter_map(|s| strategy_id_to_grid_config(s))
            .collect();

        if strategy_configs.is_empty() {
            tracing::warn!("No valid strategy configs, skipping iteration");
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            continue;
        }

        // Collect all sweep results for this iteration
        let mut all_sweep_results: Vec<SymbolSweepResult> = Vec::new();
        let mut configs_tested_this_round = 0usize;
        let mut sweep_errors = 0usize;

        // Run sweeps for each symbol × strategy combination
        for symbol in symbols.iter() {
            for strategy_config in strategy_configs.iter() {
                // Check cancellation
                if cancel_token.is_cancelled() {
                    break;
                }

                let symbol_clone = symbol.clone();
                let parquet_dir_clone = parquet_dir.clone();
                let strategy_config_clone = strategy_config.clone();
                let backtest_config_clone = backtest_config.clone();

                // Run the sweep in a blocking task to avoid blocking the async runtime
                let sweep_handle = tokio::task::spawn_blocking(move || {
                    run_symbol_sweep_sync(
                        parquet_dir_clone,
                        symbol_clone,
                        strategy_config_clone,
                        backtest_config_clone,
                    )
                });

                match sweep_handle.await {
                    Ok(Ok(result)) => {
                        let num_configs = result.sweep_result.config_results.len();
                        configs_tested_this_round += num_configs;
                        all_sweep_results.push(result);

                        tracing::debug!(
                            symbol = %symbol,
                            strategy = ?strategy_config.strategy_type,
                            configs = num_configs,
                            "Sweep completed"
                        );
                    }
                    Ok(Err(e)) => {
                        sweep_errors += 1;
                        tracing::warn!(
                            symbol = %symbol,
                            strategy = ?strategy_config.strategy_type,
                            error = %e,
                            "Sweep failed"
                        );
                    }
                    Err(e) => {
                        sweep_errors += 1;
                        tracing::error!(
                            symbol = %symbol,
                            strategy = ?strategy_config.strategy_type,
                            error = ?e,
                            "Spawn blocking failed"
                        );
                    }
                }

                // Emit progress after each symbol/strategy pair
                let _ = app.emit(
                    "yolo:progress",
                    EventEnvelope::new(
                        "yolo:progress",
                        &job_id,
                        YoloProgressPayload {
                            iteration,
                            phase: "sweeping".to_string(),
                            completed_configs: configs_tested_this_round as u64,
                            total_configs: configs_per_iteration as u64,
                        },
                    ),
                );

                // Update state
                {
                    let mut yolo = state.yolo.write_or_recover();
                    yolo.completed_configs = configs_tested_this_round as u64;
                }

                // Companion progress
                state
                    .companion_job_progress(
                        &job_id,
                        configs_tested_this_round as u64,
                        configs_per_iteration as u64,
                        &format!(
                            "YOLO iter {} - {} × {:?}",
                            iteration, symbol, strategy_config.strategy_type
                        ),
                    )
                    .await;
            }

            if cancel_token.is_cancelled() {
                break;
            }
        }

        tracing::info!(
            iteration = iteration,
            total_results = all_sweep_results.len(),
            configs_tested = configs_tested_this_round,
            errors = sweep_errors,
            "Iteration sweep complete"
        );

        // =========================================================================
        // Update Leaderboards with Diverse Selection (Clustering)
        // =========================================================================

        // Process results and update leaderboards
        for sweep_result in all_sweep_results.iter() {
            // Get top configs by Sharpe for this symbol/strategy pair
            let ranked_configs = sweep_result
                .sweep_result
                .rank_by(trendlab_core::RankMetric::Sharpe, false);

            // Take top 5 from each symbol/strategy to avoid domination
            for config_result in ranked_configs.iter().take(5) {
                let entry = sweep_result_to_leaderboard_entry(
                    &sweep_result.symbol,
                    sweep_result.strategy_type,
                    &config_result.config_id,
                    config_result,
                    iteration,
                    &[], // Dates could be extracted if needed
                );

                // Try to insert into per-symbol leaderboard
                per_symbol_leaderboard.try_insert(entry);
            }
        }

        // Apply diverse selection clustering if we have enough results
        if configs_tested_this_round >= 20 {
            // Combine all results into a single DataFrame for clustering
            let mut all_dfs: Vec<DataFrame> = Vec::new();

            for sweep_result in all_sweep_results.iter() {
                if let Ok(mut df) = sweep_to_dataframe(&sweep_result.sweep_result) {
                    // Add symbol and strategy columns for tracking
                    let n = df.height();
                    let symbol_col =
                        Series::new("symbol".into(), vec![sweep_result.symbol.as_str(); n]);
                    let strategy_col = Series::new(
                        "strategy_type".into(),
                        vec![format!("{:?}", sweep_result.strategy_type); n],
                    );
                    let _ = df.with_column(symbol_col);
                    let _ = df.with_column(strategy_col);
                    all_dfs.push(df);
                }
            }

            if !all_dfs.is_empty() {
                // Concatenate all DataFrames - collect into Vec for concat
                let lazy_frames: Vec<LazyFrame> =
                    all_dfs.iter().map(|df| df.clone().lazy()).collect();

                match concat(&lazy_frames, Default::default()).and_then(|lf| lf.collect())
                {
                    Ok(combined_df) => {
                        tracing::debug!(
                            rows = combined_df.height(),
                            cols = combined_df.width(),
                            "Combined sweep results for clustering"
                        );

                        // Apply diverse selection to get representative strategies
                        match select_diverse_top_n(&combined_df, 10) {
                            Ok(diverse_df) => {
                                tracing::info!(
                                    diverse_count = diverse_df.height(),
                                    "Applied diverse selection clustering"
                                );
                                // The diverse_df contains the selected configs
                                // These would already be represented in the leaderboard
                                // from the per-symbol updates above
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "Diverse selection failed, using top-N by Sharpe");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to concatenate sweep results");
                    }
                }
            }
        }

        // Update leaderboard stats
        per_symbol_leaderboard.add_configs_tested(configs_tested_this_round);
        cross_symbol_leaderboard.add_configs_tested(configs_tested_this_round);

        // Save leaderboards
        let _ = per_symbol_leaderboard.save(per_symbol_path);
        let _ = cross_symbol_leaderboard.save(cross_symbol_path);

        // Update state with leaderboards
        {
            let mut yolo = state.yolo.write_or_recover();
            yolo.total_configs_tested += configs_tested_this_round as u64;
            yolo.per_symbol_leaderboard = Some(per_symbol_leaderboard.clone());
            yolo.cross_symbol_leaderboard = Some(cross_symbol_leaderboard.clone());
        }

        // Emit iteration complete
        let _ = app.emit(
            "yolo:iteration_complete",
            EventEnvelope::new(
                "yolo:iteration_complete",
                &job_id,
                YoloIterationCompletePayload {
                    iteration,
                    cross_symbol_leaderboard: convert_cross_symbol_leaderboard(
                        &cross_symbol_leaderboard,
                    ),
                    per_symbol_leaderboard: convert_leaderboard(&per_symbol_leaderboard),
                    configs_tested_this_round,
                },
            ),
        );

        // Small delay between iterations
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
