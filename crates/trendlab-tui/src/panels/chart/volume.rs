//! Volume bars subplot rendering

use ratatui::{
    layout::Rect,
    style::Style,
    symbols::Marker,
    text::Span,
    widgets::{
        canvas::{Canvas, Rectangle},
        Block, Borders,
    },
    Frame,
};

use crate::ui::colors;
use trendlab_engine::app::App;

/// Draw volume bars subplot
pub fn draw_volume_bars(f: &mut Frame, app: &App, area: Rect) {
    if app.chart.candle_data.is_empty() {
        return;
    }

    let candles = &app.chart.candle_data;
    let total_bars = candles.len();

    // Apply same zoom and scroll as candlestick chart
    let visible_count = ((total_bars as f64 / app.chart.zoom_level) as usize).max(10);
    let max_scroll = total_bars.saturating_sub(visible_count);
    let scroll_offset = app.chart.scroll_offset.min(max_scroll);
    let start_idx = scroll_offset;
    let end_idx = (start_idx + visible_count).min(total_bars);

    let visible_candles: Vec<_> = candles[start_idx..end_idx].iter().collect();

    if visible_candles.is_empty() {
        return;
    }

    // Find max volume for scaling
    let max_volume = visible_candles
        .iter()
        .map(|c| c.volume)
        .fold(0.0_f64, f64::max)
        .max(1.0);

    let visible_count_f64 = visible_candles.len() as f64;

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors::BORDER))
                .title(Span::styled(
                    " Volume ",
                    Style::default().fg(colors::FG_DARK),
                )),
        )
        .x_bounds([0.0, visible_count_f64])
        .y_bounds([0.0, max_volume])
        .marker(Marker::Block)
        .paint(|ctx| {
            for (i, candle) in visible_candles.iter().enumerate() {
                let x = i as f64 + 0.15;
                let is_bullish = candle.close >= candle.open;

                let color = if is_bullish {
                    colors::VOLUME_UP
                } else {
                    colors::VOLUME_DOWN
                };

                // Draw volume bar
                ctx.draw(&Rectangle {
                    x,
                    y: 0.0,
                    width: 0.7,
                    height: candle.volume,
                    color,
                });
            }
        });

    f.render_widget(canvas, area);
}
