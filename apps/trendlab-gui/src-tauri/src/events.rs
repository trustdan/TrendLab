use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EventEnvelope<T: Serialize> {
    pub event: &'static str,
    pub job_id: String,
    pub ts_ms: u64,
    pub payload: T,
}

impl<T: Serialize> EventEnvelope<T> {
    pub fn new(event: &'static str, job_id: &str, payload: T) -> Self {
        Self {
            event,
            job_id: job_id.to_string(),
            ts_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JobProgressPayload {
    pub message: String,
    pub current: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobCompletePayload {
    pub message: String,
}
