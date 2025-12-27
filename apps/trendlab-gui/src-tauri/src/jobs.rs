use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct JobId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug)]
pub struct JobHandle {
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl JobHandle {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn token(&self) -> CancellationToken {
        CancellationToken {
            cancelled: self.cancelled.clone(),
        }
    }

    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub struct Jobs {
    inner: Arc<Mutex<HashMap<String, (JobStatus, JobHandle)>>>,
}

impl Jobs {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create(&self, job_id: String) -> CancellationToken {
        let handle = JobHandle::new();
        let token = handle.token();
        let mut guard = self.inner.lock().expect("jobs mutex poisoned");
        guard.insert(job_id, (JobStatus::Queued, handle));
        token
    }

    pub fn set_status(&self, job_id: &str, status: JobStatus) {
        let mut guard = self.inner.lock().expect("jobs mutex poisoned");
        if let Some((s, _)) = guard.get_mut(job_id) {
            *s = status;
        }
    }

    pub fn cancel(&self, job_id: &str) -> bool {
        let guard = self.inner.lock().expect("jobs mutex poisoned");
        if let Some((_, handle)) = guard.get(job_id) {
            handle.cancel();
            true
        } else {
            false
        }
    }
}

impl Clone for Jobs {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}


