use serde::Serialize;

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
