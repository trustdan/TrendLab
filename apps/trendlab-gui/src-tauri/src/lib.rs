pub mod commands;
pub mod error;
pub mod events;
pub mod jobs;
pub mod state;

use state::AppState;
use tauri::Manager;
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

    let app_state = AppState::new();
    // Initialize cached symbols from parquet directory
    app_state.init_cached_symbols();

    tauri::Builder::default()
        .manage(app_state)
        .setup(move |app| {
            let app_handle = app.handle().clone();

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
