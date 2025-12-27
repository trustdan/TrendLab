use crate::jobs::Jobs;

#[derive(Debug)]
pub struct AppState {
    pub jobs: Jobs,
}

impl AppState {
    pub fn new() -> Self {
        Self { jobs: Jobs::new() }
    }
}


