use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EventEnvelope<T: Serialize> {
    pub event: &'static str,
    pub job_id: String,
    pub ts_ms: u64,
    pub payload: T,
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


