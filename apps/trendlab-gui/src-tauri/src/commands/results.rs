//! Results panel commands - viewing and exporting sweep results.
//!
//! Provides commands for querying, sorting, filtering, and exporting
//! backtest results from completed sweeps.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

use crate::error::GuiError;
use crate::state::AppState;

// ============================================================================
// Types
// ============================================================================

/// Metrics for a single backtest result.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResultMetrics {
    pub total_return: f64,
    pub cagr: f64,
    pub sharpe: f64,
    pub sortino: f64,
    pub max_drawdown: f64,
    pub calmar: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub num_trades: u32,
    pub turnover: f64,
}

/// A single backtest result row for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultRow {
    /// Unique identifier for this result
    pub id: String,
    /// Symbol tested
    pub symbol: String,
    /// Strategy type (donchian, ma_crossover, etc.)
    pub strategy: String,
    /// Configuration ID string
    pub config_id: String,
    /// Performance metrics
    pub metrics: ResultMetrics,
    /// Equity curve (for sparkline or detail view)
    pub equity_curve: Vec<f64>,
}

/// View mode for results display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// Show best result per ticker
    #[default]
    PerTicker,
    /// Show best result per strategy
    ByStrategy,
    /// Show all configurations
    AllConfigs,
}

impl ViewMode {
    pub fn name(&self) -> &'static str {
        match self {
            ViewMode::PerTicker => "Per Ticker",
            ViewMode::ByStrategy => "By Strategy",
            ViewMode::AllConfigs => "All Configs",
        }
    }
}

/// Metric to sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortMetric {
    #[default]
    Sharpe,
    Cagr,
    Sortino,
    MaxDrawdown,
    Calmar,
    WinRate,
    ProfitFactor,
    TotalReturn,
    NumTrades,
}

impl SortMetric {
    /// Extract the value for this metric from a ResultMetrics.
    pub fn extract(&self, m: &ResultMetrics) -> f64 {
        match self {
            SortMetric::Sharpe => m.sharpe,
            SortMetric::Cagr => m.cagr,
            SortMetric::Sortino => m.sortino,
            SortMetric::MaxDrawdown => m.max_drawdown,
            SortMetric::Calmar => m.calmar,
            SortMetric::WinRate => m.win_rate,
            SortMetric::ProfitFactor => m.profit_factor,
            SortMetric::TotalReturn => m.total_return,
            SortMetric::NumTrades => m.num_trades as f64,
        }
    }
}

/// Summary for a single ticker across all strategies.
#[derive(Debug, Clone, Serialize)]
pub struct TickerSummary {
    pub symbol: String,
    pub configs_tested: usize,
    pub best_strategy: String,
    pub best_sharpe: f64,
    pub avg_sharpe: f64,
    pub best_cagr: f64,
    pub worst_drawdown: f64,
}

/// Summary for a single strategy across all tickers.
#[derive(Debug, Clone, Serialize)]
pub struct StrategySummary {
    pub strategy: String,
    pub tickers_tested: usize,
    pub configs_tested: usize,
    pub avg_sharpe: f64,
    pub best_sharpe: f64,
    pub avg_cagr: f64,
    pub best_cagr: f64,
    pub worst_drawdown: f64,
}

/// Query parameters for getting results.
#[derive(Debug, Clone, Deserialize)]
pub struct ResultsQuery {
    pub view_mode: Option<ViewMode>,
    pub sort_by: Option<SortMetric>,
    pub ascending: Option<bool>,
    pub limit: Option<usize>,
    pub symbol_filter: Option<String>,
    pub strategy_filter: Option<String>,
}

impl Default for ResultsQuery {
    fn default() -> Self {
        Self {
            view_mode: Some(ViewMode::AllConfigs),
            sort_by: Some(SortMetric::Sharpe),
            ascending: Some(false),
            limit: None,
            symbol_filter: None,
            strategy_filter: None,
        }
    }
}

/// Results state stored in AppState.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResultsState {
    /// All results from the last sweep
    pub results: Vec<ResultRow>,
    /// Sweep ID these results belong to
    pub sweep_id: Option<String>,
    /// Currently selected result ID
    pub selected_id: Option<String>,
    /// Current view mode
    pub view_mode: ViewMode,
    /// Current sort metric
    pub sort_by: SortMetric,
    /// Sort ascending
    pub ascending: bool,
}

impl ResultsState {
    /// Check if there are results to display.
    pub fn has_results(&self) -> bool {
        !self.results.is_empty()
    }

    /// Get results count.
    pub fn count(&self) -> usize {
        self.results.len()
    }
}

// ============================================================================
// Commands
// ============================================================================

/// Check if there are results available.
#[tauri::command]
pub fn has_results(state: State<'_, AppState>) -> bool {
    state.get_results_state().has_results()
}

/// Get results count.
#[tauri::command]
pub fn get_results_count(state: State<'_, AppState>) -> usize {
    state.get_results_state().count()
}

/// Get all results with optional filtering and sorting.
#[tauri::command]
pub fn get_results(state: State<'_, AppState>, query: Option<ResultsQuery>) -> Vec<ResultRow> {
    let results_state = state.get_results_state();
    let query = query.unwrap_or_default();

    let mut results = results_state.results.clone();

    // Apply symbol filter
    if let Some(ref symbol) = query.symbol_filter {
        results.retain(|r| r.symbol.eq_ignore_ascii_case(symbol));
    }

    // Apply strategy filter
    if let Some(ref strategy) = query.strategy_filter {
        results.retain(|r| r.strategy.eq_ignore_ascii_case(strategy));
    }

    // Apply view mode filtering
    let view_mode = query.view_mode.unwrap_or(ViewMode::AllConfigs);
    results = match view_mode {
        ViewMode::PerTicker => {
            // Best result per ticker
            let mut best_by_ticker: HashMap<String, ResultRow> = HashMap::new();
            for r in results {
                let key = r.symbol.clone();
                if let Some(existing) = best_by_ticker.get(&key) {
                    if r.metrics.sharpe > existing.metrics.sharpe {
                        best_by_ticker.insert(key, r);
                    }
                } else {
                    best_by_ticker.insert(key, r);
                }
            }
            best_by_ticker.into_values().collect()
        }
        ViewMode::ByStrategy => {
            // Best result per strategy
            let mut best_by_strategy: HashMap<String, ResultRow> = HashMap::new();
            for r in results {
                let key = r.strategy.clone();
                if let Some(existing) = best_by_strategy.get(&key) {
                    if r.metrics.sharpe > existing.metrics.sharpe {
                        best_by_strategy.insert(key, r);
                    }
                } else {
                    best_by_strategy.insert(key, r);
                }
            }
            best_by_strategy.into_values().collect()
        }
        ViewMode::AllConfigs => results,
    };

    // Sort
    let sort_by = query.sort_by.unwrap_or(SortMetric::Sharpe);
    let ascending = query.ascending.unwrap_or(false);
    results.sort_by(|a, b| {
        let val_a = sort_by.extract(&a.metrics);
        let val_b = sort_by.extract(&b.metrics);
        if ascending {
            val_a
                .partial_cmp(&val_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            val_b
                .partial_cmp(&val_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    // Apply limit
    if let Some(limit) = query.limit {
        results.truncate(limit);
    }

    results
}

/// Get ticker summaries (best per ticker).
#[tauri::command]
pub fn get_ticker_summaries(state: State<'_, AppState>) -> Vec<TickerSummary> {
    let results_state = state.get_results_state();

    // Group by ticker
    let mut by_ticker: HashMap<String, Vec<&ResultRow>> = HashMap::new();
    for r in &results_state.results {
        by_ticker.entry(r.symbol.clone()).or_default().push(r);
    }

    // Build summaries
    let mut summaries: Vec<TickerSummary> = by_ticker
        .into_iter()
        .map(|(symbol, rows)| {
            let configs_tested = rows.len();
            let sharpes: Vec<f64> = rows.iter().map(|r| r.metrics.sharpe).collect();
            let avg_sharpe = sharpes.iter().sum::<f64>() / sharpes.len() as f64;

            // Find best by Sharpe
            let best = rows
                .iter()
                .max_by(|a, b| {
                    a.metrics
                        .sharpe
                        .partial_cmp(&b.metrics.sharpe)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap();

            let worst_drawdown = rows
                .iter()
                .map(|r| r.metrics.max_drawdown)
                .fold(0.0_f64, |a, b| a.max(b));

            TickerSummary {
                symbol,
                configs_tested,
                best_strategy: best.strategy.clone(),
                best_sharpe: best.metrics.sharpe,
                avg_sharpe,
                best_cagr: best.metrics.cagr,
                worst_drawdown,
            }
        })
        .collect();

    // Sort by best Sharpe descending
    summaries.sort_by(|a, b| {
        b.best_sharpe
            .partial_cmp(&a.best_sharpe)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    summaries
}

/// Get strategy summaries (comparison across strategies).
#[tauri::command]
pub fn get_strategy_summaries(state: State<'_, AppState>) -> Vec<StrategySummary> {
    let results_state = state.get_results_state();

    // Group by strategy
    let mut by_strategy: HashMap<String, Vec<&ResultRow>> = HashMap::new();
    for r in &results_state.results {
        by_strategy.entry(r.strategy.clone()).or_default().push(r);
    }

    // Build summaries
    let mut summaries: Vec<StrategySummary> = by_strategy
        .into_iter()
        .map(|(strategy, rows)| {
            let configs_tested = rows.len();

            // Count unique tickers
            let tickers: std::collections::HashSet<_> = rows.iter().map(|r| &r.symbol).collect();
            let tickers_tested = tickers.len();

            let sharpes: Vec<f64> = rows.iter().map(|r| r.metrics.sharpe).collect();
            let cagrs: Vec<f64> = rows.iter().map(|r| r.metrics.cagr).collect();

            let avg_sharpe = sharpes.iter().sum::<f64>() / sharpes.len() as f64;
            let best_sharpe = sharpes.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let avg_cagr = cagrs.iter().sum::<f64>() / cagrs.len() as f64;
            let best_cagr = cagrs.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let worst_drawdown = rows
                .iter()
                .map(|r| r.metrics.max_drawdown)
                .fold(0.0_f64, |a, b| a.max(b));

            StrategySummary {
                strategy,
                tickers_tested,
                configs_tested,
                avg_sharpe,
                best_sharpe,
                avg_cagr,
                best_cagr,
                worst_drawdown,
            }
        })
        .collect();

    // Sort by average Sharpe descending
    summaries.sort_by(|a, b| {
        b.avg_sharpe
            .partial_cmp(&a.avg_sharpe)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    summaries
}

/// Get a single result by ID.
#[tauri::command]
pub fn get_result_detail(
    state: State<'_, AppState>,
    result_id: String,
) -> Result<ResultRow, GuiError> {
    let results_state = state.get_results_state();
    results_state
        .results
        .iter()
        .find(|r| r.id == result_id)
        .cloned()
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })
}

/// Set the selected result ID.
#[tauri::command]
pub fn select_result(state: State<'_, AppState>, result_id: Option<String>) {
    state.set_selected_result(result_id);
}

/// Get the selected result ID.
#[tauri::command]
pub fn get_selected_result(state: State<'_, AppState>) -> Option<String> {
    state.get_results_state().selected_id
}

/// Set view mode.
#[tauri::command]
pub fn set_view_mode(state: State<'_, AppState>, view_mode: ViewMode) {
    state.set_view_mode(view_mode);
}

/// Get view mode.
#[tauri::command]
pub fn get_view_mode(state: State<'_, AppState>) -> ViewMode {
    state.get_results_state().view_mode
}

/// Set sort configuration.
#[tauri::command]
pub fn set_sort_config(state: State<'_, AppState>, sort_by: SortMetric, ascending: bool) {
    state.set_sort_config(sort_by, ascending);
}

/// Export strategy artifact for a result.
#[tauri::command]
pub fn export_artifact(state: State<'_, AppState>, result_id: String) -> Result<String, GuiError> {
    let results_state = state.get_results_state();
    let result = results_state
        .results
        .iter()
        .find(|r| r.id == result_id)
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })?;

    // TODO: Actually generate and save artifact using trendlab_core::create_donchian_artifact
    // For now, return a placeholder path
    let artifact_path = format!(
        "artifacts/{}_{}_{}.json",
        result.symbol, result.strategy, result.config_id
    );

    Ok(artifact_path)
}

/// Clear all results.
#[tauri::command]
pub fn clear_results(state: State<'_, AppState>) {
    state.clear_results();
}
