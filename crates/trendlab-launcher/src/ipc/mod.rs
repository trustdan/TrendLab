//! IPC (Inter-Process Communication) module for companion mode.
//!
//! Uses TCP localhost for cross-platform communication between the GUI and companion terminal.

pub mod client;
pub mod server;
pub mod types;

pub use client::CompanionClient;
pub use server::CompanionServer;
pub use types::{CompanionEvent, JobType, LogLevel};

/// Default port for companion IPC (ephemeral, but we specify for predictability).
pub const DEFAULT_PORT: u16 = 0; // 0 = let OS assign

/// Generate a socket address for IPC.
///
/// Returns `127.0.0.1:{port}` where port is 0 (ephemeral) or specified.
pub fn socket_addr(port: u16) -> String {
    format!("127.0.0.1:{}", port)
}

/// Get the socket address from environment, if set.
pub fn socket_addr_from_env() -> Option<String> {
    std::env::var(crate::COMPANION_SOCKET_ENV).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_addr() {
        let addr = socket_addr(12345);
        assert_eq!(addr, "127.0.0.1:12345");
    }

    #[test]
    fn test_socket_addr_ephemeral() {
        let addr = socket_addr(0);
        assert_eq!(addr, "127.0.0.1:0");
    }
}
