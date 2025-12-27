//! YOLO Mode commands - continuous auto-optimization with leaderboard
//!
//! YOLO mode runs parameter sweeps indefinitely, applying jitter each iteration,
//! and maintaining top-4 leaderboards (per-symbol and cross-symbol).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio;
use trendlab_core::leaderboard::{
    AggregatedMetrics, CrossSymbolLeaderboard, CrossSymbolRankMetric, Leaderboard,
};
use trendlab_launcher::ipc::JobType;

use crate::error::GuiError;
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
}

/// Per-symbol leaderboard for frontend
#[derive(Debug, Clone, Serialize)]
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

/// Cross-symbol leaderboard entry for frontend
#[derive(Debug, Clone, Serialize)]
pub struct AggregatedConfigResultResponse {
    pub rank: usize,
    pub strategy_type: String,
    pub config_id: String,
    pub symbols: Vec<String>,
    pub aggregate_metrics: AggregatedMetricsResponse,
    pub discovered_at: String,
    pub iteration: u32,
}

/// Cross-symbol leaderboard for frontend
#[derive(Debug, Clone, Serialize)]
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

/// Get current YOLO state
#[tauri::command]
pub fn get_yolo_state(state: State<'_, AppState>) -> YoloStateResponse {
    let yolo = state.yolo.read().unwrap();
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
    let yolo = state.yolo.read().unwrap();
    LeaderboardsResponse {
        per_symbol: yolo.per_symbol_leaderboard.as_ref().map(convert_leaderboard),
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
    // Check if already running
    {
        let yolo = state.yolo.read().unwrap();
        if yolo.enabled {
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
        let mut yolo = state.yolo.write().unwrap();
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
        let yolo = state.yolo.read().unwrap();
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

    // Load or create leaderboards
    let mut per_symbol_leaderboard = Leaderboard::load_or_new(per_symbol_path, 4);
    let mut cross_symbol_leaderboard =
        CrossSymbolLeaderboard::load_or_new(cross_symbol_path, 4, CrossSymbolRankMetric::AvgSharpe);

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
                let mut yolo = state.yolo.write().unwrap();
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
            let mut yolo = state.yolo.write().unwrap();
            yolo.iteration = iteration;
            yolo.completed_configs = 0;
            yolo.total_configs = configs_per_iteration as u64;
        }

        // Simulate processing (TODO: integrate real Polars backtest engine)
        // For now, we do a fast simulated sweep
        let mut configs_tested_this_round = 0usize;

        for (_si, symbol) in symbols.iter().enumerate() {
            for (_sti, strategy) in strategies.iter().enumerate() {
                // Check cancellation
                if cancel_token.is_cancelled() {
                    break;
                }

                // Simulate multiple configs per symbol/strategy pair
                let configs_per_pair = 25;
                for _config_idx in 0..configs_per_pair {
                    configs_tested_this_round += 1;

                    // Progress update every 50 configs
                    if configs_tested_this_round % 50 == 0 {
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
                            let mut yolo = state.yolo.write().unwrap();
                            yolo.completed_configs = configs_tested_this_round as u64;
                        }

                        // Companion progress (less frequent)
                        if configs_tested_this_round % 200 == 0 {
                            state
                                .companion_job_progress(
                                    &job_id,
                                    configs_tested_this_round as u64,
                                    configs_per_iteration as u64,
                                    &format!("YOLO iter {} - {} × {}", iteration, symbol, strategy),
                                )
                                .await;
                        }
                    }

                    // Tiny delay to simulate work
                    tokio::time::sleep(tokio::time::Duration::from_micros(50)).await;
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
            let mut yolo = state.yolo.write().unwrap();
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
