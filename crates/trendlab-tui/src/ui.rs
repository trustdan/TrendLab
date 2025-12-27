//! Main UI rendering for TrendLab TUI

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

use crate::app::{App, Panel};
use crate::panels;

/// Tokyo Night color palette
pub mod colors {
    use ratatui::style::Color;

    pub const BG: Color = Color::Rgb(26, 27, 38);
    pub const FG: Color = Color::Rgb(169, 177, 214);
    pub const FG_DARK: Color = Color::Rgb(86, 95, 137);
    pub const BLUE: Color = Color::Rgb(122, 162, 247);
    pub const CYAN: Color = Color::Rgb(125, 207, 255);
    pub const GREEN: Color = Color::Rgb(158, 206, 106);
    pub const MAGENTA: Color = Color::Rgb(187, 154, 247);
    pub const ORANGE: Color = Color::Rgb(255, 158, 100);
    pub const RED: Color = Color::Rgb(247, 118, 142);
    pub const YELLOW: Color = Color::Rgb(224, 175, 104);
    pub const BORDER: Color = Color::Rgb(61, 66, 91);
    pub const BORDER_ACTIVE: Color = Color::Rgb(122, 162, 247);
}

/// Draw the main UI
pub fn draw(f: &mut Frame, app: &App) {
    // Main layout: tabs at top, content in middle, status at bottom
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Status
        ])
        .split(f.area());

    // Draw tabs
    draw_tabs(f, app, chunks[0]);

    // Draw active panel content
    let content_area = chunks[1];
    match app.active_panel {
        Panel::Data => panels::data::draw(f, app, content_area),
        Panel::Strategy => panels::strategy::draw(f, app, content_area),
        Panel::Sweep => panels::sweep::draw(f, app, content_area),
        Panel::Results => panels::results::draw(f, app, content_area),
        Panel::Chart => panels::chart::draw(f, app, content_area),
    }

    // Draw status bar
    draw_status(f, app, chunks[2]);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Panel::all()
        .iter()
        .map(|p| {
            let style = if *p == app.active_panel {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::FG_DARK)
            };
            Line::from(vec![
                Span::styled(
                    format!("[{}] ", p.hotkey()),
                    Style::default().fg(colors::YELLOW),
                ),
                Span::styled(p.title(), style),
            ])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors::BORDER))
                .title(Span::styled(
                    " TrendLab ",
                    Style::default()
                        .fg(colors::MAGENTA)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .style(Style::default().fg(colors::FG))
        .highlight_style(
            Style::default()
                .fg(colors::BLUE)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled(" | ", Style::default().fg(colors::FG_DARK)));

    f.render_widget(tabs, area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let help_text = match app.active_panel {
        Panel::Data => "↑↓: Select symbol  Enter: Load data  Tab: Next panel",
        Panel::Strategy => "↑↓: Select field  ←→: Adjust value  Tab: Next panel",
        Panel::Sweep => "Enter: Start sweep  Esc: Cancel  ↑↓: Select param",
        Panel::Results => "↑↓: Select result  Enter: View chart  Tab: Next panel",
        Panel::Chart => "←→: Scroll  ↑↓: Zoom  Esc: Reset view",
    };

    let status_line = Line::from(vec![
        Span::styled(&app.status_message, Style::default().fg(colors::GREEN)),
        Span::styled(" | ", Style::default().fg(colors::FG_DARK)),
        Span::styled(help_text, Style::default().fg(colors::FG_DARK)),
    ]);

    let status = Paragraph::new(status_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::BORDER))
            .title(Span::styled(
                " Status ",
                Style::default().fg(colors::FG_DARK),
            )),
    );

    f.render_widget(status, area);
}

/// Create a styled block for panels
pub fn panel_block(title: &str, is_active: bool) -> Block<'_> {
    let border_color = if is_active {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER
    };

    let title_style = if is_active {
        Style::default()
            .fg(colors::BLUE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::FG_DARK)
    };

    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(format!(" {} ", title), title_style))
}
