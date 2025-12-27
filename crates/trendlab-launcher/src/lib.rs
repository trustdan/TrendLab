//! TrendLab Unified Launcher
//!
//! Provides a single `trendlab` binary that can launch either the TUI or GUI,
//! with a companion mode that displays progress in the terminal while the GUI runs.

pub mod companion;
pub mod exec;
pub mod ipc;
pub mod prompt;

/// Environment variable name for the companion socket path.
pub const COMPANION_SOCKET_ENV: &str = "TRENDLAB_COMPANION_SOCKET";

/// Current version of the launcher.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
