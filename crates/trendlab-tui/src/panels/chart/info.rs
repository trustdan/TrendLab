//! Chart info panel rendering (statistics display)

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::ui::{colors, panel_block};
use trendlab_engine::app::App;

/// Draw the chart info/statistics panel at the bottom
pub fn draw_chart_info(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    // Get actual metrics if we have a selected result
    let (cagr, sharpe, max_dd, final_val, num_trades, win_rate) =
        if let Some(idx) = app.chart.selected_result_index {
            if let Some(result) = app.results.results.get(idx) {
                let m = &result.metrics;
                (
                    m.cagr * 100.0,
                    m.sharpe,
                    m.max_drawdown * 100.0,
                    app.chart.equity_curve.last().copied().unwrap_or(100_000.0),
                    m.num_trades,
                    m.win_rate * 100.0,
                )
            } else {
                (0.0, 0.0, 0.0, 0.0, 0, 0.0)
            }
        } else if !app.chart.equity_curve.is_empty() {
            // Calculate from equity curve if no result selected
            let final_val = app.chart.equity_curve.last().copied().unwrap_or(100_000.0);
            let initial = app.chart.equity_curve.first().copied().unwrap_or(100_000.0);
            let max_dd = app
                .chart
                .drawdown_curve
                .iter()
                .filter(|v| v.is_finite())
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .copied()
                .unwrap_or(0.0);
            let years = app.chart.equity_curve.len() as f64 / 252.0;
            let cagr = if years > 0.0 && initial > 0.0 {
                ((final_val / initial).powf(1.0 / years) - 1.0) * 100.0
            } else {
                0.0
            };
            (cagr, 0.0, max_dd, final_val, 0, 0.0)
        } else if !app.chart.strategy_curves.is_empty() {
            // Use best (first) strategy curve metrics from YOLO mode
            let best = &app.chart.strategy_curves[0];
            let m = &best.metrics;
            let final_val = best.equity.last().copied().unwrap_or(100_000.0);
            (
                m.cagr * 100.0,
                m.sharpe,
                m.max_drawdown * 100.0,
                final_val,
                m.num_trades,
                m.win_rate * 100.0,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0, 0, 0.0)
        };

    let _view_mode = app.chart.view_mode_name();

    let lines = if app.chart.equity_curve.is_empty()
        && app.chart.ticker_curves.is_empty()
        && app.chart.portfolio_curve.is_empty()
        && app.chart.strategy_curves.is_empty()
    {
        vec![
            Line::from(vec![Span::styled(
                "Select a result to view statistics",
                Style::default().fg(colors::FG_DARK),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("'d': ", Style::default().fg(colors::CYAN)),
                Span::styled("DD  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'m': ", Style::default().fg(colors::CYAN)),
                Span::styled("Mode  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'v': ", Style::default().fg(colors::CYAN)),
                Span::styled("Vol  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'c': ", Style::default().fg(colors::CYAN)),
                Span::styled("Cross  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("\u{2191}\u{2193}: ", Style::default().fg(colors::CYAN)),
                Span::styled("Zoom", Style::default().fg(colors::FG_DARK)),
            ]),
        ]
    } else {
        // Build winning config line if available
        let config_line = if let Some(ref wc) = app.chart.winning_config {
            let symbol_part = wc
                .symbol
                .as_ref()
                .map(|s| format!(" [{}]", s))
                .unwrap_or_default();
            Line::from(vec![
                Span::styled("\u{1f3c6} ", Style::default().fg(colors::YELLOW)), // Trophy emoji
                Span::styled(
                    wc.strategy_name.clone(),
                    Style::default()
                        .fg(colors::CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    wc.config_display.clone(),
                    Style::default().fg(colors::GREEN),
                ),
                Span::styled(symbol_part, Style::default().fg(colors::YELLOW)),
            ])
        } else {
            Line::from(vec![
                Span::styled("Final: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("${:.0}", final_val),
                    Style::default()
                        .fg(colors::CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   CAGR: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.1}%", cagr),
                    if cagr > 0.0 {
                        Style::default().fg(colors::GREEN)
                    } else {
                        Style::default().fg(colors::RED)
                    },
                ),
                Span::styled("   Sharpe: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.2}", sharpe),
                    if sharpe > 1.0 {
                        Style::default().fg(colors::GREEN)
                    } else if sharpe > 0.5 {
                        Style::default().fg(colors::YELLOW)
                    } else {
                        Style::default().fg(colors::ORANGE)
                    },
                ),
                Span::styled("   MaxDD: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(format!("{:.1}%", max_dd), Style::default().fg(colors::RED)),
            ])
        };

        vec![
            config_line,
            Line::from(vec![
                Span::styled("CAGR: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.1}%", cagr),
                    if cagr > 0.0 {
                        Style::default().fg(colors::GREEN)
                    } else {
                        Style::default().fg(colors::RED)
                    },
                ),
                Span::styled("  Sharpe: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.2}", sharpe),
                    if sharpe > 1.0 {
                        Style::default().fg(colors::GREEN)
                    } else if sharpe > 0.5 {
                        Style::default().fg(colors::YELLOW)
                    } else {
                        Style::default().fg(colors::ORANGE)
                    },
                ),
                Span::styled("  MaxDD: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(format!("{:.1}%", max_dd), Style::default().fg(colors::RED)),
                Span::styled("  Trades: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{}", num_trades),
                    Style::default().fg(colors::YELLOW),
                ),
                Span::styled("  Win: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.0}%", win_rate),
                    if win_rate > 50.0 {
                        Style::default().fg(colors::GREEN)
                    } else {
                        Style::default().fg(colors::ORANGE)
                    },
                ),
                if app.chart.show_drawdown {
                    Span::styled("  [DD]", Style::default().fg(colors::RED))
                } else {
                    Span::styled("", Style::default())
                },
            ]),
            Line::from(vec![
                Span::styled("'d': ", Style::default().fg(colors::CYAN)),
                Span::styled("DD  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'m': ", Style::default().fg(colors::CYAN)),
                Span::styled("Mode  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'v': ", Style::default().fg(colors::CYAN)),
                Span::styled("Vol  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'c': ", Style::default().fg(colors::CYAN)),
                Span::styled("Cross  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("\u{2191}\u{2193}: ", Style::default().fg(colors::CYAN)),
                Span::styled("Zoom  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("\u{2190}\u{2192}: ", Style::default().fg(colors::CYAN)),
                Span::styled("Scroll", Style::default().fg(colors::FG_DARK)),
            ]),
        ]
    };

    let para = Paragraph::new(lines).block(panel_block("Statistics", is_active));

    f.render_widget(para, area);
}
