//! Sweep panel - run parameter sweeps

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Panel};
use crate::ui::{colors, panel_block};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Sweep;

    // Split vertically: param grid, progress, controls
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),   // Parameter grid
            Constraint::Length(5), // Progress
            Constraint::Length(5), // Controls
        ])
        .split(area);

    // Parameter grid
    draw_param_grid(f, app, chunks[0], is_active);

    // Progress bar
    draw_progress(f, app, chunks[1], is_active);

    // Controls
    draw_controls(f, app, chunks[2], is_active);
}

fn draw_param_grid(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let mut all_items: Vec<ListItem> = Vec::new();

    // Show current strategy
    let strat_name = app.strategy.selected_type.name();
    all_items.push(ListItem::new(Line::from(vec![
        Span::styled("  Strategy: ", Style::default().fg(colors::FG_DARK)),
        Span::styled(
            strat_name,
            Style::default()
                .fg(colors::MAGENTA)
                .add_modifier(Modifier::BOLD),
        ),
    ])));

    // Show data status
    let data_status = if let Some(sym) = app.data.selected_symbol() {
        if app.data.bars_cache.contains_key(sym) {
            let bars = app.data.bars_cache.get(sym).unwrap();
            format!("{}: {} bars", sym, bars.len())
        } else {
            format!("{}: not loaded", sym)
        }
    } else {
        "No symbol selected".to_string()
    };
    let data_loaded = app
        .data
        .selected_symbol()
        .map(|s| app.data.bars_cache.contains_key(s))
        .unwrap_or(false);

    all_items.push(ListItem::new(Line::from(vec![
        Span::styled("  Data: ", Style::default().fg(colors::FG_DARK)),
        Span::styled(
            &data_status,
            Style::default().fg(if data_loaded {
                colors::GREEN
            } else {
                colors::RED
            }),
        ),
    ])));
    all_items.push(ListItem::new(Line::from("")));

    // Parameter ranges
    for (i, (name, values)) in app.sweep.param_ranges.iter().enumerate() {
        let is_selected = is_active && i == app.sweep.selected_param;

        let name_style = if is_selected {
            Style::default()
                .fg(colors::BLUE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::FG)
        };

        let values_str = values.join(", ");
        let values_style = Style::default().fg(colors::CYAN);

        all_items.push(ListItem::new(Line::from(vec![
            Span::styled(
                if is_selected { "▸ " } else { "  " },
                Style::default().fg(colors::YELLOW),
            ),
            Span::styled(format!("{:<18}", name), name_style),
            Span::styled("[", Style::default().fg(colors::FG_DARK)),
            Span::styled(values_str, values_style),
            Span::styled("]", Style::default().fg(colors::FG_DARK)),
        ])));
    }

    // Add total configs calculation
    let total_configs: usize = app
        .sweep
        .param_ranges
        .iter()
        .map(|(_, v)| v.len())
        .product();

    all_items.push(ListItem::new(Line::from("")));
    all_items.push(ListItem::new(Line::from(vec![
        Span::styled(
            "  Total configurations: ",
            Style::default().fg(colors::FG_DARK),
        ),
        Span::styled(
            format!("{}", total_configs),
            Style::default()
                .fg(colors::YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
    ])));

    let list = List::new(all_items).block(panel_block("Parameter Grid", is_active));

    f.render_widget(list, area);
}

fn draw_progress(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let (label, ratio) = if app.sweep.is_running {
        let pct = (app.sweep.progress * 100.0) as u16;
        (
            format!(
                "Running... {}/{} ({pct}%)",
                app.sweep.completed_configs, app.sweep.total_configs
            ),
            app.sweep.progress,
        )
    } else if app.sweep.completed_configs > 0 {
        ("Sweep complete!".to_string(), 1.0)
    } else {
        ("Ready to sweep".to_string(), 0.0)
    };

    let gauge_color = if app.sweep.is_running {
        colors::YELLOW
    } else if app.sweep.completed_configs > 0 {
        colors::GREEN
    } else {
        colors::FG_DARK
    };

    let gauge = Gauge::default()
        .block(panel_block("Progress", is_active))
        .gauge_style(Style::default().fg(gauge_color))
        .label(Span::styled(label, Style::default().fg(colors::FG)))
        .ratio(ratio);

    f.render_widget(gauge, area);
}

fn draw_controls(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let action = if app.sweep.is_running {
        vec![
            Span::styled("Press ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(colors::RED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to cancel sweep", Style::default().fg(colors::FG_DARK)),
        ]
    } else {
        vec![
            Span::styled("Press ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(colors::GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to start sweep", Style::default().fg(colors::FG_DARK)),
        ]
    };

    let lines = vec![
        Line::from(action),
        Line::from(vec![
            Span::styled("Use ", Style::default().fg(colors::FG_DARK)),
            Span::styled("↑↓", Style::default().fg(colors::CYAN)),
            Span::styled(
                " to select parameter, ",
                Style::default().fg(colors::FG_DARK),
            ),
            Span::styled("←→", Style::default().fg(colors::CYAN)),
            Span::styled(" to adjust grid", Style::default().fg(colors::FG_DARK)),
        ]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Controls", is_active));

    f.render_widget(para, area);
}
