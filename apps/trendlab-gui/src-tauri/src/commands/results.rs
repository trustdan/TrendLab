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
    state.has_results()
}

/// Get results count.
#[tauri::command]
pub fn get_results_count(state: State<'_, AppState>) -> usize {
    state.get_results_count()
}

/// Get all results with optional filtering and sorting.
#[tauri::command]
pub fn get_results(state: State<'_, AppState>, query: Option<ResultsQuery>) -> Vec<ResultRow> {
    let engine = state.engine_read();
    let query = query.unwrap_or_default();

    // Convert engine's SweepConfigResult to GUI's ResultRow
    let mut results: Vec<ResultRow> = engine
        .results
        .results
        .iter()
        .map(|r| ResultRow {
            id: format!("{:?}", r.config_id),
            symbol: "".to_string(), // SweepConfigResult doesn't have symbol
            strategy: format!("{:?}", r.config_id),
            config_id: format!("{:?}", r.config_id),
            metrics: ResultMetrics {
                total_return: r.metrics.total_return,
                cagr: r.metrics.cagr,
                sharpe: r.metrics.sharpe,
                sortino: r.metrics.sortino,
                max_drawdown: r.metrics.max_drawdown,
                calmar: r.metrics.calmar,
                win_rate: r.metrics.win_rate,
                profit_factor: r.metrics.profit_factor,
                num_trades: r.metrics.num_trades,
                turnover: r.metrics.turnover,
            },
            equity_curve: r.backtest_result.equity.iter().map(|p| p.equity).collect(),
        })
        .collect();
    drop(engine); // Release lock before filtering

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
    let engine = state.engine_read();

    // Use engine's ticker_summaries from multi-sweep results
    engine
        .results
        .ticker_summaries
        .iter()
        .map(|t| TickerSummary {
            symbol: t.symbol.clone(),
            configs_tested: 1,
            best_strategy: format!("{}/{}", t.best_config_entry, t.best_config_exit),
            best_sharpe: t.sharpe,
            avg_sharpe: t.sharpe,
            best_cagr: t.cagr,
            worst_drawdown: t.max_drawdown,
        })
        .collect()
}

/// Get strategy summaries (comparison across strategies).
#[tauri::command]
pub fn get_strategy_summaries(state: State<'_, AppState>) -> Vec<StrategySummary> {
    let engine = state.engine_read();

    // Use multi-strategy result's strategy_comparison if available
    // StrategyComparisonEntry has: strategy_type, total_configs_tested, avg_cagr, avg_sharpe, best_sharpe, worst_drawdown
    if let Some(ref multi_strategy) = engine.results.multi_strategy_result {
        return multi_strategy
            .strategy_comparison
            .iter()
            .map(|s| StrategySummary {
                strategy: s.strategy_type.name().to_string(),
                tickers_tested: 0, // Not tracked in StrategyComparisonEntry
                configs_tested: s.total_configs_tested,
                avg_sharpe: s.avg_sharpe,
                best_sharpe: s.best_sharpe,
                avg_cagr: s.avg_cagr,
                best_cagr: s.avg_cagr, // Use avg_cagr as fallback (best_cagr not tracked)
                worst_drawdown: s.worst_drawdown,
            })
            .collect();
    }

    // Fallback: return empty if no multi-strategy results
    Vec::new()
}

/// Get a single result by index.
#[tauri::command]
pub fn get_result_detail(
    state: State<'_, AppState>,
    result_id: String,
) -> Result<ResultRow, GuiError> {
    let engine = state.engine_read();

    // Find by config_id string match
    engine
        .results
        .results
        .iter()
        .find(|r| format!("{:?}", r.config_id) == result_id)
        .map(|r| ResultRow {
            id: format!("{:?}", r.config_id),
            symbol: "".to_string(),
            strategy: format!("{:?}", r.config_id),
            config_id: format!("{:?}", r.config_id),
            metrics: ResultMetrics {
                total_return: r.metrics.total_return,
                cagr: r.metrics.cagr,
                sharpe: r.metrics.sharpe,
                sortino: r.metrics.sortino,
                max_drawdown: r.metrics.max_drawdown,
                calmar: r.metrics.calmar,
                win_rate: r.metrics.win_rate,
                profit_factor: r.metrics.profit_factor,
                num_trades: r.metrics.num_trades,
                turnover: r.metrics.turnover,
            },
            equity_curve: r.backtest_result.equity.iter().map(|p| p.equity).collect(),
        })
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })
}

/// Set the selected result ID.
#[tauri::command]
pub fn select_result(state: State<'_, AppState>, result_id: Option<String>) {
    state.set_selected_result(result_id);
}

/// Get the selected result index.
#[tauri::command]
pub fn get_selected_result(state: State<'_, AppState>) -> Option<String> {
    let engine = state.engine_read();
    let idx = engine.results.selected_index;
    engine.results.results.get(idx).map(|r| format!("{:?}", r.config_id))
}

/// Set view mode.
#[tauri::command]
pub fn set_view_mode(state: State<'_, AppState>, view_mode: ViewMode) {
    use trendlab_engine::app::ResultsViewMode as EngineViewMode;
    let engine_mode = match view_mode {
        ViewMode::PerTicker => EngineViewMode::PerTicker,
        ViewMode::ByStrategy => EngineViewMode::Aggregated,
        ViewMode::AllConfigs => EngineViewMode::SingleSymbol,
    };
    state.set_view_mode(engine_mode);
}

/// Get view mode.
#[tauri::command]
pub fn get_view_mode(state: State<'_, AppState>) -> ViewMode {
    use trendlab_engine::app::ResultsViewMode as EngineViewMode;
    let engine = state.engine_read();
    match engine.results.view_mode {
        EngineViewMode::SingleSymbol => ViewMode::AllConfigs,
        EngineViewMode::PerTicker => ViewMode::PerTicker,
        EngineViewMode::Aggregated => ViewMode::ByStrategy,
        EngineViewMode::Leaderboard => ViewMode::PerTicker,
    }
}

/// Set sort column (0-based index).
#[tauri::command]
pub fn set_sort_config(state: State<'_, AppState>, sort_by: SortMetric, _ascending: bool) {
    // Map SortMetric to column index
    let column = match sort_by {
        SortMetric::Sharpe => 0,
        SortMetric::Cagr => 1,
        SortMetric::Sortino => 2,
        SortMetric::MaxDrawdown => 3,
        SortMetric::Calmar => 4,
        SortMetric::WinRate => 5,
        SortMetric::ProfitFactor => 6,
        SortMetric::TotalReturn => 7,
        SortMetric::NumTrades => 8,
    };
    state.set_sort_column(column);
}

/// Export strategy artifact for a result.
#[tauri::command]
pub fn export_artifact(state: State<'_, AppState>, result_id: String) -> Result<String, GuiError> {
    let engine = state.engine_read();

    let result = engine
        .results
        .results
        .iter()
        .find(|r| format!("{:?}", r.config_id) == result_id)
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })?;

    // TODO: Actually generate and save artifact using trendlab_core::create_donchian_artifact
    // For now, return a placeholder path
    let artifact_path = format!("artifacts/{:?}.json", result.config_id);

    Ok(artifact_path)
}

/// Clear all results.
#[tauri::command]
pub fn clear_results(state: State<'_, AppState>) {
    state.clear_results();
}
