//! Candlestick chart rendering with OHLC data

use ratatui::{
    layout::Rect,
    style::Style,
    symbols::Marker,
    text::Span,
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Rectangle},
        Axis, Block, Chart, Dataset, GraphType,
    },
    Frame,
};

use crate::ui::{colors, panel_block};
use trendlab_engine::app::{App, CandleData};

use super::empty_states::draw_no_candle_data;
use super::formatters::{calculate_price_bounds, format_price};

/// Draw OHLC candlestick chart using Canvas widget
pub fn draw_candlestick_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.candle_data.is_empty() {
        draw_no_candle_data(f, area, is_active);
        return;
    }

    let candles = &app.chart.candle_data;
    let total_bars = candles.len();

    // Apply zoom and scroll to get visible range
    let visible_count = ((total_bars as f64 / app.chart.zoom_level) as usize).max(10);
    let max_scroll = total_bars.saturating_sub(visible_count);
    let scroll_offset = app.chart.scroll_offset.min(max_scroll);
    let start_idx = scroll_offset;
    let end_idx = (start_idx + visible_count).min(total_bars);

    let visible_candles: Vec<&CandleData> = candles[start_idx..end_idx].iter().collect();

    if visible_candles.is_empty() {
        draw_no_candle_data(f, area, is_active);
        return;
    }

    // Calculate price bounds with padding
    let (y_min, y_max) = calculate_price_bounds(
        &visible_candles
            .iter()
            .map(|c| (*c).clone())
            .collect::<Vec<_>>(),
    );

    // Generate Y-axis labels
    let y_labels: Vec<Span> = (0..5)
        .map(|i| {
            let v = y_min + (y_max - y_min) * i as f64 / 4.0;
            Span::styled(format_price(v), Style::default().fg(colors::FG_DARK))
        })
        .collect();

    // Generate X-axis labels (dates)
    let x_labels: Vec<Span> = {
        let step = visible_candles.len() / 4;
        (0..=4)
            .map(|i| {
                let idx = (i * step).min(visible_candles.len().saturating_sub(1));
                if let Some(candle) = visible_candles.get(idx) {
                    // Show abbreviated date
                    let date = if candle.date.len() >= 10 {
                        &candle.date[5..10] // MM-DD
                    } else {
                        &candle.date
                    };
                    Span::styled(date.to_string(), Style::default().fg(colors::FG_DARK))
                } else {
                    Span::raw("")
                }
            })
            .collect()
    };

    let visible_count_f64 = visible_candles.len() as f64;

    // Create the canvas with candlesticks
    let canvas = Canvas::default()
        .block(panel_block("Candlestick Chart", is_active))
        .x_bounds([0.0, visible_count_f64])
        .y_bounds([y_min, y_max])
        .marker(Marker::Braille)
        .paint(|ctx| {
            // Draw grid lines first (behind candles)
            draw_grid_lines(ctx, visible_count_f64, y_min, y_max, 5);

            // Draw each candlestick
            for (i, candle) in visible_candles.iter().enumerate() {
                let x = i as f64 + 0.5; // Center of bar position
                let is_bullish = candle.close >= candle.open;

                let color = if is_bullish {
                    colors::GREEN
                } else {
                    colors::RED
                };

                // Draw wick (high to low line)
                ctx.draw(&CanvasLine {
                    x1: x,
                    y1: candle.low,
                    x2: x,
                    y2: candle.high,
                    color,
                });

                // Draw body (rectangle from open to close)
                let body_bottom = candle.open.min(candle.close);
                let body_top = candle.open.max(candle.close);
                let body_height = (body_top - body_bottom).max(0.001); // Ensure visible

                // Use rectangle for body
                ctx.draw(&Rectangle {
                    x: x - 0.3,
                    y: body_bottom,
                    width: 0.6,
                    height: body_height,
                    color,
                });
            }
        });

    // Render the canvas
    f.render_widget(canvas, area);

    // Overlay axis labels using a Chart widget (invisible data, just for axes)
    let empty_data: Vec<(f64, f64)> = vec![];
    let _axis_chart = Chart::new(vec![Dataset::default()
        .data(&empty_data)
        .graph_type(GraphType::Line)])
    .block(Block::default())
    .x_axis(
        Axis::default()
            .bounds([0.0, visible_count_f64])
            .labels(x_labels)
            .style(Style::default().fg(colors::FG_DARK)),
    )
    .y_axis(
        Axis::default()
            .bounds([y_min, y_max])
            .labels(y_labels)
            .style(Style::default().fg(colors::FG_DARK)),
    );

    // This is a workaround - we use a transparent overlay
    // In practice the Canvas already has the visual, just not axis labels
    // For now, just render the canvas above
}

/// Helper to draw grid lines on canvas
pub fn draw_grid_lines(
    ctx: &mut ratatui::widgets::canvas::Context,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    num_lines: usize,
) {
    let grid_color = colors::GRID;

    // Horizontal grid lines
    for i in 0..=num_lines {
        let y = y_min + (y_max - y_min) * i as f64 / num_lines as f64;
        ctx.draw(&CanvasLine {
            x1: 0.0,
            y1: y,
            x2: x_max,
            y2: y,
            color: grid_color,
        });
    }

    // Vertical grid lines (sparse)
    let x_step = (x_max / 4.0).max(1.0);
    let mut x = 0.0;
    while x <= x_max {
        ctx.draw(&CanvasLine {
            x1: x,
            y1: y_min,
            x2: x,
            y2: y_max,
            color: grid_color,
        });
        x += x_step;
    }
}
