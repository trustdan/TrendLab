//! IPC message types for companion communication.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Events sent from GUI to Companion terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CompanionEvent {
    /// GUI window opened and connected to companion.
    Started {
        /// Process ID of the GUI.
        pid: u32,
        /// Version string of the application.
        version: String,
    },

    /// GUI window closed.
    Shutdown,

    /// A job has started.
    JobStarted {
        /// Unique job identifier.
        job_id: String,
        /// Type of job (fetch, sweep, etc.).
        job_type: JobType,
        /// Human-readable description.
        description: String,
    },

    /// Job progress update.
    JobProgress {
        /// Unique job identifier.
        job_id: String,
        /// Current progress count.
        current: u64,
        /// Total items to process.
        total: u64,
        /// Current item being processed.
        message: String,
    },

    /// Job completed successfully.
    JobComplete {
        /// Unique job identifier.
        job_id: String,
        /// Summary message.
        summary: String,
        /// Elapsed time in milliseconds.
        elapsed_ms: u64,
    },

    /// Job failed with an error.
    JobFailed {
        /// Unique job identifier.
        job_id: String,
        /// Error message.
        error: String,
    },

    /// Job was cancelled.
    JobCancelled {
        /// Unique job identifier.
        job_id: String,
    },

    /// Individual sweep result (streamed as they complete).
    SweepResult {
        /// Ticker symbol.
        ticker: String,
        /// Strategy name.
        strategy: String,
        /// Configuration identifier.
        config_id: String,
        /// Sharpe ratio.
        sharpe: f64,
        /// Compound annual growth rate.
        cagr: f64,
        /// Maximum drawdown (negative value).
        max_dd: f64,
    },

    /// Log message.
    Log {
        /// Log level.
        level: LogLevel,
        /// Log message.
        message: String,
        /// Timestamp.
        ts: DateTime<Utc>,
    },

    /// Status bar update.
    Status {
        /// Status message.
        message: String,
    },
}

/// Type of job being executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// Fetching market data.
    Fetch,
    /// Single-strategy parameter sweep.
    Sweep,
    /// Multi-strategy parameter sweep.
    MultiSweep,
    /// YOLO mode (continuous optimization).
    Yolo,
    /// Statistical analysis.
    Analysis,
}

impl std::fmt::Display for JobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobType::Fetch => write!(f, "Fetch"),
            JobType::Sweep => write!(f, "Sweep"),
            JobType::MultiSweep => write!(f, "Multi-Sweep"),
            JobType::Yolo => write!(f, "YOLO"),
            JobType::Analysis => write!(f, "Analysis"),
        }
    }
}

/// Log level for companion log messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Debug information.
    Debug,
    /// Informational message.
    Info,
    /// Warning message.
    Warn,
    /// Error message.
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_companion_event_serialization_roundtrip() {
        let events = vec![
            CompanionEvent::Started {
                pid: 12345,
                version: "0.1.0".to_string(),
            },
            CompanionEvent::Shutdown,
            CompanionEvent::JobStarted {
                job_id: "sweep-1".to_string(),
                job_type: JobType::Sweep,
                description: "5 symbols x 12 strategies".to_string(),
            },
            CompanionEvent::JobProgress {
                job_id: "sweep-1".to_string(),
                current: 50,
                total: 100,
                message: "AAPL x donchian_20".to_string(),
            },
            CompanionEvent::JobComplete {
                job_id: "sweep-1".to_string(),
                summary: "Completed 100 configs".to_string(),
                elapsed_ms: 5000,
            },
            CompanionEvent::JobFailed {
                job_id: "sweep-1".to_string(),
                error: "Network error".to_string(),
            },
            CompanionEvent::JobCancelled {
                job_id: "sweep-1".to_string(),
            },
            CompanionEvent::SweepResult {
                ticker: "AAPL".to_string(),
                strategy: "donchian".to_string(),
                config_id: "20_10".to_string(),
                sharpe: 1.25,
                cagr: 0.15,
                max_dd: -0.12,
            },
            CompanionEvent::Log {
                level: LogLevel::Info,
                message: "Sweep started".to_string(),
                ts: Utc::now(),
            },
            CompanionEvent::Status {
                message: "Running sweep...".to_string(),
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).expect("serialization should succeed");
            let parsed: CompanionEvent =
                serde_json::from_str(&json).expect("deserialization should succeed");
            // Verify the JSON is not empty
            assert!(!json.is_empty());
            // Verify the parsed event has the same type tag
            let original_json = serde_json::to_value(&event).unwrap();
            let parsed_json = serde_json::to_value(&parsed).unwrap();
            assert_eq!(original_json["type"], parsed_json["type"]);
        }
    }

    #[test]
    fn test_job_type_display() {
        assert_eq!(JobType::Fetch.to_string(), "Fetch");
        assert_eq!(JobType::Sweep.to_string(), "Sweep");
        assert_eq!(JobType::MultiSweep.to_string(), "Multi-Sweep");
        assert_eq!(JobType::Yolo.to_string(), "YOLO");
        assert_eq!(JobType::Analysis.to_string(), "Analysis");
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Debug.to_string(), "DEBUG");
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Warn.to_string(), "WARN");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
    }
}
