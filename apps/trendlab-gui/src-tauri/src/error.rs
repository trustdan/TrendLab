use serde::Serialize;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub code: &'static str,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub retryable: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum GuiError {
    #[error("{message}")]
    InvalidInput { message: String },

    #[error("{0}")]
    InvalidState(String),

    #[error("Not found: {resource}")]
    NotFound { resource: String },

    #[error("Internal error: {0}")]
    Internal(String),
}

impl GuiError {
    pub fn envelope(&self) -> ErrorEnvelope {
        match self {
            GuiError::InvalidInput { message } => ErrorEnvelope {
                code: "InvalidInput",
                message: message.clone(),
                details: None,
                retryable: false,
            },
            GuiError::InvalidState(message) => ErrorEnvelope {
                code: "InvalidState",
                message: message.clone(),
                details: None,
                retryable: false,
            },
            GuiError::NotFound { resource } => ErrorEnvelope {
                code: "NotFound",
                message: format!("Not found: {}", resource),
                details: None,
                retryable: false,
            },
            GuiError::Internal(message) => ErrorEnvelope {
                code: "Internal",
                message: message.clone(),
                details: None,
                retryable: true,
            },
        }
    }
}

// Tauri uses command results as strings; we serialize a structured envelope.
impl serde::Serialize for GuiError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.envelope().serialize(serializer)
    }
}

/// Extension trait for RwLock to safely acquire locks with proper error handling.
///
/// If a lock is poisoned (another thread panicked while holding it), this will
/// recover the value and log a warning rather than propagating the panic.
pub trait RwLockExt<T> {
    /// Acquire a read lock, recovering from poison if necessary.
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T>;

    /// Acquire a write lock, recovering from poison if necessary.
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T>;

    /// Try to acquire a read lock, returning an error on poison.
    fn try_read_strict(&self) -> Result<RwLockReadGuard<'_, T>, GuiError>;

    /// Try to acquire a write lock, returning an error on poison.
    fn try_write_strict(&self) -> Result<RwLockWriteGuard<'_, T>, GuiError>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T> {
        match self.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("RwLock was poisoned, recovering read guard");
                poisoned.into_inner()
            }
        }
    }

    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T> {
        match self.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("RwLock was poisoned, recovering write guard");
                poisoned.into_inner()
            }
        }
    }

    fn try_read_strict(&self) -> Result<RwLockReadGuard<'_, T>, GuiError> {
        self.read()
            .map_err(|_| GuiError::Internal("Lock poisoned (read)".to_string()))
    }

    fn try_write_strict(&self) -> Result<RwLockWriteGuard<'_, T>, GuiError> {
        self.write()
            .map_err(|_| GuiError::Internal("Lock poisoned (write)".to_string()))
    }
}
