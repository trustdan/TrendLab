//! Strategy comparison chart rendering

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
    Frame,
};

use crate::ui::{colors, panel_block};
use trendlab_engine::app::{App, StrategyCurve, TickerBestStrategy};

use super::colors::{strategy_color, CURVE_COLORS};
use super::empty_states::draw_no_strategy_data;
use super::formatters::{generate_date_labels, generate_index_labels};

/// Maximum strategies to show in comparison chart (top N by Sharpe)
const MAX_STRATEGY_CURVES: usize = 5;

/// Create legend line for strategy comparison view with Sharpe ratios and params
fn strategy_legend_line(curves: &[&StrategyCurve]) -> Line<'static> {
    let mut spans = vec![];
    for (i, curve) in curves.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  ")); // More spacing between entries
        }
        // Use index-based colors so each curve is distinct regardless of strategy type
        let color = CURVE_COLORS[i % CURVE_COLORS.len()];
        spans.push(Span::styled("■", Style::default().fg(color)));
        spans.push(Span::styled(
            format!("{}: ", curve.strategy_type.name()),
            Style::default().fg(colors::FG),
        ));
        // Show config parameters
        spans.push(Span::styled(
            format!("{} ", curve.config_display),
            Style::default().fg(colors::YELLOW),
        ));
        // Show Sharpe ratio
        spans.push(Span::styled(
            format!("({:.2})", curve.metrics.sharpe),
            Style::default().fg(colors::FG_DARK),
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

/// Draw strategy comparison chart - overlay best config per strategy (top 5 only)
pub fn draw_strategy_comparison_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.strategy_curves.is_empty() {
        draw_no_strategy_data(f, area, is_active);
        return;
    }

    // Take only top N strategies by Sharpe (curves are already sorted by Sharpe desc)
    let top_curves: Vec<&StrategyCurve> = app
        .chart
        .strategy_curves
        .iter()
        .take(MAX_STRATEGY_CURVES)
        .collect();

    // Find max data length across all curves
    let total_bars = top_curves.iter().map(|c| c.equity.len()).max().unwrap_or(0);

    // Apply zoom and scroll to get visible range
    let visible_count = ((total_bars as f64 / app.chart.zoom_level) as usize).max(10);
    let max_scroll = total_bars.saturating_sub(visible_count);
    let scroll_offset = app.chart.scroll_offset.min(max_scroll);
    let start_idx = scroll_offset;
    let end_idx = (start_idx + visible_count).min(total_bars);

    // Split area: legend line at top, chart below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(area);

    // Render legend with strategy names and Sharpe ratios
    let legend =
        Paragraph::new(strategy_legend_line(&top_curves)).style(Style::default().bg(colors::BG));
    f.render_widget(legend, chunks[0]);

    // Prepare curve data for top strategies only (sliced to visible range)
    let mut all_data: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;

    for curve in &top_curves {
        let curve_len = curve.equity.len();
        let curve_start = start_idx.min(curve_len);
        let curve_end = end_idx.min(curve_len);

        // Skip curves that are entirely outside the visible range
        if curve_start >= curve_end {
            all_data.push(vec![]); // Keep indices aligned
            continue;
        }

        let data: Vec<(f64, f64)> = curve.equity[curve_start..curve_end]
            .iter()
            .enumerate()
            .map(|(i, v)| (i as f64, *v))
            .collect();

        for (_, y) in &data {
            if y.is_finite() {
                y_min = y_min.min(*y);
                y_max = y_max.max(*y);
            }
        }
        all_data.push(data);
    }

    let x_max = (end_idx - start_idx) as f64;

    // Apply margins
    if y_min != f64::MAX {
        y_min *= 0.95;
    }
    if y_max != f64::MIN {
        y_max *= 1.05;
    }

    // Create datasets for each top strategy with distinct colors
    let datasets: Vec<Dataset> = all_data
        .iter()
        .zip(top_curves.iter())
        .enumerate()
        .map(|(i, (data, curve))| {
            // Use index-based colors so each curve is distinct
            let color = CURVE_COLORS[i % CURVE_COLORS.len()];
            // Concise name with config and Sharpe for chart legend
            let name = format!(
                "{} {} ({:.2})",
                curve.strategy_type.name(),
                curve.config_display,
                curve.metrics.sharpe
            );
            Dataset::default()
                .name(name)
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(color))
                .data(data)
        })
        .collect();

    // Use dates from first strategy curve if available (sliced)
    let x_labels: Vec<Span> = top_curves
        .first()
        .filter(|c| c.dates.len() >= end_idx)
        .map(|c| generate_date_labels(&c.dates[start_idx..end_idx]))
        .or_else(|| {
            top_curves
                .first()
                .filter(|c| !c.dates.is_empty())
                .map(|c| generate_date_labels(&c.dates))
        })
        .unwrap_or_else(|| generate_index_labels(x_max));

    let y_labels = vec![
        Span::from(format!("${:.0}k", y_min / 1000.0)),
        Span::from(format!("${:.0}k", (y_min + y_max) / 2000.0)),
        Span::from(format!("${:.0}k", y_max / 1000.0)),
    ];

    let total = app.chart.strategy_curves.len();
    let shown = top_curves.len();
    let title = format!("Top {} Strategies by Sharpe (of {})", shown, total);

    let chart = Chart::new(datasets)
        .block(panel_block(&title, is_active))
        .x_axis(
            Axis::default()
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
pub fn draw_per_ticker_best_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.ticker_best_strategies.is_empty() {
        draw_no_strategy_data(f, area, is_active);
        return;
    }

    // Find max data length across all curves
    let total_bars = app
        .chart
        .ticker_best_strategies
        .iter()
        .map(|t| t.equity.len())
        .max()
        .unwrap_or(0);

    // Apply zoom and scroll to get visible range
    let visible_count = ((total_bars as f64 / app.chart.zoom_level) as usize).max(10);
    let max_scroll = total_bars.saturating_sub(visible_count);
    let scroll_offset = app.chart.scroll_offset.min(max_scroll);
    let start_idx = scroll_offset;
    let end_idx = (start_idx + visible_count).min(total_bars);

    // Split area: legend line at top, chart below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(area);

    // Render legend
    let legend = Paragraph::new(ticker_best_legend_line(&app.chart.ticker_best_strategies))
        .style(Style::default().bg(colors::BG));
    f.render_widget(legend, chunks[0]);

    // Prepare all curve data (sliced to visible range)
    let mut all_data: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;

    for ticker_best in &app.chart.ticker_best_strategies {
        let curve_len = ticker_best.equity.len();
        let curve_start = start_idx.min(curve_len);
        let curve_end = end_idx.min(curve_len);

        // Skip curves that are entirely outside the visible range
        if curve_start >= curve_end {
            all_data.push(vec![]); // Keep indices aligned
            continue;
        }

        let data: Vec<(f64, f64)> = ticker_best.equity[curve_start..curve_end]
            .iter()
            .enumerate()
            .map(|(i, v)| (i as f64, *v))
            .collect();

        for (_, y) in &data {
            if y.is_finite() {
                y_min = y_min.min(*y);
                y_max = y_max.max(*y);
            }
        }
        all_data.push(data);
    }

    let x_max = (end_idx - start_idx) as f64;

    // Apply margins
    if y_min != f64::MAX {
        y_min *= 0.95;
    }
    if y_max != f64::MIN {
        y_max *= 1.05;
    }

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

    // Use dates from first ticker's best strategy if available (sliced)
    let x_labels: Vec<Span> = app
        .chart
        .ticker_best_strategies
        .first()
        .filter(|t| t.dates.len() >= end_idx)
        .map(|t| generate_date_labels(&t.dates[start_idx..end_idx]))
        .or_else(|| {
            app.chart
                .ticker_best_strategies
                .first()
                .filter(|t| !t.dates.is_empty())
                .map(|t| generate_date_labels(&t.dates))
        })
        .unwrap_or_else(|| generate_index_labels(x_max));

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
