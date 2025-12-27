use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::Emitter;

use crate::{
    error::GuiError,
    events::{EventEnvelope, JobCompletePayload, JobProgressPayload},
    jobs::JobStatus,
    state::AppState,
};

#[derive(Debug, serde::Serialize)]
pub struct StartJobResponse {
    pub job_id: String,
}

/// Smoke-test command: starts a background job and emits progress + completion events.
#[tauri::command]
pub async fn ping_job(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<StartJobResponse, GuiError> {
    let job_id = format!("ping-{}", now_ms());
    let token = state.jobs.create(job_id.clone());

    let app_for_task = app.clone();
    let jobs_for_task = state.jobs.clone();
    let job_id_for_task = job_id.clone();

    tokio::spawn(async move {
        jobs_for_task.set_status(&job_id_for_task, JobStatus::Running);

        let total = 5u64;
        for i in 1..=total {
            if token.is_cancelled() {
                jobs_for_task.set_status(&job_id_for_task, JobStatus::Cancelled);
                let _ = app_for_task.emit(
                    "job:cancelled",
                    EventEnvelope {
                        event: "job:cancelled",
                        job_id: job_id_for_task.clone(),
                        ts_ms: now_ms(),
                        payload: JobCompletePayload {
                            message: "Cancelled".to_string(),
                        },
                    },
                );
                return;
            }

            let _ = app_for_task.emit(
                "job:progress",
                EventEnvelope {
                    event: "job:progress",
                    job_id: job_id_for_task.clone(),
                    ts_ms: now_ms(),
                    payload: JobProgressPayload {
                        message: format!("ping {i}/{total}"),
                        current: i,
                        total,
                    },
                },
            );

            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        jobs_for_task.set_status(&job_id_for_task, JobStatus::Completed);
        let _ = app_for_task.emit(
            "job:complete",
            EventEnvelope {
                event: "job:complete",
                job_id: job_id_for_task.clone(),
                ts_ms: now_ms(),
                payload: JobCompletePayload {
                    message: "Done".to_string(),
                },
            },
        );
    });

    Ok(StartJobResponse { job_id })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as u64
}
