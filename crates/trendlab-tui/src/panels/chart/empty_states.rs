//! Empty and placeholder state rendering for charts

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::ui::{colors, panel_block};

/// Draw empty chart placeholder
pub fn draw_empty_chart(f: &mut Frame, area: Rect, is_active: bool) {
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

/// Draw no multi-ticker data placeholder
pub fn draw_no_multi_data(f: &mut Frame, area: Rect, is_active: bool) {
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

/// Draw no strategy data placeholder
pub fn draw_no_strategy_data(f: &mut Frame, area: Rect, is_active: bool) {
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

/// Draw no candlestick data placeholder
pub fn draw_no_candle_data(f: &mut Frame, area: Rect, is_active: bool) {
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
