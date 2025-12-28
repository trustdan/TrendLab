//! Companion state management.

use std::collections::VecDeque;

use chrono::{DateTime, Local};

use crate::ipc::{CompanionEvent, JobType, LogLevel};

/// Maximum number of recent results to keep.
const MAX_RESULTS: usize = 20;
/// Maximum number of log entries to keep.
const MAX_LOGS: usize = 100;

/// A sweep result entry for display.
#[derive(Debug, Clone)]
pub struct SweepResultEntry {
    pub ticker: String,
    pub strategy: String,
    pub config_id: String,
    pub sharpe: f64,
    pub cagr: f64,
    pub max_dd: f64,
    pub ts: DateTime<Local>,
}

/// A log entry for display.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub ts: DateTime<Local>,
}

/// State for the companion terminal display.
#[derive(Debug)]
pub struct CompanionState {
    // GUI info
    gui_pid: u32,
    gui_version: Option<String>,
    connected: bool,

    // Current job
    current_job_id: Option<String>,
    current_job_type: Option<JobType>,
    current_job_desc: Option<String>,
    job_progress: u64,
    job_total: u64,
    job_message: String,

    // Results (ring buffer, newest first)
    recent_results: VecDeque<SweepResultEntry>,

    // Logs (ring buffer, newest first)
    logs: VecDeque<LogEntry>,

    // Status message
    status: String,

    // View mode
    minimized: bool,
}

impl CompanionState {
    /// Create a new companion state.
    pub fn new(gui_pid: u32) -> Self {
        Self {
            gui_pid,
            gui_version: None,
            connected: false,
            current_job_id: None,
            current_job_type: None,
            current_job_desc: None,
            job_progress: 0,
            job_total: 0,
            job_message: String::new(),
            recent_results: VecDeque::with_capacity(MAX_RESULTS),
            logs: VecDeque::with_capacity(MAX_LOGS),
            status: "Waiting for GUI connection...".to_string(),
            minimized: false,
        }
    }

    /// Get the GUI process ID.
    pub fn gui_pid(&self) -> u32 {
        self.gui_pid
    }

    /// Check if connected to GUI.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Get the GUI version.
    pub fn gui_version(&self) -> Option<&str> {
        self.gui_version.as_deref()
    }

    /// Get the current status message.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Check if in minimized mode.
    pub fn is_minimized(&self) -> bool {
        self.minimized
    }

    /// Toggle minimized mode.
    pub fn toggle_minimized(&mut self) {
        self.minimized = !self.minimized;
    }

    /// Check if a job is currently running.
    pub fn has_active_job(&self) -> bool {
        self.current_job_id.is_some()
    }

    /// Get the current job type.
    pub fn current_job_type(&self) -> Option<JobType> {
        self.current_job_type
    }

    /// Get the current job description.
    pub fn current_job_desc(&self) -> Option<&str> {
        self.current_job_desc.as_deref()
    }

    /// Get the current job message.
    pub fn job_message(&self) -> &str {
        &self.job_message
    }

    /// Get job progress as (current, total).
    pub fn job_progress(&self) -> (u64, u64) {
        (self.job_progress, self.job_total)
    }

    /// Get job progress as a percentage (0.0 to 1.0).
    pub fn progress_percent(&self) -> f64 {
        if self.job_total == 0 {
            0.0
        } else {
            self.job_progress as f64 / self.job_total as f64
        }
    }

    /// Get recent results.
    pub fn recent_results(&self) -> &VecDeque<SweepResultEntry> {
        &self.recent_results
    }

    /// Get log entries.
    pub fn logs(&self) -> &VecDeque<LogEntry> {
        &self.logs
    }

    /// Mark as disconnected.
    pub fn set_disconnected(&mut self) {
        self.connected = false;
        self.status = "GUI disconnected".to_string();
        self.add_log(LogLevel::Warn, "GUI process terminated");
    }

    /// Apply a companion event to update state.
    pub fn apply_event(&mut self, event: CompanionEvent) {
        match event {
            CompanionEvent::Started { pid: _, version } => {
                self.gui_version = Some(version.clone());
                self.connected = true;
                self.status = format!("GUI connected (v{})", version);
                self.add_log(LogLevel::Info, &format!("GUI connected (v{})", version));
            }

            CompanionEvent::Shutdown => {
                self.connected = false;
                self.status = "GUI shutting down".to_string();
                self.add_log(LogLevel::Info, "GUI shutdown");
            }

            CompanionEvent::JobStarted {
                job_id,
                job_type,
                description,
            } => {
                self.current_job_id = Some(job_id);
                self.current_job_type = Some(job_type);
                self.current_job_desc = Some(description.clone());
                self.job_progress = 0;
                self.job_total = 0;
                self.job_message.clear();
                self.add_log(LogLevel::Info, &format!("{} started: {}", job_type, description));
            }

            CompanionEvent::JobProgress {
                job_id,
                current,
                total,
                message,
            } => {
                if self.current_job_id.as_deref() == Some(&job_id) {
                    self.job_progress = current;
                    self.job_total = total;
                    self.job_message = message;
                }
            }

            CompanionEvent::JobComplete {
                job_id,
                summary,
                elapsed_ms,
            } => {
                if self.current_job_id.as_deref() == Some(&job_id) {
                    let elapsed_secs = elapsed_ms as f64 / 1000.0;
                    self.add_log(
                        LogLevel::Info,
                        &format!("Completed in {:.1}s: {}", elapsed_secs, summary),
                    );
                    self.current_job_id = None;
                    self.current_job_type = None;
                    self.current_job_desc = None;
                    self.status = summary;
                }
            }

            CompanionEvent::JobFailed { job_id, error } => {
                if self.current_job_id.as_deref() == Some(&job_id) {
                    self.add_log(LogLevel::Error, &format!("Job failed: {}", error));
                    self.current_job_id = None;
                    self.current_job_type = None;
                    self.status = format!("Failed: {}", error);
                }
            }

            CompanionEvent::JobCancelled { job_id } => {
                if self.current_job_id.as_deref() == Some(&job_id) {
                    self.add_log(LogLevel::Warn, "Job cancelled");
                    self.current_job_id = None;
                    self.current_job_type = None;
                    self.status = "Cancelled".to_string();
                }
            }

            CompanionEvent::SweepResult {
                ticker,
                strategy,
                config_id,
                sharpe,
                cagr,
                max_dd,
            } => {
                if self.recent_results.len() >= MAX_RESULTS {
                    self.recent_results.pop_back();
                }
                self.recent_results.push_front(SweepResultEntry {
                    ticker,
                    strategy,
                    config_id,
                    sharpe,
                    cagr,
                    max_dd,
                    ts: Local::now(),
                });
            }

            CompanionEvent::Log { level, message, ts: _ } => {
                self.add_log(level, &message);
            }

            CompanionEvent::LogDetailed {
                level,
                target,
                message,
                spans,
                fields: _,
                ts: _,
            } => {
                // Format the message with span context if present
                let formatted = if spans.is_empty() {
                    format!("[{}] {}", target, message)
                } else {
                    format!("[{} > {}] {}", target, spans.join(" > "), message)
                };
                self.add_log(level, &formatted);
            }

            CompanionEvent::Status { message } => {
                self.status = message;
            }
        }
    }

    /// Add a log entry.
    fn add_log(&mut self, level: LogLevel, message: &str) {
        if self.logs.len() >= MAX_LOGS {
            self.logs.pop_back();
        }
        self.logs.push_front(LogEntry {
            level,
            message: message.to_string(),
            ts: Local::now(),
        });
    }
}
