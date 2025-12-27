pub mod commands;
pub mod error;
pub mod events;
pub mod jobs;
pub mod state;

use state::AppState;
use tauri::Manager;

pub fn run() {
    let app_state = AppState::new();
    // Initialize cached symbols from parquet directory
    app_state.init_cached_symbols();

    tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            // Initialize companion client in async context
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app_handle.state::<AppState>();
                state.init_companion().await;
            });
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
