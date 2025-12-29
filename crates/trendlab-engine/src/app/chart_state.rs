//! Chart panel state and related types.

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use trendlab_core::{Bar, Metrics, StrategyTypeId};

/// Renderer-agnostic rectangle for chart bounds
/// (compatible with ratatui::layout::Rect but independent)
#[derive(Debug, Clone, Copy, Default)]
pub struct ChartRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl ChartRect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self { x, y, width, height }
    }
}

/// Chart view mode for multi-curve display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartViewMode {
    /// Single equity curve (original behavior)
    #[default]
    Single,
    /// Multiple per-ticker equity curves overlaid
    MultiTicker,
    /// Aggregated portfolio equity curve
    Portfolio,
    /// Strategy comparison: overlay best configs per strategy
    StrategyComparison,
    /// Per-ticker best strategy: each ticker's best strategy
    PerTickerBestStrategy,
    /// OHLC candlestick chart
    Candlestick,
}

/// A single candlestick for rendering
#[derive(Debug, Clone)]
pub struct CandleData {
    /// Index in the data series (for X position)
    #[allow(dead_code)]
    pub index: usize,
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
    /// Date for label display
    pub date: String,
}

#[allow(dead_code)]
impl CandleData {
    /// Returns true if this is a bullish (up) candle
    pub fn is_bullish(&self) -> bool {
        self.close >= self.open
    }

    /// Returns the body top (higher of open/close)
    pub fn body_top(&self) -> f64 {
        self.open.max(self.close)
    }

    /// Returns the body bottom (lower of open/close)
    pub fn body_bottom(&self) -> f64 {
        self.open.min(self.close)
    }
}

/// Cursor state for crosshair and tooltip
#[derive(Debug, Clone, Default)]
pub struct CursorState {
    /// Raw terminal coordinates (column, row)
    pub terminal_pos: Option<(u16, u16)>,
    /// Chart-relative coordinates (x, y within chart area)
    pub chart_pos: Option<(u16, u16)>,
    /// Data point index under cursor (for tooltip)
    pub data_index: Option<usize>,
    /// Whether cursor is within chart bounds
    pub in_chart: bool,
}

/// Animation state for smooth transitions
#[derive(Debug, Clone)]
pub struct AnimationState {
    /// Target zoom level (animate toward this)
    pub target_zoom: f64,
    /// Target scroll offset
    pub target_scroll: f64,
    /// Is animation active
    pub animating: bool,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            target_zoom: 1.0,
            target_scroll: 0.0,
            animating: false,
        }
    }
}

/// Per-ticker equity curve for multi-curve display
#[derive(Debug, Clone)]
pub struct TickerCurve {
    pub symbol: String,
    pub equity: Vec<f64>,
    pub dates: Vec<DateTime<Utc>>,
}

/// Per-strategy equity curve for strategy comparison view
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StrategyCurve {
    pub strategy_type: StrategyTypeId,
    pub config_display: String,
    pub equity: Vec<f64>,
    pub dates: Vec<DateTime<Utc>>,
    pub metrics: Metrics,
}

/// Per-ticker best strategy result for best strategy view
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TickerBestStrategy {
    pub symbol: String,
    pub strategy_type: StrategyTypeId,
    pub config_display: String,
    pub equity: Vec<f64>,
    pub dates: Vec<DateTime<Utc>>,
    pub metrics: Metrics,
}

/// Winning configuration info for display and Pine export
#[derive(Debug, Clone, Default)]
pub struct WinningConfig {
    pub strategy_name: String,
    pub config_display: String,
    pub symbol: Option<String>,
}

/// Chart panel state
#[derive(Debug, Default)]
pub struct ChartState {
    pub equity_curve: Vec<f64>,
    pub equity_dates: Vec<DateTime<Utc>>,
    pub drawdown_curve: Vec<f64>,
    pub selected_result_index: Option<usize>,
    pub zoom_level: f64,
    pub scroll_offset: usize,
    pub show_drawdown: bool,
    /// View mode for multi-curve display
    pub view_mode: ChartViewMode,
    /// Per-ticker equity curves (for MultiTicker mode)
    pub ticker_curves: Vec<TickerCurve>,
    /// Portfolio aggregate equity curve
    pub portfolio_curve: Vec<f64>,
    /// Per-strategy equity curves (for StrategyComparison mode)
    pub strategy_curves: Vec<StrategyCurve>,
    /// Per-ticker best strategy results (for PerTickerBestStrategy mode)
    pub ticker_best_strategies: Vec<TickerBestStrategy>,
    /// OHLC candle data for candlestick view
    pub candle_data: Vec<CandleData>,
    /// Currently selected symbol for candlestick view
    pub candle_symbol: Option<String>,
    /// Show volume subplot
    pub show_volume: bool,
    /// Cursor state for crosshair
    pub cursor: CursorState,
    /// Animation state for smooth zoom/pan
    pub animation: AnimationState,
    /// Crosshair visibility
    pub show_crosshair: bool,
    /// Last chart rendering area (for hit-testing)
    /// Using Mutex for thread-safety in GUI context
    pub chart_area: Mutex<Option<ChartRect>>,
    /// Winning config display string (for Pine Script export)
    pub winning_config: Option<WinningConfig>,
}

impl ChartState {
    /// Check if we have multi-curve data
    pub fn has_multi_curves(&self) -> bool {
        !self.ticker_curves.is_empty()
    }

    /// Check if we have strategy comparison data
    pub fn has_strategy_curves(&self) -> bool {
        !self.strategy_curves.is_empty()
    }

    /// Check if we have candlestick data
    pub fn has_candle_data(&self) -> bool {
        !self.candle_data.is_empty()
    }

    /// Cycle through chart view modes
    pub fn cycle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ChartViewMode::Single => {
                if self.has_candle_data() {
                    ChartViewMode::Candlestick
                } else if self.has_multi_curves() {
                    ChartViewMode::MultiTicker
                } else if self.has_strategy_curves() {
                    ChartViewMode::StrategyComparison
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::Candlestick => {
                if self.has_multi_curves() {
                    ChartViewMode::MultiTicker
                } else if self.has_strategy_curves() {
                    ChartViewMode::StrategyComparison
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::MultiTicker => ChartViewMode::Portfolio,
            ChartViewMode::Portfolio => {
                if self.has_strategy_curves() {
                    ChartViewMode::StrategyComparison
                } else if self.has_candle_data() {
                    ChartViewMode::Candlestick
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::StrategyComparison => {
                if !self.ticker_best_strategies.is_empty() {
                    ChartViewMode::PerTickerBestStrategy
                } else if self.has_candle_data() {
                    ChartViewMode::Candlestick
                } else {
                    ChartViewMode::Single
                }
            }
            ChartViewMode::PerTickerBestStrategy => {
                if self.has_candle_data() {
                    ChartViewMode::Candlestick
                } else {
                    ChartViewMode::Single
                }
            }
        };
    }

    /// Get view mode name
    pub fn view_mode_name(&self) -> &'static str {
        match self.view_mode {
            ChartViewMode::Single => "Single",
            ChartViewMode::Candlestick => "Candlestick",
            ChartViewMode::MultiTicker => "Multi-Ticker",
            ChartViewMode::Portfolio => "Portfolio",
            ChartViewMode::StrategyComparison => "Strategy Comparison",
            ChartViewMode::PerTickerBestStrategy => "Per-Ticker Best",
        }
    }

    /// Update candle data from bars
    pub fn update_candle_data(&mut self, bars: &[Bar], symbol: &str) {
        self.candle_data = bars
            .iter()
            .enumerate()
            .map(|(i, bar)| CandleData {
                index: i,
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
                date: bar.ts.format("%Y-%m-%d").to_string(),
            })
            .collect();
        self.candle_symbol = Some(symbol.to_string());
    }

    /// Animated zoom in
    pub fn zoom_in_animated(&mut self) {
        self.animation.target_zoom = (self.animation.target_zoom * 1.2).min(4.0);
        self.animation.animating = true;
    }

    /// Animated zoom out
    pub fn zoom_out_animated(&mut self) {
        self.animation.target_zoom = (self.animation.target_zoom / 1.2).max(0.25);
        self.animation.animating = true;
    }
}
