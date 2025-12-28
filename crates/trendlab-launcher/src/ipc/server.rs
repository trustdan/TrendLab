//! IPC server for companion to receive events from GUI.
//!
//! This module runs in the companion terminal to receive events from the GUI.

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::types::CompanionEvent;

/// Server for receiving events from the GUI.
pub struct CompanionServer {
    listener: TcpListener,
    local_addr: String,
}

impl CompanionServer {
    /// Bind to a TCP port on localhost.
    ///
    /// Uses an ephemeral port (0) by default, or a specific port if provided.
    pub async fn bind(port: u16) -> std::io::Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await?;

        // Get the actual bound address (important for ephemeral ports)
        let local_addr = listener.local_addr()?.to_string();

        Ok(Self {
            listener,
            local_addr,
        })
    }

    /// Get the local address (e.g., "127.0.0.1:54321").
    pub fn local_addr(&self) -> &str {
        &self.local_addr
    }

    /// Accept a connection and forward events to the channel.
    ///
    /// This runs until the connection is closed or an error occurs.
    /// Returns `Ok(())` when the connection is cleanly closed.
    pub async fn accept_and_forward(
        &self,
        tx: mpsc::Sender<CompanionEvent>,
    ) -> std::io::Result<()> {
        let (stream, _peer_addr) = self.listener.accept().await?;
        let reader = BufReader::new(stream);
        let mut lines = reader.lines();

        while let Some(line) = lines.next_line().await? {
            if let Ok(event) = serde_json::from_str::<CompanionEvent>(&line) {
                if tx.send(event).await.is_err() {
                    // Receiver dropped, stop processing
                    break;
                }
            }
        }

        Ok(())
    }

    /// Run the server loop, accepting multiple connections.
    ///
    /// Each connection is handled sequentially. Returns when the channel is closed.
    pub async fn run(&self, tx: mpsc::Sender<CompanionEvent>) {
        loop {
            match self.accept_and_forward(tx.clone()).await {
                Ok(()) => {
                    // Connection closed, wait for next
                    continue;
                }
                Err(e) => {
                    // Log error but continue accepting
                    eprintln!("IPC connection error: {}", e);
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bind_ephemeral_port() {
        let server = CompanionServer::bind(0).await.unwrap();
        let addr = server.local_addr();

        // Should be localhost with a non-zero port
        assert!(addr.starts_with("127.0.0.1:"));
        let port: u16 = addr.split(':').next_back().unwrap().parse().unwrap();
        assert!(port > 0);
    }
}
