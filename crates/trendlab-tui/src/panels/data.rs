//! Data panel - sector/ticker hierarchy with multi-select for backtesting

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use trendlab_engine::app::{App, DataViewMode, OperationState, Panel};
use crate::ui::{colors, panel_block};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Data;

    // Handle search mode overlay
    if app.data.search_mode {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        draw_sector_list(f, app, chunks[0], is_active);
        draw_search_overlay(f, app, chunks[1]);
        return;
    }

    // Split into left pane (sectors) and right pane (tickers/details)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // Left pane: Always show sector list
    draw_sector_list(f, app, chunks[0], is_active);

    // Right pane: Show tickers for selected sector OR data details
    match app.data.view_mode {
        DataViewMode::Sectors => {
            // Show sector summary/help when in sector view
            draw_sector_summary(f, app, chunks[1], is_active);
        }
        DataViewMode::Tickers => {
            // Show ticker list for selected sector
            draw_ticker_list(f, app, chunks[1], is_active);
        }
    }
}

/// Draw the sector list (left pane)
fn draw_sector_list(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let is_sector_focused = is_active && matches!(app.data.view_mode, DataViewMode::Sectors);

    let items: Vec<ListItem> = app
        .data
        .universe
        .sectors
        .iter()
        .enumerate()
        .map(|(i, sector)| {
            let is_selected = i == app.data.selected_sector_index;
            let selected_count = app.data.selected_count_in_sector(&sector.id);
            let total_count = sector.tickers.len();

            // Style based on selection state
            let style = if is_selected && is_sector_focused {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(colors::CYAN)
            } else {
                Style::default().fg(colors::FG)
            };

            // Selection indicator
            let prefix = if is_selected { "▸ " } else { "  " };
            let prefix_style = if is_selected && is_sector_focused {
                Style::default().fg(colors::YELLOW)
            } else {
                Style::default().fg(colors::FG_DARK)
            };

            // Show selection count badge
            let count_style = if selected_count > 0 {
                Style::default().fg(colors::GREEN)
            } else {
                Style::default().fg(colors::FG_DARK)
            };

            let count_text = format!(" [{}/{}]", selected_count, total_count);

            ListItem::new(Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(&sector.name, style),
                Span::styled(count_text, count_style),
            ]))
        })
        .collect();

    // Calculate total selected
    let total_selected = app.data.selected_tickers.len();
    let title = format!("Sectors ({} selected)", total_selected);

    let list = List::new(items).block(panel_block(&title, is_sector_focused));
    f.render_widget(list, area);
}

/// Draw the ticker list for selected sector (right pane in Tickers mode)
fn draw_ticker_list(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let is_ticker_focused = is_active && matches!(app.data.view_mode, DataViewMode::Tickers);

    // Get current sector
    let sector = match app.data.selected_sector() {
        Some(s) => s,
        None => {
            let para = Paragraph::new("No sector selected").block(panel_block("Tickers", false));
            f.render_widget(para, area);
            return;
        }
    };

    // Split into ticker list and details
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(8)])
        .split(area);

    // Ticker list
    let items: Vec<ListItem> = sector
        .tickers
        .iter()
        .enumerate()
        .map(|(i, ticker)| {
            let is_cursor = i == app.data.selected_ticker_index;
            let is_checked = app.data.is_ticker_selected(ticker);
            let is_loaded = app.data.bars_cache.contains_key(ticker);

            // Cursor style
            let style = if is_cursor && is_ticker_focused {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else if is_cursor {
                Style::default().fg(colors::CYAN)
            } else if is_checked {
                Style::default().fg(colors::GREEN)
            } else {
                Style::default().fg(colors::FG)
            };

            // Cursor indicator
            let prefix = if is_cursor { "▸ " } else { "  " };
            let prefix_style = if is_cursor && is_ticker_focused {
                Style::default().fg(colors::YELLOW)
            } else {
                Style::default().fg(colors::FG_DARK)
            };

            // Checkbox
            let checkbox = if is_checked { "[✓] " } else { "[ ] " };
            let checkbox_style = if is_checked {
                Style::default().fg(colors::GREEN)
            } else {
                Style::default().fg(colors::FG_DARK)
            };

            // Cache status
            let cache_indicator = if is_loaded {
                Span::styled(" ●", Style::default().fg(colors::GREEN))
            } else {
                Span::styled(" ○", Style::default().fg(colors::FG_DARK))
            };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(checkbox, checkbox_style),
                Span::styled(ticker.clone(), style),
                cache_indicator,
            ]))
        })
        .collect();

    let selected_in_sector = app.data.selected_count_in_sector(&sector.id);
    let title = format!(
        "{} [{}/{}]",
        sector.name,
        selected_in_sector,
        sector.tickers.len()
    );

    let list = List::new(items).block(panel_block(&title, is_ticker_focused));
    f.render_widget(list, chunks[0]);

    // Ticker details/help at bottom
    draw_ticker_help(f, app, chunks[1], is_ticker_focused);
}

/// Draw help text for ticker panel
fn draw_ticker_help(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let mut lines = vec![];

    // Show focused ticker info if available
    if let Some(ticker) = app.data.focused_ticker() {
        let is_loaded = app.data.bars_cache.contains_key(ticker);
        let is_selected = app.data.is_ticker_selected(ticker);

        lines.push(Line::from(vec![
            Span::styled("Ticker: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                ticker.as_str(),
                Style::default()
                    .fg(colors::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            if is_selected {
                Span::styled(" ✓ selected", Style::default().fg(colors::GREEN))
            } else {
                Span::styled("", Style::default())
            },
        ]));

        if is_loaded {
            if let Some(bars) = app.data.bars_cache.get(ticker) {
                let bar_count = bars.len();
                lines.push(Line::from(vec![
                    Span::styled("Bars: ", Style::default().fg(colors::FG_DARK)),
                    Span::styled(
                        format!("{}", bar_count),
                        Style::default().fg(colors::YELLOW),
                    ),
                    Span::styled(" loaded", Style::default().fg(colors::GREEN)),
                ]));
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "Data not loaded",
                Style::default().fg(colors::FG_DARK),
            )]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Space: Toggle  a: All  n: None  ←: Sectors  f: Fetch",
        Style::default().fg(colors::FG_DARK),
    )]));

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(if is_active {
            colors::BLUE
        } else {
            colors::BORDER
        }));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

/// Draw sector summary when in Sectors view mode
fn draw_sector_summary(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let mut lines = vec![];

    // Header
    lines.push(Line::from(vec![
        Span::styled("Universe: ", Style::default().fg(colors::FG_DARK)),
        Span::styled(
            &app.data.universe.name,
            Style::default()
                .fg(colors::CYAN)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // Selection summary
    let total_selected = app.data.selected_tickers.len();
    let total_tickers: usize = app
        .data
        .universe
        .sectors
        .iter()
        .map(|s| s.tickers.len())
        .sum();

    lines.push(Line::from(vec![
        Span::styled("Selected: ", Style::default().fg(colors::FG_DARK)),
        Span::styled(
            format!("{}/{}", total_selected, total_tickers),
            Style::default().fg(if total_selected > 0 {
                colors::GREEN
            } else {
                colors::YELLOW
            }),
        ),
        Span::styled(" tickers", Style::default().fg(colors::FG_DARK)),
    ]));
    lines.push(Line::from(""));

    // Show selected tickers by sector
    if total_selected > 0 {
        lines.push(Line::from(vec![Span::styled(
            "Selected by sector:",
            Style::default().fg(colors::FG).add_modifier(Modifier::BOLD),
        )]));

        for sector in &app.data.universe.sectors {
            let count = app.data.selected_count_in_sector(&sector.id);
            if count > 0 {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        format!("{}: ", sector.name),
                        Style::default().fg(colors::FG_DARK),
                    ),
                    Span::styled(format!("{}", count), Style::default().fg(colors::GREEN)),
                ]));
            }
        }
    } else {
        lines.push(Line::from(vec![Span::styled(
            "No tickers selected yet.",
            Style::default().fg(colors::FG_DARK),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Press → or Enter to browse tickers",
            Style::default().fg(colors::FG_DARK),
        )]));
    }

    // Operation status
    if let OperationState::FetchingData {
        current_symbol,
        completed,
        total,
    } = &app.operation
    {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            format!("Fetching {} ({}/{})", current_symbol, completed + 1, total),
            Style::default().fg(colors::ORANGE),
        )]));
    }

    // Help text
    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Navigation:",
        Style::default().fg(colors::FG).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "  ↑↓: Select sector  →/Enter: Browse tickers",
        Style::default().fg(colors::FG_DARK),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "  a: Select ALL  n: Deselect ALL  f: Fetch",
        Style::default().fg(colors::FG_DARK),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "  s: Search symbols",
        Style::default().fg(colors::FG_DARK),
    )]));

    let para = Paragraph::new(lines).block(panel_block("Selection Summary", is_active));
    f.render_widget(para, area);
}

/// Draw the search overlay when in search mode.
fn draw_search_overlay(f: &mut Frame, app: &App, area: Rect) {
    // Build search UI
    let mut lines = vec![];

    // Search input line
    let cursor = if app.data.search_loading {
        "..."
    } else {
        "▎"
    };
    lines.push(Line::from(vec![
        Span::styled("Search: ", Style::default().fg(colors::CYAN)),
        Span::styled(
            &app.data.search_input,
            Style::default()
                .fg(colors::YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(cursor, Style::default().fg(colors::YELLOW)),
    ]));

    lines.push(Line::from(""));

    // Suggestions
    if app.data.search_suggestions.is_empty() {
        if app.data.search_loading {
            lines.push(Line::from(vec![Span::styled(
                "  Searching...",
                Style::default().fg(colors::FG_DARK),
            )]));
        } else if !app.data.search_input.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "  No results found",
                Style::default().fg(colors::FG_DARK),
            )]));
        } else {
            lines.push(Line::from(vec![Span::styled(
                "  Start typing to search",
                Style::default().fg(colors::FG_DARK),
            )]));
        }
    } else {
        for (i, suggestion) in app.data.search_suggestions.iter().enumerate() {
            let is_selected = i == app.data.search_selected;

            let prefix = if is_selected { "▸ " } else { "  " };
            let prefix_style = if is_selected {
                Style::default().fg(colors::YELLOW)
            } else {
                Style::default().fg(colors::FG_DARK)
            };

            let symbol_style = if is_selected {
                Style::default()
                    .fg(colors::CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::CYAN)
            };

            let name_style = if is_selected {
                Style::default().fg(colors::FG)
            } else {
                Style::default().fg(colors::FG_DARK)
            };

            // Truncate name if too long
            let name = if suggestion.name.len() > 25 {
                format!("{}...", &suggestion.name[..22])
            } else {
                suggestion.name.clone()
            };

            let type_info = format!(" ({}/{})", suggestion.type_disp, suggestion.exchange);

            lines.push(Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(format!("{:<8}", suggestion.symbol), symbol_style),
                Span::styled(name, name_style),
                Span::styled(type_info, Style::default().fg(colors::FG_DARK)),
            ]));
        }
    }

    // Help text
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Enter: Select  Esc: Cancel  ↑↓: Navigate",
        Style::default().fg(colors::FG_DARK),
    )]));

    let block = Block::default()
        .title("Symbol Search")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::YELLOW))
        .style(Style::default().bg(colors::BG));

    let para = Paragraph::new(lines).block(block);

    f.render_widget(para, area);
}
