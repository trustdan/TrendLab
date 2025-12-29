//! Strategy panel - configure strategy type and parameters
//!
//! Displays strategies grouped by category with expandable checkboxes:
//! - Channel Breakouts: Donchian, Keltner, STARC, Bollinger, Supertrend
//! - Momentum/Direction: MA Crossover, TSMOM, DMI/ADX, Aroon, Heikin-Ashi
//! - Price Breakouts: Darvas, 52-Week High, Larry Williams, Opening Range
//! - Classic Presets: Turtle S1, Turtle S2, Parabolic SAR

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};

use trendlab_engine::app::{App, Panel, StrategyCategory, StrategyFocus, StrategyType};
use crate::ui::{colors, panel_block};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Strategy;
    let left_active = is_active && app.strategy.focus == StrategyFocus::Selection;
    let right_active = is_active && app.strategy.focus == StrategyFocus::Parameters;

    // Split into strategy selector and parameter config
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Strategy selection with grouped checkboxes
    draw_strategy_selector(f, app, chunks[0], left_active);

    // Parameter configuration
    draw_parameters(f, app, chunks[1], right_active);
}

fn draw_strategy_selector(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let mut items: Vec<ListItem> = Vec::new();
    let categories = StrategyCategory::all();

    for (cat_idx, category) in categories.iter().enumerate() {
        let is_expanded = app.strategy.expanded_categories.contains(category);
        let strategies = category.strategies();
        let selected_count = app.strategy.selected_count_in_category(*category);
        let total_count = strategies.len();

        // Is this category row focused?
        let is_cat_focused = is_active
            && app.strategy.focus_on_category
            && app.strategy.focused_category_index == cat_idx;

        // Category header row
        let expand_icon = if is_expanded { "▼" } else { "▶" };
        let cat_style = if is_cat_focused {
            Style::default()
                .fg(colors::YELLOW)
                .add_modifier(Modifier::BOLD)
        } else if is_expanded {
            Style::default().fg(colors::CYAN)
        } else {
            Style::default().fg(colors::FG_DARK)
        };

        let count_style = if selected_count > 0 {
            Style::default().fg(colors::GREEN)
        } else {
            Style::default().fg(colors::FG_DARK)
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{} ", expand_icon), cat_style),
            Span::styled(category.name(), cat_style),
            Span::styled(
                format!(" ({}/{})", selected_count, total_count),
                count_style,
            ),
        ])));

        // Strategy rows (only if expanded)
        if is_expanded {
            for (strat_idx, strat) in strategies.iter().enumerate() {
                let is_checked = app.strategy.selected_strategies.contains(strat);

                // Is this strategy row focused?
                let is_strat_focused = is_active
                    && !app.strategy.focus_on_category
                    && app.strategy.focused_category_index == cat_idx
                    && app.strategy.focused_strategy_in_category == strat_idx;

                let checkbox = if is_checked { "[x]" } else { "[ ]" };
                let checkbox_style = if is_checked {
                    Style::default().fg(colors::GREEN)
                } else {
                    Style::default().fg(colors::FG_DARK)
                };

                let name_style = if is_strat_focused {
                    Style::default()
                        .fg(colors::YELLOW)
                        .add_modifier(Modifier::BOLD)
                } else if is_checked {
                    Style::default().fg(colors::CYAN)
                } else {
                    Style::default().fg(colors::FG_DARK)
                };

                items.push(ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(checkbox, checkbox_style),
                    Span::raw(" "),
                    Span::styled(strat.name(), name_style),
                ])));
            }
        }
    }

    // Separator and ensemble toggle
    items.push(ListItem::new(Line::from("")));
    items.push(ListItem::new(Line::from(vec![Span::styled(
        "─".repeat(30),
        Style::default().fg(colors::FG_DARK),
    )])));

    let ensemble_checkbox = if app.strategy.ensemble.enabled {
        "[x]"
    } else {
        "[ ]"
    };
    let ensemble_style = if app.strategy.ensemble.enabled {
        Style::default().fg(colors::GREEN)
    } else {
        Style::default().fg(colors::FG_DARK)
    };

    items.push(ListItem::new(Line::from(vec![
        Span::styled(ensemble_checkbox, ensemble_style),
        Span::raw(" "),
        Span::styled("Ensemble Mode (e)", ensemble_style),
    ])));

    // Help text
    items.push(ListItem::new(Line::from("")));
    items.push(ListItem::new(Line::from(vec![Span::styled(
        "Space:toggle Enter:expand a/n:all",
        Style::default().fg(colors::FG_DARK),
    )])));

    let list = List::new(items).block(panel_block("Strategy Selection", is_active));

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
        StrategyType::Keltner => {
            vec![
                (
                    "EMA Period",
                    format!("{}", app.strategy.keltner_config.ema_period),
                    "EMA period for center line",
                ),
                (
                    "ATR Period",
                    format!("{}", app.strategy.keltner_config.atr_period),
                    "ATR period for band width",
                ),
                (
                    "Multiplier",
                    format!("{:.1}", app.strategy.keltner_config.multiplier),
                    "ATR multiplier for bands",
                ),
            ]
        }
        StrategyType::STARC => {
            vec![
                (
                    "SMA Period",
                    format!("{}", app.strategy.starc_config.sma_period),
                    "SMA period for center line",
                ),
                (
                    "ATR Period",
                    format!("{}", app.strategy.starc_config.atr_period),
                    "ATR period for band width",
                ),
                (
                    "Multiplier",
                    format!("{:.1}", app.strategy.starc_config.multiplier),
                    "ATR multiplier for bands",
                ),
            ]
        }
        StrategyType::Supertrend => {
            vec![
                (
                    "ATR Period",
                    format!("{}", app.strategy.supertrend_config.atr_period),
                    "ATR period for trend calculation",
                ),
                (
                    "Multiplier",
                    format!("{:.1}", app.strategy.supertrend_config.multiplier),
                    "ATR multiplier for sensitivity",
                ),
            ]
        }
        StrategyType::ParabolicSar => {
            vec![
                (
                    "AF Start",
                    format!("{:.2}", app.strategy.parabolic_sar_config.af_start),
                    "Initial acceleration factor",
                ),
                (
                    "AF Step",
                    format!("{:.2}", app.strategy.parabolic_sar_config.af_step),
                    "AF increment on new extremes",
                ),
                (
                    "AF Max",
                    format!("{:.2}", app.strategy.parabolic_sar_config.af_max),
                    "Maximum acceleration factor",
                ),
            ]
        }
        StrategyType::OpeningRange => {
            vec![
                (
                    "Range Bars",
                    format!("{}", app.strategy.opening_range_config.range_bars),
                    "Bars in opening range",
                ),
                (
                    "Period",
                    app.strategy.opening_range_config.period_name().to_string(),
                    "Weekly/Monthly/Rolling",
                ),
            ]
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

    let default_value: Option<String> = if is_fixed {
        None
    } else {
        match strat_type {
            StrategyType::Donchian => match selected_field {
                0 => Some("20".to_string()),
                1 => Some("10".to_string()),
                _ => None,
            },
            StrategyType::MACrossover => match selected_field {
                0 => Some("10".to_string()),
                1 => Some("50".to_string()),
                2 => Some("SMA".to_string()),
                _ => None,
            },
            StrategyType::Tsmom => match selected_field {
                0 => Some("252".to_string()),
                _ => None,
            },
            StrategyType::Keltner => match selected_field {
                0 => Some("20".to_string()),
                1 => Some("10".to_string()),
                2 => Some("2.0".to_string()),
                _ => None,
            },
            StrategyType::STARC => match selected_field {
                0 => Some("20".to_string()),
                1 => Some("15".to_string()),
                2 => Some("2.0".to_string()),
                _ => None,
            },
            StrategyType::Supertrend => match selected_field {
                0 => Some("10".to_string()),
                1 => Some("3.0".to_string()),
                _ => None,
            },
            StrategyType::ParabolicSar => match selected_field {
                0 => Some("0.02".to_string()),
                1 => Some("0.02".to_string()),
                2 => Some("0.20".to_string()),
                _ => None,
            },
            StrategyType::OpeningRange => match selected_field {
                0 => Some("5".to_string()),
                1 => Some("Weekly".to_string()),
                _ => None,
            },
            StrategyType::TurtleS1 | StrategyType::TurtleS2 => None,
        }
    };

    let seed_note = if app.random_defaults.enabled {
        Some(format!("seed {}", app.random_defaults.seed))
    } else {
        None
    };
    let mut meta_bits: Vec<String> = Vec::new();
    if let Some(def) = default_value {
        meta_bits.push(format!("default {}", def));
    }
    if let Some(seed) = seed_note {
        meta_bits.push(seed);
    }
    let meta = if meta_bits.is_empty() {
        String::new()
    } else {
        format!("  ({})", meta_bits.join(" • "))
    };

    let desc_item = ListItem::new(Line::from(""));
    let desc_item2 = ListItem::new(Line::from(vec![Span::styled(
        format!("  ℹ {}{}  [Press R to reset defaults]", description, meta),
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
