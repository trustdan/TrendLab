//! Value formatting helpers for chart rendering

use chrono::{DateTime, Utc};
use ratatui::{
    style::Style,
    text::Span,
};

use crate::ui::colors;

/// Format a currency value with K/M suffixes
pub fn format_value(v: f64) -> String {
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
pub fn format_price(v: f64) -> String {
    if v >= 1000.0 {
        format!("${:.0}", v)
    } else if v >= 100.0 {
        format!("${:.1}", v)
    } else {
        format!("${:.2}", v)
    }
}

/// Format volume with K/M suffix
pub fn format_volume(v: f64) -> String {
    if v >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if v >= 1_000.0 {
        format!("{:.0}K", v / 1_000.0)
    } else {
        format!("{:.0}", v)
    }
}

/// Get trend indicator Unicode symbol
pub fn trend_symbol(value: f64) -> &'static str {
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
pub fn generate_y_labels(y_min: f64, y_max: f64, count: usize) -> Vec<Span<'static>> {
    (0..count)
        .map(|i| {
            let value = y_min + (y_max - y_min) * i as f64 / (count - 1).max(1) as f64;
            Span::styled(format_value(value), Style::default().fg(colors::FG_DARK))
        })
        .collect()
}

/// Determine date format string based on date span
pub fn date_format_for_span(start: &DateTime<Utc>, end: &DateTime<Utc>) -> &'static str {
    let days = (*end - *start).num_days();
    if days < 90 {
        "%m-%d" // < 3 months: MM-DD
    } else if days < 730 {
        "%Y-%m" // 3-24 months: YYYY-MM
    } else {
        "%Y" // > 2 years: YYYY
    }
}

/// Generate X-axis date labels from dates vector (5 labels across the span)
pub fn generate_date_labels(dates: &[DateTime<Utc>]) -> Vec<Span<'static>> {
    if dates.is_empty() {
        return vec![
            Span::raw(""),
            Span::raw(""),
            Span::raw(""),
            Span::raw(""),
            Span::raw(""),
        ];
    }

    let format = date_format_for_span(dates.first().unwrap(), dates.last().unwrap());
    let len = dates.len();

    (0..=4)
        .map(|i| {
            let idx = (i * len / 4).min(len.saturating_sub(1));
            let date = &dates[idx];
            Span::styled(
                date.format(format).to_string(),
                Style::default().fg(colors::FG_DARK),
            )
        })
        .collect()
}

/// Generate X-axis index labels (fallback when no dates available)
pub fn generate_index_labels(x_max: f64) -> Vec<Span<'static>> {
    (0..=4)
        .map(|i| Span::from(format!("{}", (i as f64 * x_max / 4.0) as i32)))
        .collect()
}

/// Calculate price bounds from candle data
pub fn calculate_price_bounds(candles: &[trendlab_engine::app::CandleData]) -> (f64, f64) {
    if candles.is_empty() {
        return (0.0, 100.0);
    }
    let min = candles.iter().map(|c| c.low).fold(f64::MAX, f64::min);
    let max = candles.iter().map(|c| c.high).fold(f64::MIN, f64::max);
    let padding = (max - min) * 0.05;
    ((min - padding).max(0.0), max + padding)
}
