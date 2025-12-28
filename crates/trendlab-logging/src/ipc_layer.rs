//! IPC-based tracing layer for forwarding logs to the companion terminal.
//!
//! This module provides a custom tracing `Layer` that captures log events
//! and forwards them via a tokio mpsc channel. This is used by the GUI
//! to send logs back to the launcher's companion terminal.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::Level;

#[cfg(feature = "ipc")]
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
#[cfg(feature = "ipc")]
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

/// Log level for IPC messages.
///
/// This mirrors the tracing levels but is serializable for IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Trace level (most verbose).
    Trace,
    /// Debug level.
    Debug,
    /// Informational level.
    Info,
    /// Warning level.
    Warn,
    /// Error level (least verbose).
    Error,
}

impl From<&Level> for LogLevel {
    fn from(level: &Level) -> Self {
        match *level {
            Level::TRACE => LogLevel::Trace,
            Level::DEBUG => LogLevel::Debug,
            Level::INFO => LogLevel::Info,
            Level::WARN => LogLevel::Warn,
            Level::ERROR => LogLevel::Error,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// A log event that can be sent via IPC.
///
/// This contains all the information needed to display a log message
/// in the companion terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Timestamp when the event was recorded.
    pub timestamp: DateTime<Utc>,
    /// Log level.
    pub level: LogLevel,
    /// Target module path (e.g., "trendlab_gui::commands::yolo").
    pub target: String,
    /// The log message.
    pub message: String,
    /// Active span names (innermost to outermost).
    pub spans: Vec<String>,
    /// Additional structured fields from the event.
    pub fields: HashMap<String, String>,
}

impl LogEvent {
    /// Create a new log event.
    pub fn new(level: LogLevel, target: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level,
            target: target.into(),
            message: message.into(),
            spans: Vec::new(),
            fields: HashMap::new(),
        }
    }

    /// Add span context to the event.
    pub fn with_spans(mut self, spans: Vec<String>) -> Self {
        self.spans = spans;
        self
    }

    /// Add a field to the event.
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }
}

/// Visitor that extracts fields from a tracing event.
#[cfg(feature = "ipc")]
struct FieldVisitor {
    message: Option<String>,
    fields: HashMap<String, String>,
}

#[cfg(feature = "ipc")]
impl FieldVisitor {
    fn new() -> Self {
        Self {
            message: None,
            fields: HashMap::new(),
        }
    }
}

#[cfg(feature = "ipc")]
impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let value_str = format!("{:?}", value);
        if field.name() == "message" {
            self.message = Some(value_str);
        } else {
            self.fields.insert(field.name().to_string(), value_str);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.insert(field.name().to_string(), value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name().to_string(), value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.insert(field.name().to_string(), format!("{:.4}", value));
    }
}

/// A tracing Layer that forwards log events via IPC.
///
/// This layer captures tracing events and sends them through a tokio mpsc
/// channel. The receiver (typically in the launcher's companion mode) can
/// then display these logs in the terminal.
///
/// # Example
///
/// ```rust,ignore
/// use tokio::sync::mpsc;
/// use trendlab_logging::{IpcLayer, LogEvent};
///
/// let (tx, rx) = mpsc::channel::<LogEvent>(1000);
/// let layer = IpcLayer::new(tx);
/// ```
#[cfg(feature = "ipc")]
pub struct IpcLayer {
    sender: tokio::sync::mpsc::Sender<LogEvent>,
}

#[cfg(feature = "ipc")]
impl IpcLayer {
    /// Create a new IPC layer with the given sender.
    ///
    /// The sender should be connected to a receiver that processes log events.
    /// If the channel is full, log events will be dropped silently to avoid
    /// blocking the application.
    pub fn new(sender: tokio::sync::mpsc::Sender<LogEvent>) -> Self {
        Self { sender }
    }
}

#[cfg(feature = "ipc")]
impl<S> Layer<S> for IpcLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // Extract fields from the event
        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);

        let message = visitor.message.unwrap_or_default();
        let metadata = event.metadata();

        // Collect span names from the current scope
        let mut spans = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope {
                spans.push(span.name().to_string());
            }
        }
        spans.reverse(); // Outermost to innermost

        let log_event = LogEvent {
            timestamp: Utc::now(),
            level: LogLevel::from(metadata.level()),
            target: metadata.target().to_string(),
            message,
            spans,
            fields: visitor.fields,
        };

        // Try to send, but don't block if the channel is full
        let _ = self.sender.try_send(log_event);
    }
}

/// Stub implementation when IPC feature is disabled.
#[cfg(not(feature = "ipc"))]
pub struct IpcLayer;

#[cfg(not(feature = "ipc"))]
impl IpcLayer {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_from_tracing() {
        assert_eq!(LogLevel::from(&Level::TRACE), LogLevel::Trace);
        assert_eq!(LogLevel::from(&Level::DEBUG), LogLevel::Debug);
        assert_eq!(LogLevel::from(&Level::INFO), LogLevel::Info);
        assert_eq!(LogLevel::from(&Level::WARN), LogLevel::Warn);
        assert_eq!(LogLevel::from(&Level::ERROR), LogLevel::Error);
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Trace.to_string(), "TRACE");
        assert_eq!(LogLevel::Debug.to_string(), "DEBUG");
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Warn.to_string(), "WARN");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
    }

    #[test]
    fn test_log_event_serialization() {
        let event = LogEvent::new(LogLevel::Info, "test::module", "Test message")
            .with_field("key", "value")
            .with_spans(vec!["outer".to_string(), "inner".to_string()]);

        let json = serde_json::to_string(&event).expect("should serialize");
        let parsed: LogEvent = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(parsed.level, LogLevel::Info);
        assert_eq!(parsed.target, "test::module");
        assert_eq!(parsed.message, "Test message");
        assert_eq!(parsed.spans, vec!["outer", "inner"]);
        assert_eq!(parsed.fields.get("key"), Some(&"value".to_string()));
    }
}
