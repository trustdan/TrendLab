//! Strategy panel - configure strategy type and parameters

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};

use crate::app::{App, Panel, StrategyType};
use crate::ui::{colors, panel_block};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Strategy;
    let left_active = is_active && app.strategy.editing_strategy;
    let right_active = is_active && !app.strategy.editing_strategy;

    // Split into strategy selector and parameter config
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Strategy type selector
    draw_strategy_selector(f, app, chunks[0], left_active);

    // Parameter configuration
    draw_parameters(f, app, chunks[1], right_active);
}

fn draw_strategy_selector(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let strategies = StrategyType::all();
    let selected_idx = app.strategy.selected_type_index;

    let items: Vec<ListItem> = strategies
        .iter()
        .enumerate()
        .map(|(i, strat)| {
            let is_selected = i == selected_idx;

            let style = if is_selected && is_active {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(colors::CYAN)
            } else {
                Style::default().fg(colors::FG_DARK)
            };

            let prefix = if is_selected { "● " } else { "○ " };
            let prefix_color = if is_selected {
                colors::GREEN
            } else {
                colors::FG_DARK
            };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(prefix_color)),
                Span::styled(strat.name(), style),
            ]))
        })
        .collect();

    // Add description of selected strategy
    let desc_line = ListItem::new(Line::from(""));
    let desc_line2 = ListItem::new(Line::from(vec![Span::styled(
        format!("  ℹ {}", app.strategy.selected_type.description()),
        Style::default().fg(colors::FG_DARK),
    )]));

    let mut all_items = items;
    all_items.push(desc_line);
    all_items.push(desc_line2);

    let list = List::new(all_items).block(panel_block("Strategy Type", is_active));

    f.render_widget(list, area);
}

fn draw_parameters(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let selected_field = app.strategy.selected_field;
    let strat_type = app.strategy.selected_type;

    // Build parameter list based on strategy type
    let params: Vec<(&str, String, &str)> = match strat_type {
        StrategyType::Donchian => {
            vec![
                (
                    "Entry Lookback",
                    format!("{}", app.strategy.donchian_config.entry_lookback),
                    "Days for upper channel breakout entry",
                ),
                (
                    "Exit Lookback",
                    format!("{}", app.strategy.donchian_config.exit_lookback),
                    "Days for lower channel exit",
                ),
            ]
        }
        StrategyType::TurtleS1 => {
            vec![
                ("Entry Lookback", "20".to_string(), "Fixed: 20-day high"),
                ("Exit Lookback", "10".to_string(), "Fixed: 10-day low"),
            ]
        }
        StrategyType::TurtleS2 => {
            vec![
                ("Entry Lookback", "55".to_string(), "Fixed: 55-day high"),
                ("Exit Lookback", "20".to_string(), "Fixed: 20-day low"),
            ]
        }
        StrategyType::MACrossover => {
            vec![
                (
                    "Fast Period",
                    format!("{}", app.strategy.ma_config.fast_period),
                    "Fast moving average period",
                ),
                (
                    "Slow Period",
                    format!("{}", app.strategy.ma_config.slow_period),
                    "Slow moving average period",
                ),
                (
                    "MA Type",
                    app.strategy.ma_config.ma_type_name().to_string(),
                    "SMA (simple) or EMA (exponential)",
                ),
            ]
        }
        StrategyType::Tsmom => {
            vec![(
                "Lookback",
                format!("{}", app.strategy.tsmom_config.lookback),
                "Days for momentum calculation",
            )]
        }
    };

    let is_fixed = matches!(strat_type, StrategyType::TurtleS1 | StrategyType::TurtleS2);

    let items: Vec<ListItem> = params
        .iter()
        .enumerate()
        .map(|(idx, (name, value, _))| {
            let is_selected = is_active && idx == selected_field && !is_fixed;

            let name_style = Style::default().fg(colors::FG_DARK);

            let value_style = if is_fixed {
                Style::default().fg(colors::FG_DARK) // Fixed params shown dimmed
            } else if is_selected {
                Style::default()
                    .fg(colors::YELLOW)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::CYAN)
            };

            let arrow_style = if is_selected && !is_fixed {
                Style::default().fg(colors::YELLOW)
            } else {
                Style::default().fg(colors::BG)
            };

            let prefix = if is_selected && !is_fixed {
                "▸ "
            } else {
                "  "
            };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, arrow_style),
                Span::styled(format!("{:<16}", name), name_style),
                Span::styled(if is_fixed { "  " } else { "◀ " }, arrow_style),
                Span::styled(format!("{:>8}", value), value_style),
                Span::styled(if is_fixed { "" } else { " ▶" }, arrow_style),
            ]))
        })
        .collect();

    // Add description of selected parameter
    let description = if is_fixed {
        "Parameters are fixed for this strategy preset"
    } else {
        params
            .get(selected_field)
            .map(|(_, _, desc)| *desc)
            .unwrap_or("")
    };

    let desc_item = ListItem::new(Line::from(""));
    let desc_item2 = ListItem::new(Line::from(vec![Span::styled(
        format!("  ℹ {}", description),
        Style::default().fg(colors::FG_DARK),
    )]));

    // Add help text
    let help_item = ListItem::new(Line::from(""));
    let help_text = if is_fixed {
        "Tab: Switch  ↑↓: Strategy"
    } else {
        "Tab: Switch  ↑↓: Select  ←→: Adjust"
    };
    let help_item2 = ListItem::new(Line::from(vec![Span::styled(
        format!("  {}", help_text),
        Style::default().fg(colors::FG_DARK),
    )]));

    let mut all_items = items;
    all_items.push(desc_item);
    all_items.push(desc_item2);
    all_items.push(help_item);
    all_items.push(help_item2);

    let list = List::new(all_items).block(panel_block("Parameters", is_active));

    f.render_widget(list, area);
}
