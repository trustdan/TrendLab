//! Crosshair and tooltip overlays for chart interaction

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::ui::colors;
use trendlab_engine::app::{App, ChartViewMode};

use super::formatters::{format_price, format_value, format_volume, trend_symbol};

/// Draw crosshair at cursor position
pub fn draw_crosshair(f: &mut Frame, app: &App, area: Rect) {
    let Some((cursor_x, cursor_y)) = app.chart.cursor.terminal_pos else {
        return;
    };

    // Check if cursor is within chart area
    if cursor_x < area.x || cursor_x >= area.x + area.width {
        return;
    }
    if cursor_y < area.y || cursor_y >= area.y + area.height {
        return;
    }

    // Draw vertical line
    for y in area.y..area.y + area.height {
        if y != cursor_y {
            if let Some(buf_cell) = f.buffer_mut().cell_mut((cursor_x, y)) {
                buf_cell.set_char('│');
                buf_cell.set_fg(colors::CROSSHAIR);
            }
        }
    }

    // Draw horizontal line
    for x in area.x..area.x + area.width {
        if x != cursor_x {
            if let Some(buf_cell) = f.buffer_mut().cell_mut((x, cursor_y)) {
                buf_cell.set_char('─');
                buf_cell.set_fg(colors::CROSSHAIR);
            }
        }
    }

    // Draw intersection
    if let Some(buf_cell) = f.buffer_mut().cell_mut((cursor_x, cursor_y)) {
        buf_cell.set_char('┼');
        buf_cell.set_fg(colors::CYAN);
    }
}

/// Draw data tooltip near cursor
pub fn draw_tooltip(f: &mut Frame, app: &App, area: Rect) {
    let Some(data_idx) = app.chart.cursor.data_index else {
        return;
    };

    // Build tooltip content based on view mode
    let lines: Vec<Line> = match app.chart.view_mode {
        ChartViewMode::Candlestick => {
            if let Some(candle) = app.chart.candle_data.get(data_idx) {
                let change = candle.close - candle.open;
                let change_pct = if candle.open > 0.0 {
                    (change / candle.open) * 100.0
                } else {
                    0.0
                };
                let trend = trend_symbol(change);
                let trend_color = if change >= 0.0 {
                    colors::GREEN
                } else {
                    colors::RED
                };

                vec![
                    Line::from(vec![Span::styled(
                        candle.date.clone(),
                        Style::default()
                            .fg(colors::CYAN)
                            .add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(vec![
                        Span::styled("O: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(format_price(candle.open), Style::default().fg(colors::FG)),
                    ]),
                    Line::from(vec![
                        Span::styled("H: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(format_price(candle.high), Style::default().fg(colors::FG)),
                    ]),
                    Line::from(vec![
                        Span::styled("L: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(format_price(candle.low), Style::default().fg(colors::FG)),
                    ]),
                    Line::from(vec![
                        Span::styled("C: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(format_price(candle.close), Style::default().fg(colors::FG)),
                        Span::raw(" "),
                        Span::styled(trend, Style::default().fg(trend_color)),
                        Span::styled(
                            format!(" {:.1}%", change_pct),
                            Style::default().fg(trend_color),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Vol: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(
                            format_volume(candle.volume),
                            Style::default().fg(colors::YELLOW),
                        ),
                    ]),
                ]
            } else {
                return;
            }
        }
        _ => {
            // Equity curve tooltip
            if let Some(&equity) = app.chart.equity_curve.get(data_idx) {
                let drawdown = app
                    .chart
                    .drawdown_curve
                    .get(data_idx)
                    .copied()
                    .unwrap_or(0.0);
                let initial = app.chart.equity_curve.first().copied().unwrap_or(100_000.0);
                let gain = equity - initial;
                let gain_pct = if initial > 0.0 {
                    (gain / initial) * 100.0
                } else {
                    0.0
                };
                let trend = trend_symbol(gain);
                let trend_color = if gain >= 0.0 {
                    colors::GREEN
                } else {
                    colors::RED
                };

                vec![
                    Line::from(vec![Span::styled(
                        format!("Day {}", data_idx),
                        Style::default()
                            .fg(colors::CYAN)
                            .add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(vec![
                        Span::styled("Equity: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(format_value(equity), Style::default().fg(colors::FG)),
                    ]),
                    Line::from(vec![
                        Span::styled("Gain: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(trend, Style::default().fg(trend_color)),
                        Span::styled(
                            format!(" {} ({:.1}%)", format_value(gain), gain_pct),
                            Style::default().fg(trend_color),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("DD: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(
                            format!("{:.1}%", drawdown),
                            Style::default().fg(colors::RED),
                        ),
                    ]),
                ]
            } else {
                return;
            }
        }
    };

    // Calculate tooltip position
    let Some((cursor_x, cursor_y)) = app.chart.cursor.terminal_pos else {
        return;
    };

    let tooltip_width: u16 = 20;
    let tooltip_height = lines.len() as u16 + 2; // +2 for borders

    // Position tooltip, flip if near edge
    let x = if cursor_x + tooltip_width + 2 > area.x + area.width {
        cursor_x.saturating_sub(tooltip_width + 1)
    } else {
        cursor_x + 2
    };

    let y = if cursor_y + tooltip_height > area.y + area.height {
        cursor_y.saturating_sub(tooltip_height)
    } else {
        cursor_y + 1
    };

    let tooltip_area = Rect::new(x, y, tooltip_width, tooltip_height);

    // Clear area and draw tooltip
    f.render_widget(Clear, tooltip_area);

    let tooltip = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::BORDER_ACTIVE))
            .style(Style::default().bg(colors::TOOLTIP_BG)),
    );

    f.render_widget(tooltip, tooltip_area);
}
