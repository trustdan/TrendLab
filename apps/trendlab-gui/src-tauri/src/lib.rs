pub mod commands;
pub mod error;
pub mod events;
pub mod jobs;
pub mod state;

use state::AppState;
use std::sync::mpsc::Receiver;
use tauri::Manager;
use trendlab_engine::worker::{spawn_worker, WorkerUpdate};
use trendlab_launcher::ipc::CompanionEvent;
use trendlab_logging::{LogConfig, LogEvent};

/// Forward log events from the tracing layer to the companion terminal via IPC.
async fn log_forwarder(
    mut rx: tokio::sync::mpsc::Receiver<LogEvent>,
    app_handle: tauri::AppHandle,
) {
    while let Some(log_event) = rx.recv().await {
        let state = app_handle.state::<AppState>();

        // Convert LogEvent to CompanionEvent::LogDetailed
        let companion_event = CompanionEvent::LogDetailed {
            level: log_event.level.into(),
            target: log_event.target,
            message: log_event.message,
            spans: log_event.spans,
            fields: log_event.fields,
            ts: log_event.timestamp,
        };

        state.emit_to_companion(companion_event).await;
    }
}

/// Forward worker updates to the React frontend.
/// This runs in an async task, polling the std::sync::mpsc receiver.
fn worker_update_bridge(
    update_rx: Receiver<WorkerUpdate>,
    app_handle: tauri::AppHandle,
) {
    // Run in a blocking task since std::sync::mpsc::Receiver is not async
    std::thread::spawn(move || {
        while let Ok(update) = update_rx.recv() {
            let state = app_handle.state::<AppState>();

            // Apply update to engine state (mirroring TUI's apply_update)
            {
                let mut engine = state.engine_write();
                apply_worker_update(&mut engine, &update);
            }

            // Emit event to React frontend
            emit_worker_update(&app_handle, &update);
        }
    });
}

/// Apply a worker update to the engine state (mirrors TUI's apply_update).
fn apply_worker_update(engine: &mut trendlab_engine::app::App, update: &WorkerUpdate) {
    use trendlab_engine::app::{OperationState, SearchSuggestion};

    match update {
        // Ready/Idle updates
        WorkerUpdate::Ready => {}
        WorkerUpdate::Idle => {}

        // Search results
        WorkerUpdate::SearchResults { query, results } => {
            // Only apply if query matches current search input
            if engine.data.search_input == *query {
                engine.data.search_loading = false;
                engine.data.search_suggestions = results
                    .iter()
                    .map(|r| SearchSuggestion {
                        symbol: r.symbol.clone(),
                        name: r.name.clone(),
                        exchange: r.exchange.clone(),
                        type_disp: r.type_disp.clone(),
                    })
                    .collect();
                engine.data.search_selected = 0;
            }
        }
        WorkerUpdate::SearchError { query, error } => {
            if engine.data.search_input == *query {
                engine.data.search_loading = false;
                tracing::warn!(query, error, "Symbol search failed");
            }
        }

        // Fetch progress
        WorkerUpdate::FetchStarted { symbol, index, total } => {
            engine.operation = OperationState::FetchingData {
                current_symbol: symbol.clone(),
                completed: *index,
                total: *total,
            };
        }
        WorkerUpdate::FetchComplete { symbol, bars, .. } => {
            // Store bars in cache
            engine.data.bars_cache.insert(symbol.clone(), bars.clone());
            // Update symbols list if not already present
            if !engine.data.symbols.contains(symbol) {
                engine.data.symbols.push(symbol.clone());
            }
        }
        WorkerUpdate::FetchError { symbol, error } => {
            tracing::error!(symbol, error, "Failed to fetch data");
        }
        WorkerUpdate::FetchAllComplete { .. } => {
            engine.operation = OperationState::Idle;
        }

        // Cache load
        WorkerUpdate::CacheLoadStarted { symbol, index, total } => {
            engine.operation = OperationState::FetchingData {
                current_symbol: symbol.clone(),
                completed: *index,
                total: *total,
            };
        }
        WorkerUpdate::CacheLoadComplete { symbol, bars } => {
            engine.data.bars_cache.insert(symbol.clone(), bars.clone());
            if !engine.data.symbols.contains(symbol) {
                engine.data.symbols.push(symbol.clone());
            }
        }
        WorkerUpdate::CacheLoadError { symbol, error } => {
            tracing::warn!(symbol, error, "Failed to load cached data");
        }
        WorkerUpdate::CacheLoadAllComplete { .. } => {
            engine.operation = OperationState::Idle;
        }

        // Sweep progress
        WorkerUpdate::SweepStarted { total_configs } => {
            engine.operation = OperationState::RunningSweep {
                completed: 0,
                total: *total_configs,
            };
        }
        WorkerUpdate::SweepProgress { completed, total } => {
            engine.operation = OperationState::RunningSweep {
                completed: *completed,
                total: *total,
            };
        }
        WorkerUpdate::SweepComplete { result } => {
            engine.operation = OperationState::Idle;
            engine.results.results = result.config_results.clone();
        }
        WorkerUpdate::SweepCancelled { .. } => {
            engine.operation = OperationState::Idle;
        }

        // Multi-sweep progress
        WorkerUpdate::MultiSweepStarted { total_symbols, configs_per_symbol } => {
            engine.operation = OperationState::RunningSweep {
                completed: 0,
                total: total_symbols * configs_per_symbol,
            };
        }
        WorkerUpdate::MultiSweepSymbolStarted { .. } => {}
        WorkerUpdate::MultiSweepSymbolComplete { .. } => {}
        WorkerUpdate::MultiSweepComplete { result } => {
            engine.operation = OperationState::Idle;
            // Store multi-sweep result
            engine.results.multi_sweep_result = Some(result.clone());
            engine.results.update_ticker_summaries();
            // Store first symbol's results for drill-down
            if let Some((_symbol, sweep_result)) = result.symbol_results.iter().next() {
                engine.results.results = sweep_result.config_results.clone();
            }
        }

        // Multi-strategy sweep
        WorkerUpdate::MultiStrategySweepStarted { total_symbols, total_strategies, total_configs } => {
            engine.operation = OperationState::RunningSweep {
                completed: 0,
                total: *total_configs,
            };
            tracing::info!(
                total_symbols, total_strategies, total_configs,
                "Multi-strategy sweep started"
            );
        }
        WorkerUpdate::MultiStrategySweepStrategyStarted { .. } => {}
        WorkerUpdate::MultiStrategySweepProgress { completed_configs, total_configs, .. } => {
            engine.operation = OperationState::RunningSweep {
                completed: *completed_configs,
                total: *total_configs,
            };
        }
        WorkerUpdate::MultiStrategySweepComplete { result } => {
            engine.operation = OperationState::Idle;
            // Store multi-strategy result
            engine.results.multi_strategy_result = Some(result.clone());
            // For results display, flatten first symbol/strategy results
            if let Some(((_symbol, _strategy), sweep_result)) = result.results.iter().next() {
                engine.results.results = sweep_result.config_results.clone();
            }
        }

        // Analysis
        WorkerUpdate::AnalysisComplete { analysis_id, analysis } => {
            engine.results.analysis_cache.insert(analysis_id.clone(), analysis.clone());
        }
        WorkerUpdate::AnalysisError { analysis_id, error } => {
            tracing::error!(analysis_id, error, "Analysis failed");
        }

        // YOLO mode
        WorkerUpdate::YoloIterationComplete { iteration, per_symbol_leaderboard, cross_symbol_leaderboard, .. } => {
            engine.yolo.iteration = *iteration;
            engine.yolo.session_leaderboard = per_symbol_leaderboard.clone();
            engine.yolo.session_cross_symbol_leaderboard = Some(cross_symbol_leaderboard.clone());
        }
        WorkerUpdate::YoloStopped { per_symbol_leaderboard, cross_symbol_leaderboard, total_iterations, .. } => {
            engine.yolo.enabled = false;
            engine.yolo.iteration = *total_iterations;
            engine.yolo.session_leaderboard = per_symbol_leaderboard.clone();
            engine.yolo.session_cross_symbol_leaderboard = Some(cross_symbol_leaderboard.clone());
        }
        WorkerUpdate::YoloModeStarted { .. } => {
            engine.yolo.enabled = true;
        }
        WorkerUpdate::YoloProgress { iteration, .. } => {
            engine.yolo.iteration = *iteration;
        }

        // Cancelled states - return to idle
        WorkerUpdate::MultiSweepCancelled { .. } => {
            engine.operation = OperationState::Idle;
        }
        WorkerUpdate::MultiStrategySweepCancelled { .. } => {
            engine.operation = OperationState::Idle;
        }

        // Analysis started - no state change needed
        WorkerUpdate::AnalysisStarted { .. } => {}
    }
}

/// Emit a worker update as a Tauri event for the React frontend.
fn emit_worker_update(app_handle: &tauri::AppHandle, update: &WorkerUpdate) {
    use tauri::Emitter;

    match update {
        WorkerUpdate::SearchResults { .. } => {
            let _ = app_handle.emit("worker:search-results", ());
        }
        WorkerUpdate::FetchStarted { symbol, index, total } => {
            let _ = app_handle.emit("worker:fetch-started", serde_json::json!({
                "symbol": symbol,
                "index": index,
                "total": total
            }));
        }
        WorkerUpdate::FetchComplete { symbol, .. } => {
            let _ = app_handle.emit("worker:fetch-complete", serde_json::json!({
                "symbol": symbol
            }));
        }
        WorkerUpdate::FetchAllComplete { symbols_fetched } => {
            let _ = app_handle.emit("worker:fetch-all-complete", serde_json::json!({
                "symbolsFetched": symbols_fetched
            }));
        }
        WorkerUpdate::SweepStarted { total_configs } => {
            let _ = app_handle.emit("worker:sweep-started", serde_json::json!({
                "totalConfigs": total_configs
            }));
        }
        WorkerUpdate::SweepProgress { completed, total } => {
            let _ = app_handle.emit("worker:sweep-progress", serde_json::json!({
                "completed": completed,
                "total": total
            }));
        }
        WorkerUpdate::SweepComplete { .. } => {
            let _ = app_handle.emit("worker:sweep-complete", ());
        }
        WorkerUpdate::SweepCancelled { completed } => {
            let _ = app_handle.emit("worker:sweep-cancelled", serde_json::json!({
                "completed": completed
            }));
        }
        WorkerUpdate::MultiSweepComplete { .. } => {
            let _ = app_handle.emit("worker:sweep-complete", ());
        }
        WorkerUpdate::MultiStrategySweepStarted { total_configs, .. } => {
            let _ = app_handle.emit("worker:sweep-started", serde_json::json!({
                "totalConfigs": total_configs
            }));
        }
        WorkerUpdate::MultiStrategySweepStrategyStarted { .. } => {}
        WorkerUpdate::MultiStrategySweepProgress { completed_configs, total_configs, .. } => {
            let _ = app_handle.emit("worker:sweep-progress", serde_json::json!({
                "completed": completed_configs,
                "total": total_configs
            }));
        }
        WorkerUpdate::MultiStrategySweepComplete { .. } => {
            let _ = app_handle.emit("worker:sweep-complete", ());
        }
        WorkerUpdate::YoloIterationComplete { iteration, .. } => {
            let _ = app_handle.emit("worker:yolo-iteration", serde_json::json!({
                "iteration": iteration
            }));
        }
        WorkerUpdate::YoloStopped { .. } => {
            let _ = app_handle.emit("worker:yolo-stopped", ());
        }
        // Other updates don't need frontend notification
        _ => {}
    }
}

pub fn run() {
    // Initialize logging from environment if enabled
    let log_config = LogConfig::from_env();

    // Create channel for IPC log forwarding (if logging enabled)
    let (log_tx, log_rx) = if log_config.enabled {
        let (tx, rx) = tokio::sync::mpsc::channel::<LogEvent>(1000);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    // Initialize logging (file + optional IPC)
    let _log_guard = if log_config.enabled {
        tracing::info!("GUI starting with logging enabled");
        trendlab_logging::init_gui_logging(&log_config, log_tx)
    } else {
        None
    };

    // Spawn background worker thread (same as TUI)
    let (channels, _worker_handle) = spawn_worker();

    // Create AppState with command sender and cancel flag
    // Note: update_rx is processed separately in an async task
    let app_state = AppState::new(channels.command_tx, channels.cancel_flag);
    // Initialize cached symbols from parquet directory
    app_state.init_cached_symbols();

    // Extract update_rx for the event bridge (will be moved into async task)
    let update_rx = channels.update_rx;

    tauri::Builder::default()
        .manage(app_state)
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Start worker update bridge (forwards updates to React)
            let app_handle_bridge = app_handle.clone();
            worker_update_bridge(update_rx, app_handle_bridge);

            // Initialize companion client in async context
            let app_handle_companion = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let state = app_handle_companion.state::<AppState>();
                state.init_companion().await;
            });

            // Start log forwarder if logging is enabled
            if let Some(rx) = log_rx {
                let app_handle_logs = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    log_forwarder(rx, app_handle_logs).await;
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Send shutdown to companion on window close
                let state = window.state::<AppState>();
                tauri::async_runtime::block_on(async move {
                    state.shutdown_companion().await;
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            // System
            commands::system::ping_job,
            commands::jobs::cancel_job,
            // Data
            commands::data::get_universe,
            commands::data::get_cached_symbols,
            commands::data::update_selection,
            commands::data::get_selection,
            commands::data::search_symbols,
            commands::data::fetch_data,
            // Strategy
            commands::strategy::get_strategy_categories,
            commands::strategy::get_strategy_defaults,
            commands::strategy::get_strategy_selection,
            commands::strategy::update_strategy_selection,
            commands::strategy::get_strategy_params,
            commands::strategy::update_strategy_params,
            commands::strategy::get_ensemble_config,
            commands::strategy::set_ensemble_enabled,
            // Sweep
            commands::sweep::get_selection_summary,
            commands::sweep::get_depth_options,
            commands::sweep::get_sweep_depth,
            commands::sweep::set_sweep_depth,
            commands::sweep::get_cost_model,
            commands::sweep::set_cost_model,
            commands::sweep::get_date_range,
            commands::sweep::set_date_range,
            commands::sweep::get_sweep_state,
            commands::sweep::start_sweep,
            commands::sweep::cancel_sweep,
            // Results
            commands::results::has_results,
            commands::results::get_results_count,
            commands::results::get_results,
            commands::results::get_ticker_summaries,
            commands::results::get_strategy_summaries,
            commands::results::get_result_detail,
            commands::results::select_result,
            commands::results::get_selected_result,
            commands::results::set_view_mode,
            commands::results::get_view_mode,
            commands::results::set_sort_config,
            commands::results::export_artifact,
            commands::results::clear_results,
            // Chart
            commands::chart::get_chart_state,
            commands::chart::set_chart_mode,
            commands::chart::set_chart_selection,
            commands::chart::toggle_overlay,
            commands::chart::get_overlays,
            commands::chart::get_candle_data,
            commands::chart::get_equity_curve,
            commands::chart::get_drawdown_curve,
            commands::chart::get_multi_ticker_curves,
            commands::chart::get_portfolio_curve,
            commands::chart::get_strategy_curves,
            commands::chart::get_trades,
            commands::chart::get_chart_data,
            // YOLO
            commands::yolo::get_yolo_state,
            commands::yolo::get_leaderboard,
            commands::yolo::start_yolo_mode,
            commands::yolo::stop_yolo_mode
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
