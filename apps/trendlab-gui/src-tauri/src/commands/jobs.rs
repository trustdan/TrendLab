use crate::{error::GuiError, state::AppState};

#[tauri::command]
pub async fn cancel_job(
    state: tauri::State<'_, AppState>,
    job_id: String,
) -> Result<bool, GuiError> {
    if job_id.trim().is_empty() {
        return Err(GuiError::InvalidInput {
            message: "job_id must be non-empty".to_string(),
        });
    }

    Ok(state.jobs.cancel(&job_id))
}
