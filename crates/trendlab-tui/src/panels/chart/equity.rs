//! Equity curve chart rendering (single, multi-ticker, portfolio)

use ratatui::{
    layout::Rect,
    style::Style,
    symbols::Marker,
    text::Span,
    widgets::{Axis, Chart, Dataset, GraphType, LegendPosition},
    Frame,
};

use crate::ui::{colors, panel_block};
use trendlab_engine::app::App;

use super::colors::CURVE_COLORS;
use super::empty_states::{draw_empty_chart, draw_no_multi_data};
use super::formatters::{generate_date_labels, generate_index_labels};

/// Draw single equity curve (original behavior)
pub fn draw_single_equity_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    // Check if we have actual data
    if app.chart.equity_curve.is_empty() {
        draw_empty_chart(f, area, is_active);
        return;
    }

    let total_bars = app.chart.equity_curve.len();

    // Apply zoom and scroll to get visible range
    let visible_count = ((total_bars as f64 / app.chart.zoom_level) as usize).max(10);
    let max_scroll = total_bars.saturating_sub(visible_count);
    let scroll_offset = app.chart.scroll_offset.min(max_scroll);
    let start_idx = scroll_offset;
    let end_idx = (start_idx + visible_count).min(total_bars);

    // Prepare visible equity data (re-indexed from 0)
    let equity_data: Vec<(f64, f64)> = app.chart.equity_curve[start_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect();

    // Calculate bounds for visible data
    let x_max = equity_data.len() as f64;
    let y_min = equity_data
        .iter()
        .map(|(_, y)| *y)
        .filter(|y| y.is_finite())
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(90000.0)
        * 0.95;
    let y_max = equity_data
        .iter()
        .map(|(_, y)| *y)
        .filter(|y| y.is_finite())
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(150000.0)
        * 1.05;

    // Create datasets
    let mut datasets = vec![Dataset::default()
        .name("Equity")
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(colors::GREEN))
        .data(&equity_data)];

    // Add drawdown overlay if enabled (also sliced to visible range)
    let drawdown_data: Vec<(f64, f64)> =
        if app.chart.show_drawdown && !app.chart.drawdown_curve.is_empty() {
            let dd_len = app.chart.drawdown_curve.len();
            // Calculate safe bounds for drawdown curve
            let dd_start = start_idx.min(dd_len);
            let dd_end = end_idx.min(dd_len);

            if dd_start < dd_end {
                // Scale drawdown to fit in the same chart (inverted, as percentage)
                let dd_scale = (y_max - y_min) / 50.0; // Max 50% drawdown fills chart
                app.chart.drawdown_curve[dd_start..dd_end]
                    .iter()
                    .enumerate()
                    .map(|(i, dd)| (i as f64, y_max + dd * dd_scale))
                    .collect()
            } else {
                vec![]
            }
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

    // Use dates if available (sliced to visible range), otherwise fall back to day indices
    let x_labels: Vec<Span> = if app.chart.equity_dates.len() >= end_idx {
        generate_date_labels(&app.chart.equity_dates[start_idx..end_idx])
    } else if !app.chart.equity_dates.is_empty() {
        // Fallback: use available dates
        generate_date_labels(&app.chart.equity_dates)
    } else {
        generate_index_labels(x_max)
    };

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
pub fn draw_multi_ticker_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.ticker_curves.is_empty() {
        draw_no_multi_data(f, area, is_active);
        return;
    }

    // Find max data length across all curves
    let total_bars = app
        .chart
        .ticker_curves
        .iter()
        .map(|c| c.equity.len())
        .max()
        .unwrap_or(0);

    // Apply zoom and scroll to get visible range
    let visible_count = ((total_bars as f64 / app.chart.zoom_level) as usize).max(10);
    let max_scroll = total_bars.saturating_sub(visible_count);
    let scroll_offset = app.chart.scroll_offset.min(max_scroll);
    let start_idx = scroll_offset;
    let end_idx = (start_idx + visible_count).min(total_bars);

    // Prepare all curve data (sliced to visible range)
    let mut all_data: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;

    for curve in &app.chart.ticker_curves {
        let curve_len = curve.equity.len();
        let curve_start = start_idx.min(curve_len);
        let curve_end = end_idx.min(curve_len);

        // Skip curves that are entirely outside the visible range
        if curve_start >= curve_end {
            all_data.push(vec![]); // Keep indices aligned with ticker_curves
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

    // Use dates from first curve if available (sliced), otherwise fall back to indices
    let x_labels: Vec<Span> = app
        .chart
        .ticker_curves
        .first()
        .filter(|c| c.dates.len() >= end_idx)
        .map(|c| generate_date_labels(&c.dates[start_idx..end_idx]))
        .or_else(|| {
            app.chart
                .ticker_curves
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

    let title = format!(
        "Multi-Ticker Equity ({} symbols)",
        app.chart.ticker_curves.len()
    );

    let chart = Chart::new(datasets)
        .block(panel_block(&title, is_active))
        .legend_position(Some(LegendPosition::TopRight))
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

    f.render_widget(chart, area);
}

/// Draw portfolio aggregate equity curve
pub fn draw_portfolio_chart(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    if app.chart.portfolio_curve.is_empty() {
        draw_no_multi_data(f, area, is_active);
        return;
    }

    let total_bars = app.chart.portfolio_curve.len();

    // Apply zoom and scroll to get visible range
    let visible_count = ((total_bars as f64 / app.chart.zoom_level) as usize).max(10);
    let max_scroll = total_bars.saturating_sub(visible_count);
    let scroll_offset = app.chart.scroll_offset.min(max_scroll);
    let start_idx = scroll_offset;
    let end_idx = (start_idx + visible_count).min(total_bars);

    // Prepare visible portfolio data (re-indexed from 0)
    let portfolio_data: Vec<(f64, f64)> = app.chart.portfolio_curve[start_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect();

    // Calculate bounds for visible data
    let x_max = portfolio_data.len() as f64;
    let y_min = portfolio_data
        .iter()
        .map(|(_, y)| *y)
        .filter(|y| y.is_finite())
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(90000.0)
        * 0.95;
    let y_max = portfolio_data
        .iter()
        .map(|(_, y)| *y)
        .filter(|y| y.is_finite())
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(150000.0)
        * 1.05;

    let datasets = vec![Dataset::default()
        .name("Portfolio")
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(colors::CYAN))
        .data(&portfolio_data)];

    // Use dates from first ticker curve if available (sliced), otherwise fall back to indices
    let x_labels: Vec<Span> = app
        .chart
        .ticker_curves
        .first()
        .filter(|c| c.dates.len() >= end_idx)
        .map(|c| generate_date_labels(&c.dates[start_idx..end_idx]))
        .or_else(|| {
            app.chart
                .ticker_curves
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

    let num_symbols = app.chart.ticker_curves.len();
    let title = format!("Portfolio Equity ({} symbols combined)", num_symbols);

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

    f.render_widget(chart, area);
}
