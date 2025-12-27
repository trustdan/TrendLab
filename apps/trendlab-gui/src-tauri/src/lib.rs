pub mod commands;
pub mod error;
pub mod events;
pub mod jobs;
pub mod state;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::system::ping_job,
            commands::jobs::cancel_job
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


