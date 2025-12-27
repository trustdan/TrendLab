//! Chart panel - visualize equity curves, candlesticks, and drawdowns with multi-curve support

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Line as CanvasLine, Rectangle},
        Axis, Block, Borders, Chart, Clear, Dataset, GraphType, LegendPosition, Paragraph,
    },
    Frame,
};

use trendlab_core::StrategyTypeId;

use crate::app::{App, CandleData, ChartViewMode, Panel, StrategyCurve, TickerBestStrategy};
use crate::ui::{colors, panel_block};

// ============================================================================
// Value Formatting Helpers
// ============================================================================

/// Format a currency value with K/M suffixes
fn format_value(v: f64) -> String {
    let abs_v = v.abs();
    let sign = if v < 0.0 { "-" } else { "" };
    if abs_v >= 1_000_000.0 {
        format!("{}${:.1}M", sign, abs_v / 1_000_000.0)
    } else if abs_v >= 1_000.0 {
        format!("{}${:.0}K", sign, abs_v / 1_000.0)
    } else {
        format!("{}${:.0}", sign, abs_v)
    }
}

/// Format a price value (for candlestick charts)
fn format_price(v: f64) -> String {
    if v >= 1000.0 {
        format!("${:.0}", v)
    } else if v >= 100.0 {
        format!("${:.1}", v)
    } else {
        format!("${:.2}", v)
    }
}

/// Get trend indicator Unicode symbol
fn trend_symbol(value: f64) -> &'static str {
    if value > 0.0 {
        "\u{25B2}" // ▲
    } else if value < 0.0 {
        "\u{25BC}" // ▼
    } else {
        "\u{25AC}" // ▬
    }
}

/// Generate Y-axis labels with smart formatting
#[allow(dead_code)]
fn generate_y_labels(y_min: f64, y_max: f64, count: usize) -> Vec<Span<'static>> {
    (0..count)
        .map(|i| {
            let value = y_min + (y_max - y_min) * i as f64 / (count - 1).max(1) as f64;
            Span::styled(format_value(value), Style::default().fg(colors::FG_DARK))
        })
        .collect()
}

/// Calculate price bounds from candle data
fn calculate_price_bounds(candles: &[CandleData]) -> (f64, f64) {
    if candles.is_empty() {
        return (0.0, 100.0);
    }
    let min = candles.iter().map(|c| c.low).fold(f64::MAX, f64::min);
    let max = candles.iter().map(|c| c.high).fold(f64::MIN, f64::max);
    let padding = (max - min) * 0.05;
    ((min - padding).max(0.0), max + padding)
}

/// Color palette for multi-ticker curves
const CURVE_COLORS: &[Color] = &[
    Color::Rgb(46, 204, 113),  // Emerald green
    Color::Rgb(52, 152, 219),  // Blue
    Color::Rgb(155, 89, 182),  // Purple
    Color::Rgb(241, 196, 15),  // Yellow
    Color::Rgb(231, 76, 60),   // Red
    Color::Rgb(26, 188, 156),  // Turquoise
    Color::Rgb(230, 126, 34),  // Orange
    Color::Rgb(236, 240, 241), // Light gray
    Color::Rgb(149, 165, 166), // Gray
    Color::Rgb(46, 134, 193),  // Steel blue
    Color::Rgb(175, 122, 197), // Amethyst
    Color::Rgb(244, 208, 63),  // Sunflower
];

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Chart;

    // Split into chart area and info panel
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(6)])
        .split(area);

    // Store chart area for mouse hit-testing
    app.chart.chart_area.set(Some(chunks[0]));

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
        .legend_position(Some(LegendPosition::TopRight))
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

/// Fixed color mapping for strategy types
fn strategy_color(strategy_type: StrategyTypeId) -> Color {
    match strategy_type {
        StrategyTypeId::Donchian => Color::Rgb(46, 204, 113), // Green
        StrategyTypeId::TurtleS1 => Color::Rgb(52, 152, 219), // Blue
        StrategyTypeId::TurtleS2 => Color::Rgb(155, 89, 182), // Purple
        StrategyTypeId::MACrossover => Color::Rgb(241, 196, 15), // Yellow
        StrategyTypeId::Tsmom => Color::Rgb(231, 76, 60),     // Red
        StrategyTypeId::Keltner => Color::Rgb(230, 126, 34),  // Orange
        StrategyTypeId::STARC => Color::Rgb(26, 188, 156),    // Teal
        StrategyTypeId::Supertrend => Color::Rgb(142, 68, 173), // Dark Purple
        StrategyTypeId::DmiAdx => Color::Rgb(22, 160, 133),   // Dark Teal
        StrategyTypeId::Aroon => Color::Rgb(39, 174, 96),     // Dark Green
        StrategyTypeId::BollingerSqueeze => Color::Rgb(41, 128, 185), // Dark Blue
        StrategyTypeId::FiftyTwoWeekHigh => Color::Rgb(192, 57, 43), // Dark Red
        StrategyTypeId::DarvasBox => Color::Rgb(243, 156, 18), // Orange-Yellow
        StrategyTypeId::LarryWilliams => Color::Rgb(211, 84, 0), // Burnt Orange
        StrategyTypeId::HeikinAshi => Color::Rgb(127, 140, 141), // Gray
        StrategyTypeId::ParabolicSar => Color::Rgb(44, 62, 80), // Dark Gray
        StrategyTypeId::OpeningRangeBreakout => Color::Rgb(189, 195, 199), // Light Gray
        StrategyTypeId::Ensemble => Color::Rgb(149, 165, 166), // Silver
    }
}

/// Create legend line for strategy comparison view
fn strategy_legend_line(curves: &[StrategyCurve]) -> Line<'static> {
    let mut spans = vec![];
    for (i, curve) in curves.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let color = strategy_color(curve.strategy_type);
        spans.push(Span::styled("■ ", Style::default().fg(color)));
        spans.push(Span::styled(
            curve.strategy_type.name().to_string(),
            Style::default().fg(colors::FG),
        ));
    }
    Line::from(spans)
}

/// Create legend line for per-ticker best strategy view
fn ticker_best_legend_line(tickers: &[TickerBestStrategy]) -> Line<'static> {
    let mut spans = vec![];
    for (i, ticker) in tickers.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let color = strategy_color(ticker.strategy_type);
        spans.push(Span::styled("■ ", Style::default().fg(color)));
        spans.push(Span::styled(
            ticker.symbol.to_string(),
            Style::default().fg(colors::FG),
        ));
    }
    Line::from(spans)
}

/// Draw strategy comparison chart - overlay best config per strategy
fn draw_strategy_comparison_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.strategy_curves.is_empty() {
        draw_no_strategy_data(f, area, is_active);
        return;
    }

    // Split area: legend line at top, chart below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(area);

    // Render legend
    let legend = Paragraph::new(strategy_legend_line(&app.chart.strategy_curves))
        .style(Style::default().bg(colors::BG));
    f.render_widget(legend, chunks[0]);

    // Prepare all curve data
    let mut all_data: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;
    let mut x_max = 0.0_f64;

    for curve in &app.chart.strategy_curves {
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

    // Create datasets for each strategy
    let datasets: Vec<Dataset> = all_data
        .iter()
        .zip(app.chart.strategy_curves.iter())
        .map(|(data, curve)| {
            let color = strategy_color(curve.strategy_type);
            Dataset::default()
                .name(curve.strategy_type.name())
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
        "Strategy Comparison ({} strategies)",
        app.chart.strategy_curves.len()
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

    f.render_widget(chart, chunks[1]);
}

/// Draw per-ticker best strategy chart - each ticker's best strategy
fn draw_per_ticker_best_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.ticker_best_strategies.is_empty() {
        draw_no_strategy_data(f, area, is_active);
        return;
    }

    // Split area: legend line at top, chart below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(area);

    // Render legend
    let legend = Paragraph::new(ticker_best_legend_line(&app.chart.ticker_best_strategies))
        .style(Style::default().bg(colors::BG));
    f.render_widget(legend, chunks[0]);

    // Prepare all curve data
    let mut all_data: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;
    let mut x_max = 0.0_f64;

    for ticker_best in &app.chart.ticker_best_strategies {
        let data: Vec<(f64, f64)> = ticker_best
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

    // Create datasets for each ticker's best strategy
    let datasets: Vec<Dataset> = all_data
        .iter()
        .zip(app.chart.ticker_best_strategies.iter())
        .map(|(data, ticker_best)| {
            // Use strategy color for this ticker's best strategy
            let color = strategy_color(ticker_best.strategy_type);
            let label = format!(
                "{} ({})",
                ticker_best.symbol,
                ticker_best.strategy_type.name()
            );
            Dataset::default()
                .name(label)
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
        "Per-Ticker Best Strategy ({} tickers)",
        app.chart.ticker_best_strategies.len()
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

    f.render_widget(chart, chunks[1]);
}

fn draw_no_strategy_data(f: &mut Frame, area: Rect, is_active: bool) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "No multi-strategy data available.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Run Full-Auto mode to compare strategies across all tickers.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press 'm' to switch view modes.",
            Style::default().fg(colors::FG_DARK),
        )]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Strategy Comparison", is_active));

    f.render_widget(para, area);
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

// ============================================================================
// Candlestick Chart Rendering
// ============================================================================

/// Draw OHLC candlestick chart using Canvas widget
fn draw_candlestick_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
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
fn draw_grid_lines(
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

fn draw_no_candle_data(f: &mut Frame, area: Rect, is_active: bool) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "No candlestick data available.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Load price data in the Data panel to view candlesticks.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press 'm' to switch view modes.",
            Style::default().fg(colors::FG_DARK),
        )]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Candlestick Chart", is_active));
    f.render_widget(para, area);
}

// ============================================================================
// Volume Subplot
// ============================================================================

/// Draw volume bars subplot
fn draw_volume_bars(f: &mut Frame, app: &App, area: Rect) {
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

    let visible_candles: Vec<&CandleData> = candles[start_idx..end_idx].iter().collect();

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

// ============================================================================
// Crosshair Overlay
// ============================================================================

/// Draw crosshair at cursor position
fn draw_crosshair(f: &mut Frame, app: &App, area: Rect) {
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

// ============================================================================
// Tooltip Overlay
// ============================================================================

/// Draw data tooltip near cursor
fn draw_tooltip(f: &mut Frame, app: &App, area: Rect) {
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

/// Format volume with K/M suffix
fn format_volume(v: f64) -> String {
    if v >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if v >= 1_000.0 {
        format!("{:.0}K", v / 1_000.0)
    } else {
        format!("{:.0}", v)
    }
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

    let _view_mode = app.chart.view_mode_name();

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
