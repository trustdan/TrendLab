//! Shared logging infrastructure for TrendLab applications.
//!
//! This crate provides unified logging setup using the `tracing` ecosystem,
//! with support for:
//! - File-based logging with daily rotation
//! - IPC-based log forwarding (for GUI → companion terminal)
//! - Environment-based configuration
//!
//! # Usage
//!
//! ```rust,ignore
//! use trendlab_logging::{LogConfig, init_tui_logging};
//!
//! let config = LogConfig::from_env();
//! let _guard = init_tui_logging(&config);
//!
//! tracing::info!("Application started");
//! ```

mod ipc_layer;

pub use ipc_layer::{IpcLayer, LogEvent, LogLevel};

use std::path::PathBuf;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

/// Configuration for TrendLab logging.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Enable logging output.
    pub enabled: bool,
    /// Log level filter (e.g., "info", "debug", "trendlab=debug,polars=warn").
    pub filter: String,
    /// Directory for log files.
    pub log_dir: PathBuf,
    /// Enable daily log rotation.
    pub rotate_daily: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            filter: "info,trendlab=debug,polars=warn".to_string(),
            log_dir: PathBuf::from("data/logs"),
            rotate_daily: true,
        }
    }
}

impl LogConfig {
    /// Create a new LogConfig with the specified filter.
    pub fn new(filter: impl Into<String>) -> Self {
        Self {
            enabled: true,
            filter: filter.into(),
            ..Default::default()
        }
    }

    /// Create LogConfig from environment variables.
    ///
    /// Reads:
    /// - `TRENDLAB_LOG_ENABLED`: Set to "1" to enable logging
    /// - `TRENDLAB_LOG_FILTER`: Log filter string (default: "info,trendlab=debug")
    /// - `TRENDLAB_LOG_DIR`: Log directory (default: "data/logs")
    pub fn from_env() -> Self {
        let enabled = std::env::var("TRENDLAB_LOG_ENABLED")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        let filter = std::env::var("TRENDLAB_LOG_FILTER")
            .unwrap_or_else(|_| "info,trendlab=debug".to_string());

        let log_dir = std::env::var("TRENDLAB_LOG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/logs"));

        Self {
            enabled,
            filter,
            log_dir,
            rotate_daily: true,
        }
    }

    /// Set environment variables for child processes.
    pub fn set_env(&self) {
        if self.enabled {
            std::env::set_var("TRENDLAB_LOG_ENABLED", "1");
            std::env::set_var("TRENDLAB_LOG_FILTER", &self.filter);
            std::env::set_var("TRENDLAB_LOG_DIR", self.log_dir.to_string_lossy().as_ref());
        }
    }
}

/// Guard that ensures logs are flushed on drop.
///
/// Keep this guard alive for the duration of logging.
/// When dropped, it will flush any buffered log entries.
pub struct LogGuard {
    _worker_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl LogGuard {
    fn new(worker_guard: Option<tracing_appender::non_blocking::WorkerGuard>) -> Self {
        Self {
            _worker_guard: worker_guard,
        }
    }

    /// Create an empty guard (no-op).
    pub fn empty() -> Self {
        Self {
            _worker_guard: None,
        }
    }
}

/// Create a file appender with optional daily rotation.
fn create_file_appender(config: &LogConfig) -> RollingFileAppender {
    // Ensure log directory exists
    if let Err(e) = std::fs::create_dir_all(&config.log_dir) {
        eprintln!("Warning: Failed to create log directory: {}", e);
    }

    let rotation = if config.rotate_daily {
        Rotation::DAILY
    } else {
        Rotation::NEVER
    };

    RollingFileAppender::new(rotation, &config.log_dir, "trendlab.log")
}

/// Create an EnvFilter from the config's filter string.
fn create_filter(config: &LogConfig) -> EnvFilter {
    EnvFilter::try_new(&config.filter).unwrap_or_else(|e| {
        eprintln!("Warning: Invalid log filter '{}': {}", config.filter, e);
        EnvFilter::new("info")
    })
}

/// Initialize logging for the launcher (stderr output).
///
/// This is used by the launcher before it spawns TUI or GUI.
/// Logs go to stderr so they don't interfere with the prompt display.
///
/// Returns `None` if logging is disabled.
pub fn init_launcher_logging(config: &LogConfig) -> Option<LogGuard> {
    if !config.enabled {
        return None;
    }

    let filter = create_filter(config);

    // Launcher logs to stderr with a compact format
    let stderr_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_ansi(true)
        .compact()
        .with_writer(std::io::stderr)
        .with_filter(filter);

    tracing_subscriber::registry().with(stderr_layer).init();

    Some(LogGuard::empty())
}

/// Initialize logging for the TUI (file-only output).
///
/// TUI logs only to a file to avoid interfering with the terminal UI.
/// The log file is located at `{log_dir}/trendlab.YYYY-MM-DD.log`.
///
/// Returns `None` if logging is disabled.
pub fn init_tui_logging(config: &LogConfig) -> Option<LogGuard> {
    if !config.enabled {
        return None;
    }

    let filter = create_filter(config);
    let file_appender = create_file_appender(config);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // TUI logs to file only with full details
    let file_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(non_blocking)
        .with_filter(filter);

    tracing_subscriber::registry().with(file_layer).init();

    Some(LogGuard::new(Some(guard)))
}

/// Initialize logging for the GUI (file + optional IPC output).
///
/// GUI logs to both a file and optionally via IPC to the companion terminal.
/// The IPC sender should be connected to the companion's log receiver.
///
/// # Arguments
///
/// * `config` - Logging configuration
/// * `ipc_sender` - Optional channel sender for IPC log forwarding
///
/// Returns `None` if logging is disabled.
#[cfg(feature = "ipc")]
pub fn init_gui_logging(
    config: &LogConfig,
    ipc_sender: Option<tokio::sync::mpsc::Sender<LogEvent>>,
) -> Option<LogGuard> {
    if !config.enabled {
        return None;
    }

    let filter = create_filter(config);
    let file_appender = create_file_appender(config);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // File layer with full details
    let file_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .with_span_events(FmtSpan::CLOSE)
        .with_writer(non_blocking)
        .with_filter(filter.clone());

    if let Some(sender) = ipc_sender {
        // Dual output: file + IPC
        let ipc_layer = IpcLayer::new(sender).with_filter(filter);

        tracing_subscriber::registry()
            .with(file_layer)
            .with(ipc_layer)
            .init();
    } else {
        // File only
        tracing_subscriber::registry().with(file_layer).init();
    }

    Some(LogGuard::new(Some(guard)))
}

/// Initialize logging for the GUI without IPC support.
///
/// This is a simpler version for when the `ipc` feature is not enabled.
#[cfg(not(feature = "ipc"))]
pub fn init_gui_logging(config: &LogConfig) -> Option<LogGuard> {
    init_tui_logging(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.filter, "info,trendlab=debug,polars=warn");
        assert_eq!(config.log_dir, PathBuf::from("data/logs"));
        assert!(config.rotate_daily);
    }

    #[test]
    fn test_log_config_new() {
        let config = LogConfig::new("debug");
        assert!(config.enabled);
        assert_eq!(config.filter, "debug");
    }

    #[test]
    fn test_log_config_from_env() {
        // Clear any existing env vars
        std::env::remove_var("TRENDLAB_LOG_ENABLED");
        std::env::remove_var("TRENDLAB_LOG_FILTER");

        let config = LogConfig::from_env();
        assert!(!config.enabled);

        // Set env vars and test again
        std::env::set_var("TRENDLAB_LOG_ENABLED", "1");
        std::env::set_var("TRENDLAB_LOG_FILTER", "trace");

        let config = LogConfig::from_env();
        assert!(config.enabled);
        assert_eq!(config.filter, "trace");

        // Cleanup
        std::env::remove_var("TRENDLAB_LOG_ENABLED");
        std::env::remove_var("TRENDLAB_LOG_FILTER");
    }
}
