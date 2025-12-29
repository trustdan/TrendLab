//! Sweep panel commands - parameter sweeps and cost model configuration

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio;
use trendlab_core::{
    run_strategy_sweep_polars_parallel, scan_symbol_parquet_lazy, CostModel as CoreCostModel,
    PolarsBacktestConfig, StrategyGridConfig, SweepDepth as CoreSweepDepth,
};
use trendlab_launcher::ipc::JobType;

use crate::error::GuiError;
use crate::events::EventEnvelope;
use crate::jobs::JobStatus;
use crate::state::AppState;

// ============================================================================
// Types
// ============================================================================

/// Sweep depth level (controls parameter grid density)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SweepDepth {
    /// Quick sweep: minimal grid for fast validation
    Quick,
    /// Normal sweep: balanced coverage
    #[default]
    Normal,
    /// Deep sweep: comprehensive coverage
    Deep,
    /// Exhaustive sweep: maximum coverage (slow)
    Exhaustive,
}

impl SweepDepth {
    /// Get display name for the depth level
    pub fn name(&self) -> &'static str {
        match self {
            SweepDepth::Quick => "Quick",
            SweepDepth::Normal => "Normal",
            SweepDepth::Deep => "Deep",
            SweepDepth::Exhaustive => "Exhaustive",
        }
    }

    /// Get description for the depth level
    pub fn description(&self) -> &'static str {
        match self {
            SweepDepth::Quick => "3-5 values per param, ~50 configs",
            SweepDepth::Normal => "5-8 values per param, ~200 configs",
            SweepDepth::Deep => "10-15 values per param, ~500 configs",
            SweepDepth::Exhaustive => "20+ values per param, ~2000+ configs",
        }
    }

    /// Get estimated config count multiplier
    pub fn config_multiplier(&self) -> usize {
        match self {
            SweepDepth::Quick => 25,
            SweepDepth::Normal => 100,
            SweepDepth::Deep => 400,
            SweepDepth::Exhaustive => 1600,
        }
    }

    /// Convert to core sweep depth
    fn as_core(self) -> CoreSweepDepth {
        match self {
            SweepDepth::Quick => CoreSweepDepth::Quick,
            SweepDepth::Normal => CoreSweepDepth::Standard,
            SweepDepth::Deep => CoreSweepDepth::Comprehensive,
            SweepDepth::Exhaustive => CoreSweepDepth::Comprehensive,
        }
    }
}

/// Convert a strategy name to a StrategyGridConfig
fn strategy_name_to_config(name: &str, depth: CoreSweepDepth) -> Option<StrategyGridConfig> {
    match name.to_lowercase().as_str() {
        "donchian" | "donchian breakout" => Some(StrategyGridConfig::donchian_with_depth(depth)),
        "turtle_s1" | "turtle s1" => Some(StrategyGridConfig::turtle_s1()),
        "turtle_s2" | "turtle s2" => Some(StrategyGridConfig::turtle_s2()),
        "ma_crossover" | "ma crossover" | "moving average crossover" => {
            Some(StrategyGridConfig::ma_crossover_with_depth(depth))
        }
        "tsmom" | "time series momentum" => Some(StrategyGridConfig::tsmom_with_depth(depth)),
        _ => None,
    }
}

/// Cost model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostModel {
    /// Trading fees in basis points (1 bp = 0.01%)
    pub fees_bps: f64,
    /// Slippage in basis points
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

/// Sweep configuration (full sweep job config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepConfig {
    pub symbols: Vec<String>,
    pub strategies: Vec<String>,
    pub depth: SweepDepth,
    pub cost_model: CostModel,
    pub date_range: DateRange,
}

/// Date range for sweep
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: String, // "YYYY-MM-DD"
    pub end: String,
}

impl Default for DateRange {
    fn default() -> Self {
        // Default to 10 years of history
        let end = chrono::Local::now().format("%Y-%m-%d").to_string();
        let start = (chrono::Local::now() - chrono::Duration::days(365 * 10))
            .format("%Y-%m-%d")
            .to_string();
        Self { start, end }
    }
}

/// Sweep state stored in AppState
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SweepState {
    pub depth: SweepDepth,
    pub cost_model: CostModel,
    pub date_range: DateRange,
    pub is_running: bool,
    pub current_job_id: Option<String>,
}

/// Selection summary for display
#[derive(Debug, Clone, Serialize)]
pub struct SelectionSummary {
    pub symbols: Vec<String>,
    pub strategies: Vec<String>,
    pub symbol_count: usize,
    pub strategy_count: usize,
    pub estimated_configs: usize,
    pub has_cached_data: bool,
}

/// Depth option for selector
#[derive(Debug, Clone, Serialize)]
pub struct DepthOption {
    pub id: String,
    pub name: String,
    pub description: String,
    pub estimated_configs: usize,
}

/// Start sweep response
#[derive(Debug, Clone, Serialize)]
pub struct StartSweepResponse {
    pub job_id: String,
    pub total_configs: usize,
}

/// Sweep progress event payload
#[derive(Debug, Clone, Serialize)]
pub struct SweepProgressPayload {
    pub job_id: String,
    pub current: usize,
    pub total: usize,
    pub symbol: String,
    pub strategy: String,
    pub config_id: String,
    pub message: String,
}

/// Sweep complete event payload
#[derive(Debug, Clone, Serialize)]
pub struct SweepCompletePayload {
    pub job_id: String,
    pub total_configs: usize,
    pub successful: usize,
    pub failed: usize,
    pub elapsed_ms: u64,
}

// ============================================================================
// Commands
// ============================================================================

/// Get current selection summary for the sweep panel
#[tauri::command]
pub fn get_selection_summary(state: State<'_, AppState>) -> SelectionSummary {
    let symbols = state.get_selected_tickers();
    let strategies = state.get_selected_strategies();
    let sweep_state = state.sweep.read().unwrap();

    let symbol_count = symbols.len();
    let strategy_count = strategies.len();

    // Estimate configs based on depth and selection
    let base_configs = symbol_count * strategy_count * sweep_state.depth.config_multiplier();

    // Check if selected symbols have cached data
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

/// Get available depth options
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

/// Get current sweep depth
#[tauri::command]
pub fn get_sweep_depth(state: State<'_, AppState>) -> SweepDepth {
    let sweep_state = state.sweep.read().unwrap();
    sweep_state.depth
}

/// Set sweep depth
#[tauri::command]
pub fn set_sweep_depth(state: State<'_, AppState>, depth: SweepDepth) {
    let mut sweep_state = state.sweep.write().unwrap();
    sweep_state.depth = depth;
}

/// Get cost model
#[tauri::command]
pub fn get_cost_model(state: State<'_, AppState>) -> CostModel {
    let sweep_state = state.sweep.read().unwrap();
    sweep_state.cost_model.clone()
}

/// Set cost model
#[tauri::command]
pub fn set_cost_model(state: State<'_, AppState>, cost_model: CostModel) {
    let mut sweep_state = state.sweep.write().unwrap();
    sweep_state.cost_model = cost_model;
}

/// Get date range
#[tauri::command]
pub fn get_date_range(state: State<'_, AppState>) -> DateRange {
    let sweep_state = state.sweep.read().unwrap();
    sweep_state.date_range.clone()
}

/// Set date range
#[tauri::command]
pub fn set_date_range(state: State<'_, AppState>, date_range: DateRange) {
    let mut sweep_state = state.sweep.write().unwrap();
    sweep_state.date_range = date_range;
}

/// Get sweep state (is running, job id)
#[tauri::command]
pub fn get_sweep_state(state: State<'_, AppState>) -> SweepState {
    let sweep_state = state.sweep.read().unwrap();
    sweep_state.clone()
}

/// Start a parameter sweep
#[tauri::command]
pub async fn start_sweep(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StartSweepResponse, GuiError> {
    // Check if already running
    {
        let sweep_state = state.sweep.read().unwrap();
        if sweep_state.is_running {
            return Err(GuiError::InvalidState(
                "A sweep is already running".to_string(),
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

    // Get sweep config
    let (depth, _cost_model, _date_range) = {
        let sweep_state = state.sweep.read().unwrap();
        (
            sweep_state.depth,
            sweep_state.cost_model.clone(),
            sweep_state.date_range.clone(),
        )
    };

    // Generate job ID
    let job_id = format!("sweep-{}", chrono::Utc::now().timestamp_millis());

    // Calculate total configs (simplified estimate)
    let total_configs = symbols.len() * strategies.len() * depth.config_multiplier();

    // Create job and get cancellation token
    let token = state.jobs.create(job_id.clone());

    // Mark sweep as running
    {
        let mut sweep_state = state.sweep.write().unwrap();
        sweep_state.is_running = true;
        sweep_state.current_job_id = Some(job_id.clone());
    }

    state.jobs.set_status(&job_id, JobStatus::Running);

    // Emit started event
    let _ = app.emit(
        "sweep:started",
        EventEnvelope::new(
            "sweep:started",
            &job_id,
            serde_json::json!({
                "job_id": job_id,
                "total_configs": total_configs,
                "symbols": symbols,
                "strategies": strategies,
            }),
        ),
    );

    // Emit to companion terminal (if connected)
    state
        .companion_job_started(
            &job_id,
            JobType::Sweep,
            &format!(
                "Sweep: {} symbols × {} strategies",
                symbols.len(),
                strategies.len()
            ),
        )
        .await;

    // Clone what we need for the async task
    let job_id_clone = job_id.clone();
    let app_clone = app.clone();
    let symbols_clone = symbols.clone();
    let strategies_clone = strategies.clone();

    // Clone cost model for the task
    let cost_model_clone = {
        let sweep_state = state.sweep.read().unwrap();
        sweep_state.cost_model.clone()
    };
    let core_depth = depth.as_core();

    // Spawn background task
    tokio::spawn(async move {
        let state_handle = app_clone.state::<AppState>();
        let start_time = std::time::Instant::now();
        let mut completed = 0usize;
        let mut failed = 0usize;
        let mut actual_total = 0usize;

        // Configure Polars backtest
        let polars_config = PolarsBacktestConfig::new(100_000.0, 100.0).with_cost_model(
            CoreCostModel {
                fees_bps_per_side: cost_model_clone.fees_bps,
                slippage_bps: cost_model_clone.slippage_bps,
            },
        );

        let parquet_dir = Path::new("data/parquet");

        // Run actual backtests
        for symbol in &symbols_clone {
            // Check for cancellation
            if token.is_cancelled() {
                let _ = state_handle.sweep.write().map(|mut s| {
                    s.is_running = false;
                    s.current_job_id = None;
                });
                state_handle
                    .jobs
                    .set_status(&job_id_clone, JobStatus::Cancelled);

                let _ = app_clone.emit(
                    "sweep:cancelled",
                    EventEnvelope::new(
                        "sweep:cancelled",
                        &job_id_clone,
                        serde_json::json!({
                            "job_id": job_id_clone,
                            "completed": completed,
                        }),
                    ),
                );

                state_handle
                    .companion_job_failed(&job_id_clone, "Cancelled by user")
                    .await;

                return;
            }

            // Load data for this symbol from Parquet
            let df = match scan_symbol_parquet_lazy(parquet_dir, symbol, "1d", None, None) {
                Ok(lf) => match lf.collect() {
                    Ok(df) => df,
                    Err(e) => {
                        tracing::warn!("Failed to collect DataFrame for {}: {}", symbol, e);
                        failed += 1;
                        continue;
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to scan Parquet for {}: {}", symbol, e);
                    failed += 1;
                    continue;
                }
            };

            for strategy in &strategies_clone {
                // Check for cancellation
                if token.is_cancelled() {
                    break;
                }

                // Convert strategy name to config
                let strategy_config = match strategy_name_to_config(strategy, core_depth) {
                    Some(config) => config,
                    None => {
                        tracing::warn!("Unknown strategy: {}", strategy);
                        failed += 1;
                        continue;
                    }
                };

                // Calculate expected configs for this strategy
                let expected_configs = strategy_config.config_count();
                actual_total += expected_configs;

                // Emit progress before running sweep
                let _ = app_clone.emit(
                    "sweep:progress",
                    EventEnvelope::new(
                        "sweep:progress",
                        &job_id_clone,
                        SweepProgressPayload {
                            job_id: job_id_clone.clone(),
                            current: completed,
                            total: total_configs,
                            symbol: symbol.clone(),
                            strategy: strategy.clone(),
                            config_id: format!("{}-{}", symbol, strategy),
                            message: format!(
                                "{} × {} ({} configs)",
                                symbol, strategy, expected_configs
                            ),
                        },
                    ),
                );

                state_handle
                    .companion_job_progress(
                        &job_id_clone,
                        completed as u64,
                        total_configs as u64,
                        &format!("{} × {}", symbol, strategy),
                    )
                    .await;

                // Run the actual backtest sweep (blocking in spawn_blocking to avoid blocking async runtime)
                let df_clone = df.clone();
                let config_clone = strategy_config.clone();
                let polars_config_clone = polars_config.clone();

                let sweep_result = tokio::task::spawn_blocking(move || {
                    run_strategy_sweep_polars_parallel(&df_clone, &config_clone, &polars_config_clone)
                })
                .await;

                match sweep_result {
                    Ok(Ok(result)) => {
                        completed += result.config_results.len();
                        tracing::info!(
                            "Sweep {} × {} completed: {} configs",
                            symbol,
                            strategy,
                            result.config_results.len()
                        );
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Sweep failed for {} × {}: {}", symbol, strategy, e);
                        failed += expected_configs;
                    }
                    Err(e) => {
                        tracing::error!("Sweep task panicked for {} × {}: {}", symbol, strategy, e);
                        failed += expected_configs;
                    }
                }
            }
        }

        // Mark complete
        {
            let mut sweep_state = state_handle.sweep.write().unwrap();
            sweep_state.is_running = false;
            sweep_state.current_job_id = None;
        }
        state_handle
            .jobs
            .set_status(&job_id_clone, JobStatus::Completed);

        let elapsed = start_time.elapsed();
        let _ = app_clone.emit(
            "sweep:complete",
            EventEnvelope::new(
                "sweep:complete",
                &job_id_clone,
                SweepCompletePayload {
                    job_id: job_id_clone.clone(),
                    total_configs: actual_total,
                    successful: completed,
                    failed,
                    elapsed_ms: elapsed.as_millis() as u64,
                },
            ),
        );

        state_handle
            .companion_job_complete(
                &job_id_clone,
                &format!(
                    "Completed {} configs, {} successful, {} failed",
                    actual_total, completed, failed
                ),
                elapsed.as_millis() as u64,
            )
            .await;
    });

    Ok(StartSweepResponse {
        job_id,
        total_configs,
    })
}

/// Cancel a running sweep (delegates to job cancellation)
#[tauri::command]
pub fn cancel_sweep(state: State<'_, AppState>) -> Result<(), GuiError> {
    let job_id = {
        let sweep_state = state.sweep.read().unwrap();
        sweep_state.current_job_id.clone()
    };

    if let Some(job_id) = job_id {
        state.jobs.cancel(&job_id);
        Ok(())
    } else {
        Err(GuiError::InvalidState("No sweep is running".to_string()))
    }
}
