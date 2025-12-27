pub mod commands;
pub mod error;
pub mod events;
pub mod jobs;
pub mod state;

use state::AppState;

pub fn run() {
    let app_state = AppState::new();
    // Initialize cached symbols from parquet directory
    app_state.init_cached_symbols();

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::system::ping_job,
            commands::jobs::cancel_job,
            commands::data::get_universe,
            commands::data::get_cached_symbols,
            commands::data::update_selection,
            commands::data::get_selection,
            commands::data::search_symbols,
            commands::data::fetch_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
