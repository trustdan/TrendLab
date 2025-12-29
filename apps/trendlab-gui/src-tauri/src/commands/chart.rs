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
    use trendlab_engine::app::ChartViewMode as EngineMode;

    // Read engine state fields individually (ChartState doesn't implement Clone)
    let engine = state.engine_read();
    let view_mode = engine.chart.view_mode;
    let candle_symbol = engine.chart.candle_symbol.clone();
    let show_drawdown = engine.chart.show_drawdown;
    let show_volume = engine.chart.show_volume;
    let show_crosshair = engine.chart.show_crosshair;
    let selected_index = engine.chart.selected_result_index;

    // Get config_id from selected result if available
    let config_id = selected_index.and_then(|idx| {
        engine.results.results.get(idx).map(|r| r.config_id.id())
    });
    drop(engine);

    let mode = match view_mode {
        EngineMode::Candlestick => ChartMode::Candlestick,
        EngineMode::Single => ChartMode::Equity,
        EngineMode::MultiTicker => ChartMode::MultiTicker,
        EngineMode::Portfolio => ChartMode::Portfolio,
        EngineMode::StrategyComparison => ChartMode::StrategyComparison,
        EngineMode::PerTickerBestStrategy => ChartMode::MultiTicker, // Map to closest equivalent
    };

    ChartState {
        mode,
        symbol: candle_symbol,
        strategy: None, // Not stored in engine's ChartState
        config_id,
        overlays: ChartOverlays {
            drawdown: show_drawdown,
            volume: show_volume,
            trades: true,
            crosshair: show_crosshair,
        },
    }
}

/// Set chart mode.
#[tauri::command]
pub fn set_chart_mode(state: State<'_, AppState>, mode: ChartMode) {
    use trendlab_engine::app::ChartViewMode as EngineMode;
    let engine_mode = match mode {
        ChartMode::Candlestick => EngineMode::Candlestick,
        ChartMode::Equity => EngineMode::Single,
        ChartMode::MultiTicker => EngineMode::MultiTicker,
        ChartMode::Portfolio => EngineMode::Portfolio,
        ChartMode::StrategyComparison => EngineMode::StrategyComparison,
    };
    state.set_chart_mode(engine_mode);
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
    get_chart_state(state).overlays
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
                                            d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()
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
    let engine = state.engine_read();

    let result = engine
        .results
        .results
        .iter()
        .find(|r| r.config_id.id() == result_id)
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })?;

    // Map engine's equity points to GUI's equity points
    // Engine's EquityPoint has ts (DateTime) and equity (f64)
    let equity: Vec<EquityPoint> = result
        .backtest_result
        .equity
        .iter()
        .map(|p| EquityPoint {
            time: p.ts.timestamp(),
            value: p.equity,
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
    let engine = state.engine_read();

    let result = engine
        .results
        .results
        .iter()
        .find(|r| r.config_id.id() == result_id)
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })?;

    // Calculate drawdown from equity curve
    let mut peak = f64::NEG_INFINITY;
    let drawdown: Vec<DrawdownPoint> = result
        .backtest_result
        .equity
        .iter()
        .map(|p| {
            peak = peak.max(p.equity);
            let dd = if peak > 0.0 {
                (p.equity - peak) / peak
            } else {
                0.0
            };
            DrawdownPoint {
                time: p.ts.timestamp(),
                drawdown: dd,
            }
        })
        .collect();

    Ok(drawdown)
}

/// Get multiple equity curves for comparison.
#[tauri::command]
pub fn get_multi_ticker_curves(
    state: State<'_, AppState>,
) -> Result<Vec<NamedEquityCurve>, GuiError> {
    let engine = state.engine_read();

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

    // Use multi_sweep_result if available (has per-symbol breakdown)
    if let Some(ref multi) = engine.results.multi_sweep_result {
        for (i, (symbol, sweep_result)) in multi.symbol_results.iter().enumerate() {
            // Get best result by Sharpe for this symbol
            if let Some(best) = sweep_result.config_results.iter().max_by(|a, b| {
                a.metrics.sharpe.partial_cmp(&b.metrics.sharpe).unwrap_or(std::cmp::Ordering::Equal)
            }) {
                let data: Vec<EquityPoint> = best
                    .backtest_result
                    .equity
                    .iter()
                    .map(|p| EquityPoint {
                        time: p.ts.timestamp(),
                        value: p.equity,
                    })
                    .collect();

                curves.push(NamedEquityCurve {
                    name: symbol.clone(),
                    color: colors[i % colors.len()].to_string(),
                    data,
                });
            }
        }
    } else {
        // Fall back to ticker summaries if available
        for (i, summary) in engine.results.ticker_summaries.iter().enumerate() {
            // Find the result matching this ticker's best config
            if let Some(result) = engine.results.results.iter().find(|r| {
                r.config_id.entry_lookback == summary.best_config_entry
                    && r.config_id.exit_lookback == summary.best_config_exit
            }) {
                let data: Vec<EquityPoint> = result
                    .backtest_result
                    .equity
                    .iter()
                    .map(|p| EquityPoint {
                        time: p.ts.timestamp(),
                        value: p.equity,
                    })
                    .collect();

                curves.push(NamedEquityCurve {
                    name: summary.symbol.clone(),
                    color: colors[i % colors.len()].to_string(),
                    data,
                });
            }
        }
    }

    Ok(curves)
}

/// Get portfolio (combined) equity curve.
#[tauri::command]
pub fn get_portfolio_curve(state: State<'_, AppState>) -> Result<Vec<EquityPoint>, GuiError> {
    let engine = state.engine_read();

    // Collect best results per symbol from multi_sweep_result
    let mut best_results: Vec<&trendlab_core::SweepConfigResult> = Vec::new();

    if let Some(ref multi) = engine.results.multi_sweep_result {
        for (_symbol, sweep_result) in &multi.symbol_results {
            if let Some(best) = sweep_result.config_results.iter().max_by(|a, b| {
                a.metrics.sharpe.partial_cmp(&b.metrics.sharpe).unwrap_or(std::cmp::Ordering::Equal)
            }) {
                best_results.push(best);
            }
        }
    } else if !engine.results.results.is_empty() {
        // Fallback: use all results as a single "portfolio"
        if let Some(best) = engine.results.results.iter().max_by(|a, b| {
            a.metrics.sharpe.partial_cmp(&b.metrics.sharpe).unwrap_or(std::cmp::Ordering::Equal)
        }) {
            best_results.push(best);
        }
    }

    if best_results.is_empty() {
        return Err(GuiError::NotFound {
            resource: "No results for portfolio".to_string(),
        });
    }

    // Find the result with the most data points
    let max_len = best_results
        .iter()
        .map(|r| r.backtest_result.equity.len())
        .max()
        .unwrap_or(0);

    if max_len == 0 {
        return Err(GuiError::NotFound {
            resource: "No equity data for portfolio".to_string(),
        });
    }

    // Get timestamps from first result
    let first_result = best_results[0];
    let n_curves = best_results.len() as f64;
    let mut portfolio: Vec<EquityPoint> = Vec::new();

    for i in 0..max_len {
        let sum: f64 = best_results
            .iter()
            .filter_map(|r| r.backtest_result.equity.get(i).map(|p| p.equity))
            .sum();

        // Average (equal weight)
        let value = sum / n_curves;

        if let Some(p) = first_result.backtest_result.equity.get(i) {
            portfolio.push(EquityPoint {
                time: p.ts.timestamp(),
                value,
            });
        }
    }

    Ok(portfolio)
}

/// Get strategy comparison curves.
#[tauri::command]
pub fn get_strategy_curves(state: State<'_, AppState>) -> Result<Vec<NamedEquityCurve>, GuiError> {
    let engine = state.engine_read();

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

    // Use multi_strategy_result if available
    if let Some(ref multi) = engine.results.multi_strategy_result {
        // Collect best result per strategy type
        let mut best_by_strategy: std::collections::HashMap<String, &trendlab_core::SweepConfigResult> =
            std::collections::HashMap::new();

        for ((_symbol, strategy_type), sweep_result) in &multi.results {
            let strategy_name = strategy_type.name().to_string();
            if let Some(best) = sweep_result.config_results.iter().max_by(|a, b| {
                a.metrics.sharpe.partial_cmp(&b.metrics.sharpe).unwrap_or(std::cmp::Ordering::Equal)
            }) {
                if let Some(existing) = best_by_strategy.get(&strategy_name) {
                    if best.metrics.sharpe > existing.metrics.sharpe {
                        best_by_strategy.insert(strategy_name, best);
                    }
                } else {
                    best_by_strategy.insert(strategy_name, best);
                }
            }
        }

        for (i, (strategy, result)) in best_by_strategy.iter().enumerate() {
            let data: Vec<EquityPoint> = result
                .backtest_result
                .equity
                .iter()
                .map(|p| EquityPoint {
                    time: p.ts.timestamp(),
                    value: p.equity,
                })
                .collect();

            curves.push(NamedEquityCurve {
                name: strategy.clone(),
                color: colors[i % colors.len()].to_string(),
                data,
            });
        }
    } else {
        // Fallback: use current results (single strategy likely)
        // Group by config pattern (since we don't have strategy name directly)
        if let Some(best) = engine.results.results.iter().max_by(|a, b| {
            a.metrics.sharpe.partial_cmp(&b.metrics.sharpe).unwrap_or(std::cmp::Ordering::Equal)
        }) {
            let data: Vec<EquityPoint> = best
                .backtest_result
                .equity
                .iter()
                .map(|p| EquityPoint {
                    time: p.ts.timestamp(),
                    value: p.equity,
                })
                .collect();

            curves.push(NamedEquityCurve {
                name: best.config_id.id(),
                color: colors[0].to_string(),
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
    let engine = state.engine_read();

    let result = engine
        .results
        .results
        .iter()
        .find(|r| r.config_id.id() == result_id)
        .ok_or_else(|| GuiError::NotFound {
            resource: format!("Result with id '{}'", result_id),
        })?;

    // Convert trades from backtest result to GUI markers
    // Trade has: entry: Fill, exit: Fill, gross_pnl, net_pnl, direction
    // Fill has: ts, side, qty, price, fees, raw_price, atr_at_fill
    let trades: Vec<TradeMarker> = result
        .backtest_result
        .trades
        .iter()
        .flat_map(|t| {
            let direction = format!("{:?}", t.direction).to_lowercase();
            vec![
                TradeMarker {
                    time: t.entry.ts.timestamp(),
                    price: t.entry.price,
                    marker_type: "entry".to_string(),
                    direction: direction.clone(),
                    pnl: None,
                    text: format!("Entry @ {:.2}", t.entry.price),
                },
                TradeMarker {
                    time: t.exit.ts.timestamp(),
                    price: t.exit.price,
                    marker_type: "exit".to_string(),
                    direction,
                    pnl: Some(t.net_pnl),
                    text: format!("Exit @ {:.2} (P&L: {:.2})", t.exit.price, t.net_pnl),
                },
            ]
        })
        .collect();

    Ok(trades)
}

/// Get complete chart data based on current state.
#[tauri::command]
pub fn get_chart_data(state: State<'_, AppState>) -> Result<ChartData, GuiError> {
    // Get GUI chart state (converted from engine's ChartState)
    let gui_chart_state = get_chart_state(state.clone());

    // Get engine state for results access
    let engine = state.engine_read();
    let results_count = engine.results.results.len();
    drop(engine); // Release lock early

    debug!(
        mode = ?gui_chart_state.mode,
        symbol = ?gui_chart_state.symbol,
        strategy = ?gui_chart_state.strategy,
        config_id = ?gui_chart_state.config_id,
        results_count = results_count,
        "Getting chart data"
    );

    match gui_chart_state.mode {
        ChartMode::Candlestick => {
            let symbol = gui_chart_state.symbol.clone().ok_or_else(|| {
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

            // Find result matching config_id, or fall back to selected result
            let engine = state.engine_read();
            let result_id = if let Some(ref config_id) = gui_chart_state.config_id {
                // Match by config_id string
                engine
                    .results
                    .results
                    .iter()
                    .find(|r| r.config_id.id() == *config_id)
                    .map(|r| r.config_id.id())
            } else {
                // Fall back to selected result by index
                engine
                    .results
                    .results
                    .get(engine.results.selected_index)
                    .map(|r| r.config_id.id())
            };
            drop(engine);

            let result_id = result_id.ok_or_else(|| {
                warn!(
                    config_id = ?gui_chart_state.config_id,
                    results_count = results_count,
                    "No matching result found for equity chart"
                );
                GuiError::NotFound {
                    resource: "No matching result for equity chart".to_string(),
                }
            })?;

            debug!(result_id = %result_id, "Found matching result for equity chart");

            let equity = get_equity_curve(state.clone(), result_id.clone())?;

            let drawdown = if gui_chart_state.overlays.drawdown {
                Some(get_drawdown_curve(state.clone(), result_id.clone())?)
            } else {
                None
            };

            let trades = if gui_chart_state.overlays.trades {
                Some(get_trades(state.clone(), result_id.clone())?)
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
