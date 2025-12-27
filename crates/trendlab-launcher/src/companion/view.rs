//! Companion terminal UI rendering.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Row, Table},
    Frame,
};

use super::state::CompanionState;
use crate::ipc::LogLevel;

/// Tokyo Night color palette.
mod colors {
    use ratatui::style::Color;

    pub const BG: Color = Color::Rgb(26, 27, 38);
    pub const FG: Color = Color::Rgb(169, 177, 214);
    pub const BLUE: Color = Color::Rgb(122, 162, 247);
    pub const GREEN: Color = Color::Rgb(158, 206, 106);
    pub const RED: Color = Color::Rgb(247, 118, 142);
    pub const YELLOW: Color = Color::Rgb(224, 175, 104);
    pub const PURPLE: Color = Color::Rgb(187, 154, 247);
    pub const CYAN: Color = Color::Rgb(125, 207, 255);
    pub const DARK: Color = Color::Rgb(36, 40, 59);
    pub const COMMENT: Color = Color::Rgb(86, 95, 137);
}

/// Render the companion UI.
pub fn render(frame: &mut Frame, state: &CompanionState) {
    if state.is_minimized() {
        render_minimized(frame, state);
    } else {
        render_full(frame, state);
    }
}

/// Render minimized single-line view.
fn render_minimized(frame: &mut Frame, state: &CompanionState) {
    let area = frame.area();

    let status = if state.has_active_job() {
        let (current, total) = state.job_progress();
        let percent = state.progress_percent() * 100.0;
        format!(
            "TrendLab | GUI PID {} | {} {:.0}% ({}/{}) | [Esc] expand | [q] quit",
            state.gui_pid(),
            state.current_job_type().map(|t| t.to_string()).unwrap_or_default(),
            percent,
            current,
            total
        )
    } else {
        format!(
            "TrendLab | GUI PID {} | {} | [Esc] expand | [q] quit",
            state.gui_pid(),
            state.status()
        )
    };

    let style = if state.is_connected() {
        Style::default().fg(colors::GREEN)
    } else {
        Style::default().fg(colors::RED)
    };

    let para = Paragraph::new(status).style(style);
    frame.render_widget(para, area);
}

/// Render full companion view.
fn render_full(frame: &mut Frame, state: &CompanionState) {
    let area = frame.area();

    // Main layout: status bar, progress, results, logs
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Status bar
            Constraint::Length(5),  // Progress section
            Constraint::Min(8),     // Results table
            Constraint::Length(8),  // Logs
        ])
        .split(area);

    render_status_bar(frame, chunks[0], state);
    render_progress(frame, chunks[1], state);
    render_results(frame, chunks[2], state);
    render_logs(frame, chunks[3], state);
}

/// Render the status bar.
fn render_status_bar(frame: &mut Frame, area: Rect, state: &CompanionState) {
    let connection_status = if state.is_connected() {
        Span::styled("Connected", Style::default().fg(colors::GREEN))
    } else {
        Span::styled("Disconnected", Style::default().fg(colors::RED))
    };

    let version = state
        .gui_version()
        .map(|v| format!(" v{}", v))
        .unwrap_or_default();

    let title = Line::from(vec![
        Span::styled(" TrendLab Companion ", Style::default().fg(colors::BLUE).add_modifier(Modifier::BOLD)),
        Span::raw("| GUI PID "),
        Span::styled(state.gui_pid().to_string(), Style::default().fg(colors::CYAN)),
        Span::styled(version, Style::default().fg(colors::COMMENT)),
        Span::raw(" | "),
        connection_status,
    ]);

    let hints = Line::from(vec![
        Span::styled(" [q] ", Style::default().fg(colors::YELLOW)),
        Span::raw("quit "),
        Span::styled("[Esc] ", Style::default().fg(colors::YELLOW)),
        Span::raw("minimize"),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::DARK))
        .title(title)
        .title_bottom(hints);

    let status_text = Paragraph::new(state.status())
        .style(Style::default().fg(colors::FG))
        .block(block);

    frame.render_widget(status_text, area);
}

/// Render the progress section.
fn render_progress(frame: &mut Frame, area: Rect, state: &CompanionState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::DARK))
        .title(Span::styled(" Progress ", Style::default().fg(colors::PURPLE)));

    if state.has_active_job() {
        let (current, total) = state.job_progress();
        let percent = state.progress_percent();

        let job_type = state
            .current_job_type()
            .map(|t| t.to_string())
            .unwrap_or_else(|| "Job".to_string());
        let desc = state.current_job_desc().unwrap_or("");

        let label = format!(
            "{}: {} | {:.0}% ({}/{})",
            job_type,
            desc,
            percent * 100.0,
            current,
            total
        );

        let gauge = Gauge::default()
            .block(block)
            .gauge_style(Style::default().fg(colors::BLUE).bg(colors::DARK))
            .percent((percent * 100.0) as u16)
            .label(Span::styled(label, Style::default().fg(colors::FG)));

        frame.render_widget(gauge, area);

        // Show current item below gauge
        let inner = area.inner(ratatui::layout::Margin {
            vertical: 2,
            horizontal: 1,
        });
        if !state.job_message().is_empty() {
            let current_item = Paragraph::new(format!("Current: {}", state.job_message()))
                .style(Style::default().fg(colors::COMMENT));
            frame.render_widget(current_item, inner);
        }
    } else {
        let para = Paragraph::new("No active job")
            .style(Style::default().fg(colors::COMMENT))
            .block(block);
        frame.render_widget(para, area);
    }
}

/// Render the results table.
fn render_results(frame: &mut Frame, area: Rect, state: &CompanionState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::DARK))
        .title(Span::styled(" Recent Results ", Style::default().fg(colors::GREEN)));

    let results = state.recent_results();

    if results.is_empty() {
        let para = Paragraph::new("No results yet")
            .style(Style::default().fg(colors::COMMENT))
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    let header = Row::new(vec!["Ticker", "Strategy", "Config", "Sharpe", "CAGR", "Max DD"])
        .style(Style::default().fg(colors::BLUE).add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = results
        .iter()
        .take(10) // Show at most 10 rows
        .map(|r| {
            let sharpe_color = if r.sharpe >= 1.0 {
                colors::GREEN
            } else if r.sharpe >= 0.5 {
                colors::YELLOW
            } else {
                colors::RED
            };

            Row::new(vec![
                Span::styled(r.ticker.clone(), Style::default().fg(colors::FG)),
                Span::styled(r.strategy.clone(), Style::default().fg(colors::FG)),
                Span::styled(r.config_id.clone(), Style::default().fg(colors::COMMENT)),
                Span::styled(format!("{:.2}", r.sharpe), Style::default().fg(sharpe_color)),
                Span::styled(format!("{:.1}%", r.cagr * 100.0), Style::default().fg(colors::FG)),
                Span::styled(format!("{:.1}%", r.max_dd * 100.0), Style::default().fg(colors::RED)),
            ])
            .height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),   // Ticker
            Constraint::Length(20),  // Strategy
            Constraint::Length(12),  // Config
            Constraint::Length(8),   // Sharpe
            Constraint::Length(8),   // CAGR
            Constraint::Length(8),   // Max DD
        ],
    )
    .header(header)
    .block(block);

    frame.render_widget(table, area);
}

/// Render the logs section.
fn render_logs(frame: &mut Frame, area: Rect, state: &CompanionState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::DARK))
        .title(Span::styled(" Log ", Style::default().fg(colors::CYAN)));

    let logs = state.logs();

    if logs.is_empty() {
        let para = Paragraph::new("No log messages")
            .style(Style::default().fg(colors::COMMENT))
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    let lines: Vec<Line> = logs
        .iter()
        .take(6) // Show at most 6 log lines
        .map(|entry| {
            let level_style = match entry.level {
                LogLevel::Debug => Style::default().fg(colors::COMMENT),
                LogLevel::Info => Style::default().fg(colors::BLUE),
                LogLevel::Warn => Style::default().fg(colors::YELLOW),
                LogLevel::Error => Style::default().fg(colors::RED),
            };

            let time = entry.ts.format("%H:%M:%S").to_string();

            Line::from(vec![
                Span::styled(format!("[{}] ", time), Style::default().fg(colors::COMMENT)),
                Span::styled(format!("{:5} ", entry.level), level_style),
                Span::styled(&entry.message, Style::default().fg(colors::FG)),
            ])
        })
        .collect();

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}
