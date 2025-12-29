//! TrendLab Engine - Shared state and worker for TUI and GUI
//!
//! This crate provides the core engine that powers both the TUI and GUI interfaces.
//! It contains:
//! - `app`: Application state (data, strategy, sweep, results, chart, yolo substates)
//! - `worker`: Background worker thread for async operations (data fetch, sweeps)
//!
//! The TUI and GUI both consume this crate:
//! - TUI: Uses ratatui to render the state
//! - GUI: Uses Tauri commands to expose state to React frontend

pub mod app;
pub mod worker;

// Re-export main types for convenience
pub use app::App;
pub use worker::{WorkerChannels, WorkerCommand, WorkerUpdate};
