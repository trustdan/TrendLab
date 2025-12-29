//! Chart panel - visualize equity curves, candlesticks, and drawdowns with multi-curve support

mod candlestick;
mod colors;
mod empty_states;
mod equity;
mod formatters;
mod info;
mod overlays;
mod strategy;
mod volume;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use trendlab_engine::app::{App, ChartRect, ChartViewMode, Panel};

use candlestick::draw_candlestick_chart;
use equity::{draw_multi_ticker_chart, draw_portfolio_chart, draw_single_equity_chart};
use info::draw_chart_info;
use overlays::{draw_crosshair, draw_tooltip};
use strategy::{draw_per_ticker_best_chart, draw_strategy_comparison_chart};
use volume::draw_volume_bars;

/// Main chart panel draw function
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Chart;

    // Split into chart area and info panel
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(6)])
        .split(area);

    // Store chart area for mouse hit-testing (convert Rect to ChartRect)
    let r = chunks[0];
    if let Ok(mut guard) = app.chart.chart_area.lock() {
        *guard = Some(ChartRect::new(r.x, r.y, r.width, r.height));
    }

    // Determine if we need volume subplot
    let (chart_area, volume_area) = if app.chart.show_volume
        && (app.chart.view_mode == ChartViewMode::Candlestick || !app.chart.candle_data.is_empty())
    {
        let vol_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(chunks[0]);
        (vol_chunks[0], Some(vol_chunks[1]))
    } else {
        (chunks[0], None)
    };

    // Main chart based on view mode
    match app.chart.view_mode {
        ChartViewMode::Single => draw_single_equity_chart(f, app, chart_area, is_active),
        ChartViewMode::Candlestick => draw_candlestick_chart(f, app, chart_area, is_active),
        ChartViewMode::MultiTicker => draw_multi_ticker_chart(f, app, chart_area, is_active),
        ChartViewMode::Portfolio => draw_portfolio_chart(f, app, chart_area, is_active),
        ChartViewMode::StrategyComparison => {
            draw_strategy_comparison_chart(f, app, chart_area, is_active)
        }
        ChartViewMode::PerTickerBestStrategy => {
            draw_per_ticker_best_chart(f, app, chart_area, is_active)
        }
    }

    // Draw volume subplot if enabled
    if let Some(vol_area) = volume_area {
        draw_volume_bars(f, app, vol_area);
    }

    // Draw crosshair overlay if enabled
    if app.chart.show_crosshair && app.chart.cursor.in_chart {
        draw_crosshair(f, app, chart_area);
    }

    // Draw tooltip if cursor is over data
    if app.chart.cursor.in_chart && app.chart.cursor.data_index.is_some() {
        draw_tooltip(f, app, chart_area);
    }

    // Info panel
    draw_chart_info(f, app, chunks[1], is_active);
}
