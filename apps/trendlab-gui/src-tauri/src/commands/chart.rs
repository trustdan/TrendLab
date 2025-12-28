//! Chart panel commands - visualization data for TradingView Lightweight Charts.
//!
//! Provides commands for fetching candlestick data, equity curves, trade markers,
//! and managing chart display state.

use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::{debug, trace, warn};

use crate::error::GuiError;
use crate::state::AppState;

// ============================================================================
// Types
// ============================================================================

/// Chart display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChartMode {
    /// Candlestick OHLC chart
    #[default]
    Candlestick,
    /// Single equity curve
    Equity,
    /// Multiple tickers overlaid
    MultiTicker,
    /// Combined portfolio equity
    Portfolio,
    /// Strategy comparison
    StrategyComparison,
}

impl ChartMode {
    pub fn name(&self) -> &'static str {
        match self {
            ChartMode::Candlestick => "Candlestick",
            ChartMode::Equity => "Equity Curve",
            ChartMode::MultiTicker => "Multi-Ticker",
            ChartMode::Portfolio => "Portfolio",
            ChartMode::StrategyComparison => "Strategy Comparison",
        }
    }
}

/// A single OHLCV candlestick bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleData {
    /// Unix timestamp in seconds
    pub time: i64,
    /// Open price
    pub open: f64,
    /// High price
    pub high: f64,
    /// Low price
    pub low: f64,
    /// Close price
    pub close: f64,
    /// Volume
    pub volume: f64,
}

/// A single point on an equity curve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    /// Unix timestamp in seconds
    pub time: i64,
    /// Equity value
    pub value: f64,
}

/// Drawdown point for overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawdownPoint {
    /// Unix timestamp in seconds
    pub time: i64,
    /// Drawdown as decimal (e.g., -0.15 for -15%)
    pub drawdown: f64,
}

/// A trade marker for chart display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeMarker {
    /// Unix timestamp in seconds
    pub time: i64,
    /// Price at entry/exit
    pub price: f64,
    /// "entry" or "exit"
    pub marker_type: String,
    /// "long" or "short"
    pub direction: String,
    /// PnL for exit markers (None for entries)
    pub pnl: Option<f64>,
    /// Tooltip text
    pub text: String,
}

/// Named equity curve for multi-series charts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedEquityCurve {
    /// Series identifier (ticker symbol or strategy name)
    pub name: String,
    /// Display color (hex)
    pub color: String,
    /// Equity points
    pub data: Vec<EquityPoint>,
}

/// Complete chart data response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartData {
    /// Candlestick data (for Candlestick mode)
    pub candles: Option<Vec<CandleData>>,
    /// Primary equity curve
    pub equity: Option<Vec<EquityPoint>>,
    /// Multiple equity curves (for MultiTicker/StrategyComparison)
    pub curves: Option<Vec<NamedEquityCurve>>,
    /// Drawdown overlay
    pub drawdown: Option<Vec<DrawdownPoint>>,
    /// Trade markers
    pub trades: Option<Vec<TradeMarker>>,
}

/// Chart overlay options.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChartOverlays {
    /// Show drawdown overlay
    pub drawdown: bool,
    /// Show volume subplot
    pub volume: bool,
    /// Show trade markers
    pub trades: bool,
    /// Show crosshair
    pub crosshair: bool,
}

/// Chart state stored in AppState.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartState {
    /// Current chart mode
    pub mode: ChartMode,
    /// Current symbol (for Candlestick/Equity modes)
    pub symbol: Option<String>,
    /// Current strategy (for Equity mode)
    pub strategy: Option<String>,
    /// Current config ID (for Equity mode)
    pub config_id: Option<String>,
    /// Overlay visibility
    pub overlays: ChartOverlays,
}

impl Default for ChartState {
    fn default() -> Self {
        Self {
            mode: ChartMode::Candlestick,
            symbol: None,
            strategy: None,
            config_id: None,
            overlays: ChartOverlays {
                drawdown: false,
                volume: true,
                trades: true,
                crosshair: true,
            },
        }
    }
}

// ============================================================================
// Commands
// ============================================================================

/// Get current chart state.
#[tauri::command]
pub fn get_chart_state(state: State<'_, AppState>) -> ChartState {
    state.get_chart_state()
}

/// Set chart mode.
#[tauri::command]
pub fn set_chart_mode(state: State<'_, AppState>, mode: ChartMode) {
    state.set_chart_mode(mode);
}

/// Set chart symbol/strategy/config for single result view.
#[tauri::command]
pub fn set_chart_selection(
    state: State<'_, AppState>,
    symbol: Option<String>,
    strategy: Option<String>,
    config_id: Option<String>,
) {
    state.set_chart_selection(symbol, strategy, config_id);
}

/// Toggle overlay visibility.
#[tauri::command]
pub fn toggle_overlay(state: State<'_, AppState>, overlay: String, enabled: bool) {
    state.toggle_chart_overlay(&overlay, enabled);
}

/// Get overlay settings.
#[tauri::command]
pub fn get_overlays(state: State<'_, AppState>) -> ChartOverlays {
    state.get_chart_state().overlays
}

/// Get candlestick data for a symbol.
#[tauri::command]
pub fn get_candle_data(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<Vec<CandleData>, GuiError> {
    debug!(symbol = %symbol, "Loading candle data");

    // Build path to parquet file
    let parquet_dir = state.data_config.parquet_dir();
    let symbol_dir = parquet_dir.join("1d").join(format!("symbol={}", symbol));

    trace!(path = %symbol_dir.display(), "Checking symbol directory");

    if !symbol_dir.exists() {
        warn!(symbol = %symbol, path = %symbol_dir.display(), "Symbol directory not found");
        return Err(GuiError::NotFound {
            resource: format!("Candle data for {}", symbol),
        });
    }

    // Find parquet files in the symbol directory
    let mut candles = Vec::new();

    let mut parquet_files_found = 0;
    if let Ok(entries) = std::fs::read_dir(&symbol_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "parquet").unwrap_or(false) {
                parquet_files_found += 1;
                trace!(file = %path.display(), "Scanning parquet file");
                // Read parquet file using polars
                if let Ok(df) = polars::prelude::LazyFrame::scan_parquet(&path, Default::default())
                    .and_then(|lf| lf.collect())
                {
                    trace!(rows = df.height(), "Parquet file loaded");
                    // Extract columns
                    let dates = df.column("date").ok();
                    let opens = df.column("open").ok();
                    let highs = df.column("high").ok();
                    let lows = df.column("low").ok();
                    let closes = df.column("close").ok();
                    let volumes = df.column("volume").ok();

                    if let (Some(d), Some(o), Some(h), Some(l), Some(c), Some(v)) =
                        (dates, opens, highs, lows, closes, volumes)
                    {
                        let len = d.len();
                        for i in 0..len {
                            // Parse date to timestamp
                            let time = if let Ok(date_str) = d.str() {
                                if let Some(s) = date_str.get(i) {
                                    // Parse YYYY-MM-DD to unix timestamp
                                    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                        .map(|d| {
                                            d.and_hms_opt(0, 0, 0)
                                                .unwrap()
                                                .and_utc()
                                                .timestamp()
                                        })
                                        .unwrap_or(0)
                                } else {
                                    0
                                }
                            } else {
                                0
                            };

                            let open = o.f64().ok().and_then(|s| s.get(i)).unwrap_or(0.0);
                            let high = h.f64().ok().and_then(|s| s.get(i)).unwrap_or(0.0);
                            let low = l.f64().ok().and_then(|s| s.get(i)).unwrap_or(0.0);
                            let close = c.f64().ok().and_then(|s| s.get(i)).unwrap_or(0.0);
                            let volume = v.f64().ok().and_then(|s| s.get(i)).unwrap_or(0.0);

                            candles.push(CandleData {
                                time,
                                open,
                                high,
                                low,
                                close,
                                volume,
                            });
                        }
                    }
                }
            }
        }
    }

    // Sort by time
    candles.sort_by_key(|c| c.time);

    debug!(
        symbol = %symbol,
        parquet_files = parquet_files_found,
        candles = candles.len(),
        "Candle data loaded"
    );

    if candles.is_empty() {
        warn!(symbol = %symbol, parquet_files = parquet_files_found, "No candle data found despite directory existing");
        return Err(GuiError::NotFound {
            resource: format!("Candle data for {}", symbol),
        });
    }

    Ok(candles)
}

/// Get equity curve for a specific result.
#[tauri::command]
pub fn get_equity_curve(
    state: State<'_, AppState>,
    result_id: String,
) -> Result<Vec<EquityPoint>, GuiError> {
    let results_state = state.get_results_state();

    let result = results_state
        .results
        .iter()
        .find(|r| r.id == result_id)
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })?;

    // Get candle data to map equity values to timestamps
    let candles = get_candle_data(state.clone(), result.symbol.clone())?;

    // Map equity curve values to timestamps
    let equity: Vec<EquityPoint> = result
        .equity_curve
        .iter()
        .enumerate()
        .filter_map(|(i, &value)| {
            candles.get(i).map(|c| EquityPoint {
                time: c.time,
                value,
            })
        })
        .collect();

    Ok(equity)
}

/// Get drawdown curve for a specific result.
#[tauri::command]
pub fn get_drawdown_curve(
    state: State<'_, AppState>,
    result_id: String,
) -> Result<Vec<DrawdownPoint>, GuiError> {
    let results_state = state.get_results_state();

    let result = results_state
        .results
        .iter()
        .find(|r| r.id == result_id)
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })?;

    // Get candle data for timestamps
    let candles = get_candle_data(state.clone(), result.symbol.clone())?;

    // Calculate drawdown from equity curve
    let mut peak = f64::NEG_INFINITY;
    let drawdown: Vec<DrawdownPoint> = result
        .equity_curve
        .iter()
        .enumerate()
        .filter_map(|(i, &value)| {
            peak = peak.max(value);
            let dd = if peak > 0.0 {
                (value - peak) / peak
            } else {
                0.0
            };
            candles.get(i).map(|c| DrawdownPoint {
                time: c.time,
                drawdown: dd,
            })
        })
        .collect();

    Ok(drawdown)
}

/// Get multiple equity curves for comparison.
#[tauri::command]
pub fn get_multi_ticker_curves(
    state: State<'_, AppState>,
) -> Result<Vec<NamedEquityCurve>, GuiError> {
    let results_state = state.get_results_state();

    // Get best result per ticker
    let mut best_by_ticker: std::collections::HashMap<String, &crate::commands::results::ResultRow> =
        std::collections::HashMap::new();

    for r in &results_state.results {
        if let Some(existing) = best_by_ticker.get(&r.symbol) {
            if r.metrics.sharpe > existing.metrics.sharpe {
                best_by_ticker.insert(r.symbol.clone(), r);
            }
        } else {
            best_by_ticker.insert(r.symbol.clone(), r);
        }
    }

    // Color palette for series
    let colors = [
        "#7aa2f7", // Blue
        "#9ece6a", // Green
        "#f7768e", // Red/Pink
        "#e0af68", // Orange
        "#bb9af7", // Purple
        "#73daca", // Cyan
        "#ff9e64", // Peach
        "#2ac3de", // Teal
    ];

    let mut curves: Vec<NamedEquityCurve> = Vec::new();

    for (i, (symbol, result)) in best_by_ticker.iter().enumerate() {
        // Get timestamps from candle data
        if let Ok(candles) = get_candle_data(state.clone(), symbol.clone()) {
            let data: Vec<EquityPoint> = result
                .equity_curve
                .iter()
                .enumerate()
                .filter_map(|(j, &value)| {
                    candles.get(j).map(|c| EquityPoint {
                        time: c.time,
                        value,
                    })
                })
                .collect();

            curves.push(NamedEquityCurve {
                name: symbol.clone(),
                color: colors[i % colors.len()].to_string(),
                data,
            });
        }
    }

    Ok(curves)
}

/// Get portfolio (combined) equity curve.
#[tauri::command]
pub fn get_portfolio_curve(state: State<'_, AppState>) -> Result<Vec<EquityPoint>, GuiError> {
    let results_state = state.get_results_state();

    if results_state.results.is_empty() {
        return Err(GuiError::NotFound {
            resource: "No results for portfolio".to_string(),
        });
    }

    // Get best result per ticker
    let mut best_by_ticker: std::collections::HashMap<String, &crate::commands::results::ResultRow> =
        std::collections::HashMap::new();

    for r in &results_state.results {
        if let Some(existing) = best_by_ticker.get(&r.symbol) {
            if r.metrics.sharpe > existing.metrics.sharpe {
                best_by_ticker.insert(r.symbol.clone(), r);
            }
        } else {
            best_by_ticker.insert(r.symbol.clone(), r);
        }
    }

    // Find the result with the most data points
    let max_len = best_by_ticker
        .values()
        .map(|r| r.equity_curve.len())
        .max()
        .unwrap_or(0);

    if max_len == 0 {
        return Err(GuiError::NotFound {
            resource: "No equity data for portfolio".to_string(),
        });
    }

    // Get timestamps from first symbol's candle data
    let first_symbol = best_by_ticker.keys().next().unwrap();
    let candles = get_candle_data(state.clone(), first_symbol.clone())?;

    // Sum equity curves (equal weight)
    let n_curves = best_by_ticker.len() as f64;
    let mut portfolio: Vec<EquityPoint> = Vec::new();

    for i in 0..max_len {
        let sum: f64 = best_by_ticker
            .values()
            .filter_map(|r| r.equity_curve.get(i))
            .sum();

        // Average (equal weight)
        let value = sum / n_curves;

        if let Some(c) = candles.get(i) {
            portfolio.push(EquityPoint {
                time: c.time,
                value,
            });
        }
    }

    Ok(portfolio)
}

/// Get strategy comparison curves.
#[tauri::command]
pub fn get_strategy_curves(
    state: State<'_, AppState>,
) -> Result<Vec<NamedEquityCurve>, GuiError> {
    let results_state = state.get_results_state();

    // Get best result per strategy
    let mut best_by_strategy: std::collections::HashMap<String, &crate::commands::results::ResultRow> =
        std::collections::HashMap::new();

    for r in &results_state.results {
        if let Some(existing) = best_by_strategy.get(&r.strategy) {
            if r.metrics.sharpe > existing.metrics.sharpe {
                best_by_strategy.insert(r.strategy.clone(), r);
            }
        } else {
            best_by_strategy.insert(r.strategy.clone(), r);
        }
    }

    // Color palette for series
    let colors = [
        "#7aa2f7", // Blue
        "#9ece6a", // Green
        "#f7768e", // Red/Pink
        "#e0af68", // Orange
        "#bb9af7", // Purple
        "#73daca", // Cyan
    ];

    let mut curves: Vec<NamedEquityCurve> = Vec::new();

    for (i, (strategy, result)) in best_by_strategy.iter().enumerate() {
        // Get timestamps from candle data
        if let Ok(candles) = get_candle_data(state.clone(), result.symbol.clone()) {
            let data: Vec<EquityPoint> = result
                .equity_curve
                .iter()
                .enumerate()
                .filter_map(|(j, &value)| {
                    candles.get(j).map(|c| EquityPoint {
                        time: c.time,
                        value,
                    })
                })
                .collect();

            curves.push(NamedEquityCurve {
                name: strategy.clone(),
                color: colors[i % colors.len()].to_string(),
                data,
            });
        }
    }

    Ok(curves)
}

/// Get trade markers for a result.
#[tauri::command]
pub fn get_trades(
    state: State<'_, AppState>,
    result_id: String,
) -> Result<Vec<TradeMarker>, GuiError> {
    // For now, return empty since we don't have trade data in ResultRow
    // TODO: Add trades to ResultRow when backtest is fully implemented
    let results_state = state.get_results_state();

    // Verify result exists
    if !results_state.results.iter().any(|r| r.id == result_id) {
        return Err(GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        });
    }

    // Placeholder: return empty trades list
    // Real implementation would extract from backtest results
    Ok(vec![])
}

/// Get complete chart data based on current state.
#[tauri::command]
pub fn get_chart_data(state: State<'_, AppState>) -> Result<ChartData, GuiError> {
    let chart_state = state.get_chart_state();
    let results_state = state.get_results_state();

    debug!(
        mode = ?chart_state.mode,
        symbol = ?chart_state.symbol,
        strategy = ?chart_state.strategy,
        config_id = ?chart_state.config_id,
        results_count = results_state.results.len(),
        "Getting chart data"
    );

    match chart_state.mode {
        ChartMode::Candlestick => {
            let symbol = chart_state.symbol.clone().ok_or_else(|| {
                warn!("Candlestick mode requested but no symbol selected");
                GuiError::InvalidInput {
                    message: "No symbol selected for candlestick chart".to_string(),
                }
            })?;

            debug!(symbol = %symbol, "Fetching candlestick data");
            let candles = get_candle_data(state.clone(), symbol)?;

            Ok(ChartData {
                candles: Some(candles),
                equity: None,
                curves: None,
                drawdown: None,
                trades: None,
            })
        }
        ChartMode::Equity => {
            debug!("Equity mode: searching for matching result");
            // Find result matching symbol/strategy/config
            let result = results_state
                .results
                .iter()
                .find(|r| {
                    chart_state
                        .symbol
                        .as_ref()
                        .map(|s| &r.symbol == s)
                        .unwrap_or(true)
                        && chart_state
                            .strategy
                            .as_ref()
                            .map(|s| &r.strategy == s)
                            .unwrap_or(true)
                        && chart_state
                            .config_id
                            .as_ref()
                            .map(|c| &r.config_id == c)
                            .unwrap_or(true)
                })
                .or_else(|| {
                    // Fall back to selected result
                    trace!(selected_id = ?results_state.selected_id, "No direct match, falling back to selected result");
                    results_state
                        .selected_id
                        .as_ref()
                        .and_then(|id| results_state.results.iter().find(|r| &r.id == id))
                })
                .ok_or_else(|| {
                    warn!(
                        symbol = ?chart_state.symbol,
                        strategy = ?chart_state.strategy,
                        config_id = ?chart_state.config_id,
                        selected_id = ?results_state.selected_id,
                        results_count = results_state.results.len(),
                        "No matching result found for equity chart"
                    );
                    GuiError::NotFound {
                        resource: "No matching result for equity chart".to_string(),
                    }
                })?;

            debug!(
                result_id = %result.id,
                result_symbol = %result.symbol,
                result_strategy = %result.strategy,
                "Found matching result for equity chart"
            );

            let equity = get_equity_curve(state.clone(), result.id.clone())?;

            let drawdown = if chart_state.overlays.drawdown {
                Some(get_drawdown_curve(state.clone(), result.id.clone())?)
            } else {
                None
            };

            let trades = if chart_state.overlays.trades {
                Some(get_trades(state.clone(), result.id.clone())?)
            } else {
                None
            };

            Ok(ChartData {
                candles: None,
                equity: Some(equity),
                curves: None,
                drawdown,
                trades,
            })
        }
        ChartMode::MultiTicker => {
            let curves = get_multi_ticker_curves(state.clone())?;
            Ok(ChartData {
                candles: None,
                equity: None,
                curves: Some(curves),
                drawdown: None,
                trades: None,
            })
        }
        ChartMode::Portfolio => {
            let equity = get_portfolio_curve(state.clone())?;
            Ok(ChartData {
                candles: None,
                equity: Some(equity),
                curves: None,
                drawdown: None,
                trades: None,
            })
        }
        ChartMode::StrategyComparison => {
            let curves = get_strategy_curves(state.clone())?;
            Ok(ChartData {
                candles: None,
                equity: None,
                curves: Some(curves),
                drawdown: None,
                trades: None,
            })
        }
    }
}
