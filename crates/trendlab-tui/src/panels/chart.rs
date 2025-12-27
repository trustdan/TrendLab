//! Chart panel - visualize equity curves and drawdowns with multi-curve support

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
    Frame,
};

use crate::app::{App, ChartViewMode, Panel};
use crate::ui::{colors, panel_block};

/// Color palette for multi-ticker curves
const CURVE_COLORS: &[Color] = &[
    Color::Rgb(46, 204, 113),   // Emerald green
    Color::Rgb(52, 152, 219),   // Blue
    Color::Rgb(155, 89, 182),   // Purple
    Color::Rgb(241, 196, 15),   // Yellow
    Color::Rgb(231, 76, 60),    // Red
    Color::Rgb(26, 188, 156),   // Turquoise
    Color::Rgb(230, 126, 34),   // Orange
    Color::Rgb(236, 240, 241),  // Light gray
    Color::Rgb(149, 165, 166),  // Gray
    Color::Rgb(46, 134, 193),   // Steel blue
    Color::Rgb(175, 122, 197),  // Amethyst
    Color::Rgb(244, 208, 63),   // Sunflower
];

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Chart;

    // Split into chart area and info panel
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(6)])
        .split(area);

    // Main chart based on view mode
    match app.chart.view_mode {
        ChartViewMode::Single => draw_single_equity_chart(f, app, chunks[0], is_active),
        ChartViewMode::MultiTicker => draw_multi_ticker_chart(f, app, chunks[0], is_active),
        ChartViewMode::Portfolio => draw_portfolio_chart(f, app, chunks[0], is_active),
    }

    // Info panel
    draw_chart_info(f, app, chunks[1], is_active);
}

/// Draw single equity curve (original behavior)
fn draw_single_equity_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    // Check if we have actual data
    if app.chart.equity_curve.is_empty() {
        draw_empty_chart(f, area, is_active);
        return;
    }

    // Prepare equity data
    let equity_data: Vec<(f64, f64)> = app
        .chart
        .equity_curve
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect();

    // Calculate bounds
    let x_max = equity_data.len() as f64;
    let y_min = equity_data
        .iter()
        .map(|(_, y)| *y)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(90000.0)
        * 0.95;
    let y_max = equity_data
        .iter()
        .map(|(_, y)| *y)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(150000.0)
        * 1.05;

    // Create datasets
    let mut datasets = vec![Dataset::default()
        .name("Equity")
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(colors::GREEN))
        .data(&equity_data)];

    // Add drawdown overlay if enabled
    let drawdown_data: Vec<(f64, f64)> =
        if app.chart.show_drawdown && !app.chart.drawdown_curve.is_empty() {
            // Scale drawdown to fit in the same chart (inverted, as percentage)
            let dd_scale = (y_max - y_min) / 50.0; // Max 50% drawdown fills chart
            app.chart
                .drawdown_curve
                .iter()
                .enumerate()
                .map(|(i, dd)| (i as f64, y_max + dd * dd_scale))
                .collect()
        } else {
            vec![]
        };

    if app.chart.show_drawdown && !drawdown_data.is_empty() {
        datasets.push(
            Dataset::default()
                .name("Drawdown")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(colors::RED))
                .data(&drawdown_data),
        );
    }

    let x_labels: Vec<Span> = (0..=4)
        .map(|i| Span::from(format!("{}", (i as f64 * x_max / 4.0) as i32)))
        .collect();

    // Format y-axis labels as currency
    let y_labels = vec![
        Span::from(format!("${:.0}k", y_min / 1000.0)),
        Span::from(format!("${:.0}k", (y_min + y_max) / 2000.0)),
        Span::from(format!("${:.0}k", y_max / 1000.0)),
    ];

    // Title includes result info if available
    let title = if let Some(idx) = app.chart.selected_result_index {
        if let Some(result) = app.results.results.get(idx) {
            format!(
                "Equity Curve - Entry={}, Exit={}",
                result.config_id.entry_lookback, result.config_id.exit_lookback
            )
        } else {
            "Equity Curve".to_string()
        }
    } else {
        "Equity Curve".to_string()
    };

    let chart = Chart::new(datasets)
        .block(panel_block(&title, is_active))
        .x_axis(
            Axis::default()
                .title(Span::styled("Days", Style::default().fg(colors::FG_DARK)))
                .style(Style::default().fg(colors::FG_DARK))
                .bounds([0.0, x_max])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title(Span::styled("Equity", Style::default().fg(colors::FG_DARK)))
                .style(Style::default().fg(colors::FG_DARK))
                .bounds([y_min, y_max])
                .labels(y_labels),
        );

    f.render_widget(chart, area);
}

/// Draw multi-ticker equity curves overlaid
fn draw_multi_ticker_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.ticker_curves.is_empty() {
        draw_no_multi_data(f, area, is_active);
        return;
    }

    // Prepare all curve data
    let mut all_data: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;
    let mut x_max = 0.0_f64;

    for curve in &app.chart.ticker_curves {
        let data: Vec<(f64, f64)> = curve
            .equity
            .iter()
            .enumerate()
            .map(|(i, v)| (i as f64, *v))
            .collect();

        if !data.is_empty() {
            x_max = x_max.max(data.len() as f64);
            for (_, y) in &data {
                y_min = y_min.min(*y);
                y_max = y_max.max(*y);
            }
        }
        all_data.push(data);
    }

    // Apply margins
    y_min *= 0.95;
    y_max *= 1.05;

    // Create datasets for each ticker
    let datasets: Vec<Dataset> = all_data
        .iter()
        .zip(app.chart.ticker_curves.iter())
        .enumerate()
        .map(|(i, (data, curve))| {
            let color = CURVE_COLORS[i % CURVE_COLORS.len()];
            Dataset::default()
                .name(curve.symbol.clone())
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(color))
                .data(data)
        })
        .collect();

    let x_labels: Vec<Span> = (0..=4)
        .map(|i| Span::from(format!("{}", (i as f64 * x_max / 4.0) as i32)))
        .collect();

    let y_labels = vec![
        Span::from(format!("${:.0}k", y_min / 1000.0)),
        Span::from(format!("${:.0}k", (y_min + y_max) / 2000.0)),
        Span::from(format!("${:.0}k", y_max / 1000.0)),
    ];

    let title = format!(
        "Multi-Ticker Equity ({} symbols)",
        app.chart.ticker_curves.len()
    );

    let chart = Chart::new(datasets)
        .block(panel_block(&title, is_active))
        .x_axis(
            Axis::default()
                .title(Span::styled("Days", Style::default().fg(colors::FG_DARK)))
                .style(Style::default().fg(colors::FG_DARK))
                .bounds([0.0, x_max])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title(Span::styled("Equity", Style::default().fg(colors::FG_DARK)))
                .style(Style::default().fg(colors::FG_DARK))
                .bounds([y_min, y_max])
                .labels(y_labels),
        );

    f.render_widget(chart, area);
}

/// Draw portfolio aggregate equity curve
fn draw_portfolio_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.portfolio_curve.is_empty() {
        draw_no_multi_data(f, area, is_active);
        return;
    }

    // Prepare portfolio data
    let portfolio_data: Vec<(f64, f64)> = app
        .chart
        .portfolio_curve
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect();

    // Calculate bounds
    let x_max = portfolio_data.len() as f64;
    let y_min = portfolio_data
        .iter()
        .map(|(_, y)| *y)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(90000.0)
        * 0.95;
    let y_max = portfolio_data
        .iter()
        .map(|(_, y)| *y)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(150000.0)
        * 1.05;

    let datasets = vec![Dataset::default()
        .name("Portfolio")
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(colors::CYAN))
        .data(&portfolio_data)];

    let x_labels: Vec<Span> = (0..=4)
        .map(|i| Span::from(format!("{}", (i as f64 * x_max / 4.0) as i32)))
        .collect();

    let y_labels = vec![
        Span::from(format!("${:.0}k", y_min / 1000.0)),
        Span::from(format!("${:.0}k", (y_min + y_max) / 2000.0)),
        Span::from(format!("${:.0}k", y_max / 1000.0)),
    ];

    let num_symbols = app.chart.ticker_curves.len();
    let title = format!("Portfolio Equity ({} symbols combined)", num_symbols);

    let chart = Chart::new(datasets)
        .block(panel_block(&title, is_active))
        .x_axis(
            Axis::default()
                .title(Span::styled("Days", Style::default().fg(colors::FG_DARK)))
                .style(Style::default().fg(colors::FG_DARK))
                .bounds([0.0, x_max])
                .labels(x_labels),
        )
        .y_axis(
            Axis::default()
                .title(Span::styled("Equity", Style::default().fg(colors::FG_DARK)))
                .style(Style::default().fg(colors::FG_DARK))
                .bounds([y_min, y_max])
                .labels(y_labels),
        );

    f.render_widget(chart, area);
}

fn draw_empty_chart(f: &mut Frame, area: Rect, is_active: bool) {
    let lines = vec![
        Line::from(""),
        Line::from(""),
        Line::from(vec![Span::styled(
            "No equity curve to display.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Run a sweep and select a result to view its chart.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("1. ", Style::default().fg(colors::CYAN)),
            Span::styled(
                "Load data in Data panel (Enter to load from cache, 'f' to fetch)",
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
        Line::from(vec![
            Span::styled("2. ", Style::default().fg(colors::CYAN)),
            Span::styled(
                "Run sweep in Sweep panel (Enter to start)",
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
        Line::from(vec![
            Span::styled("3. ", Style::default().fg(colors::CYAN)),
            Span::styled(
                "Select result in Results panel (Enter to view chart)",
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Equity Curve", is_active));

    f.render_widget(para, area);
}

fn draw_no_multi_data(f: &mut Frame, area: Rect, is_active: bool) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "No multi-ticker data available.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Select multiple tickers and run a sweep to view multi-ticker charts.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press 'm' to switch view modes.",
            Style::default().fg(colors::FG_DARK),
        )]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Multi-Ticker Chart", is_active));

    f.render_widget(para, area);
}

fn draw_chart_info(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
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
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .copied()
                .unwrap_or(0.0);
            let years = app.chart.equity_curve.len() as f64 / 252.0;
            let cagr = if years > 0.0 && initial > 0.0 {
                ((final_val / initial).powf(1.0 / years) - 1.0) * 100.0
            } else {
                0.0
            };
            (cagr, 0.0, max_dd, final_val, 0, 0.0)
        } else {
            (0.0, 0.0, 0.0, 0.0, 0, 0.0)
        };

    let view_mode = app.chart.view_mode_name();

    let lines = if app.chart.equity_curve.is_empty()
        && app.chart.ticker_curves.is_empty()
        && app.chart.portfolio_curve.is_empty()
    {
        vec![
            Line::from(vec![Span::styled(
                "Select a result to view statistics",
                Style::default().fg(colors::FG_DARK),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("'d': ", Style::default().fg(colors::CYAN)),
                Span::styled("Toggle drawdown  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'m': ", Style::default().fg(colors::CYAN)),
                Span::styled("Mode  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("\u{2191}\u{2193}: ", Style::default().fg(colors::CYAN)),
                Span::styled("Zoom  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("Esc: ", Style::default().fg(colors::CYAN)),
                Span::styled("Reset", Style::default().fg(colors::FG_DARK)),
            ]),
        ]
    } else {
        vec![
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
            ]),
            Line::from(vec![
                Span::styled("Trades: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{}", num_trades),
                    Style::default().fg(colors::YELLOW),
                ),
                Span::styled("   Win Rate: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.0}%", win_rate),
                    if win_rate > 50.0 {
                        Style::default().fg(colors::GREEN)
                    } else {
                        Style::default().fg(colors::ORANGE)
                    },
                ),
                Span::styled("   Zoom: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.0}%", app.chart.zoom_level * 100.0),
                    Style::default().fg(colors::YELLOW),
                ),
                Span::styled("   Mode: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    view_mode,
                    Style::default()
                        .fg(colors::MAGENTA)
                        .add_modifier(Modifier::BOLD),
                ),
                if app.chart.show_drawdown {
                    Span::styled("   [DD ON]", Style::default().fg(colors::RED))
                } else {
                    Span::styled("   [DD OFF]", Style::default().fg(colors::FG_DARK))
                },
            ]),
            Line::from(vec![
                Span::styled("'d': ", Style::default().fg(colors::CYAN)),
                Span::styled("Drawdown  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'m': ", Style::default().fg(colors::CYAN)),
                Span::styled("Mode  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("\u{2191}\u{2193}: ", Style::default().fg(colors::CYAN)),
                Span::styled("Zoom  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("\u{2190}\u{2192}: ", Style::default().fg(colors::CYAN)),
                Span::styled("Scroll  ", Style::default().fg(colors::FG_DARK)),
                Span::styled("Esc: ", Style::default().fg(colors::CYAN)),
                Span::styled("Reset", Style::default().fg(colors::FG_DARK)),
            ]),
        ]
    };

    let para = Paragraph::new(lines).block(panel_block("Statistics", is_active));

    f.render_widget(para, area);
}
