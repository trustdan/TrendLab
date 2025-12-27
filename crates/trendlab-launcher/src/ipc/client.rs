//! IPC client for GUI to send events to companion.
//!
//! This module is used by the GUI to emit events to the companion terminal.

use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::RwLock;

use super::types::CompanionEvent;

/// Client for sending events to the companion terminal.
///
/// Thread-safe and gracefully handles missing companion (no-op).
#[derive(Clone)]
pub struct CompanionClient {
    inner: Arc<RwLock<Option<TcpStream>>>,
}

impl CompanionClient {
    /// Try to connect to the companion server.
    ///
    /// Returns `Some(client)` if connection succeeds, `None` if companion is not running.
    pub async fn try_connect() -> Option<Self> {
        let addr = super::socket_addr_from_env()?;

        let stream = TcpStream::connect(&addr).await.ok()?;
        Some(Self {
            inner: Arc::new(RwLock::new(Some(stream))),
        })
    }

    /// Create a no-op client (used when companion is not running).
    pub fn noop() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if the client is connected.
    pub async fn is_connected(&self) -> bool {
        self.inner.read().await.is_some()
    }

    /// Emit an event to the companion.
    ///
    /// No-op if not connected. Disconnects on error.
    pub async fn emit(&self, event: CompanionEvent) {
        let mut guard = self.inner.write().await;
        if let Some(ref mut stream) = *guard {
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(_) => return,
            };
            let msg = format!("{}\n", json);

            if stream.write_all(msg.as_bytes()).await.is_err() {
                // Disconnect on error
                *guard = None;
            }
        }
    }

    /// Send Started event with current process info.
    pub async fn send_started(&self) {
        self.emit(CompanionEvent::Started {
            pid: std::process::id(),
            version: crate::VERSION.to_string(),
        })
        .await;
    }

    /// Send Shutdown event.
    pub async fn send_shutdown(&self) {
        self.emit(CompanionEvent::Shutdown).await;
    }
}

impl Default for CompanionClient {
    fn default() -> Self {
        Self::noop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_client() {
        let client = CompanionClient::noop();
        assert!(!client.is_connected().await);

        // Should not panic on emit
        client
            .emit(CompanionEvent::Status {
                message: "test".to_string(),
            })
            .await;
    }
}
