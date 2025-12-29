//! Data panel commands for fetching and managing market data.
//!
//! Data fetching is delegated to the worker thread (same as TUI).
//! This module provides thin wrappers around the engine state.

use chrono::NaiveDate;
use trendlab_engine::worker::WorkerCommand;

use crate::{
    error::GuiError,
    state::{AppState, UniverseInfo},
};

/// Search result from Yahoo Finance.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub type_disp: String,
}

/// Get the universe of sectors and tickers.
#[tauri::command]
pub fn get_universe(state: tauri::State<'_, AppState>) -> UniverseInfo {
    state.get_universe_info()
}

/// Get list of symbols that have cached parquet data.
#[tauri::command]
pub fn get_cached_symbols(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.get_cached_symbols()
}

/// Update the selected tickers.
#[tauri::command]
pub fn update_selection(state: tauri::State<'_, AppState>, tickers: Vec<String>) {
    state.set_selected_tickers(tickers);
}

/// Get the currently selected tickers.
#[tauri::command]
pub fn get_selection(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.get_selected_tickers()
}

/// Search for symbols via Yahoo Finance.
/// Uses the worker thread for the actual search.
#[tauri::command]
pub fn search_symbols(
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<(), GuiError> {
    if query.trim().is_empty() {
        return Ok(());
    }

    state
        .send_command(WorkerCommand::SearchSymbols { query })
        .map_err(|e| GuiError::Internal(format!("Failed to send search command: {}", e)))
}

/// Get search suggestions from the engine state.
#[tauri::command]
pub fn get_search_results(state: tauri::State<'_, AppState>) -> Vec<SearchResult> {
    let engine = state.engine_read();
    engine
        .data
        .search_suggestions
        .iter()
        .map(|s| SearchResult {
            symbol: s.symbol.clone(),
            name: s.name.clone(),
            exchange: s.exchange.clone(),
            type_disp: "EQUITY".to_string(),
        })
        .collect()
}

/// Start a data fetch job for the given symbols.
/// Delegates to the worker thread which will emit progress events.
#[tauri::command]
pub fn fetch_data(
    state: tauri::State<'_, AppState>,
    symbols: Vec<String>,
    start: String,
    end: String,
    force: bool,
) -> Result<(), GuiError> {
    if symbols.is_empty() {
        return Err(GuiError::InvalidInput {
            message: "No symbols provided".to_string(),
        });
    }

    let start_date = parse_date(&start)?;
    let end_date = parse_date(&end)?;

    // Clear any previous cancellation
    state.clear_cancel();

    // Send fetch command to worker
    state
        .send_command(WorkerCommand::FetchData {
            symbols,
            start: start_date,
            end: end_date,
            force,
        })
        .map_err(|e| GuiError::Internal(format!("Failed to send fetch command: {}", e)))
}

/// Cancel any in-progress data operations.
#[tauri::command]
pub fn cancel_fetch(state: tauri::State<'_, AppState>) {
    state.request_cancel();
    let _ = state.send_command(WorkerCommand::Cancel);
}

/// Get the current operation state (idle, fetching, sweeping).
#[tauri::command]
pub fn get_operation_state(state: tauri::State<'_, AppState>) -> OperationStateResponse {
    let engine = state.engine_read();
    match &engine.operation {
        trendlab_engine::app::OperationState::Idle => OperationStateResponse {
            state: "idle".to_string(),
            symbol: None,
            index: None,
            total: None,
        },
        trendlab_engine::app::OperationState::FetchingData { current_symbol, completed, total } => {
            OperationStateResponse {
                state: "fetching".to_string(),
                symbol: Some(current_symbol.clone()),
                index: Some(*completed),
                total: Some(*total),
            }
        }
        trendlab_engine::app::OperationState::RunningSweep { completed, total } => {
            OperationStateResponse {
                state: "sweeping".to_string(),
                symbol: None,
                index: Some(*completed),
                total: Some(*total),
            }
        }
    }
}

/// Response for operation state query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OperationStateResponse {
    pub state: String,
    pub symbol: Option<String>,
    pub index: Option<usize>,
    pub total: Option<usize>,
}

/// Parse a date string in YYYY-MM-DD format.
fn parse_date(s: &str) -> Result<NaiveDate, GuiError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| GuiError::InvalidInput {
        message: format!("Invalid date '{}': {}", s, e),
    })
}
