//! Main UI rendering for TrendLab TUI

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
    Frame,
};

use crate::app::{App, Panel, YoloConfigField};
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

    // Chart enhancement colors
    /// Muted green for volume bars (bullish)
    pub const VOLUME_UP: Color = Color::Rgb(40, 80, 50);
    /// Muted red for volume bars (bearish)
    pub const VOLUME_DOWN: Color = Color::Rgb(80, 40, 45);
    /// Grid line color (very subtle)
    pub const GRID: Color = Color::Rgb(45, 50, 65);
    /// Crosshair color (dim)
    pub const CROSSHAIR: Color = Color::Rgb(80, 85, 110);
    /// Tooltip background
    pub const TOOLTIP_BG: Color = Color::Rgb(35, 38, 52);
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
        Panel::Results => {
            // Show analysis panel if toggled, otherwise show normal results
            if app.results.show_analysis {
                panels::analysis::draw(f, app, content_area);
            } else {
                panels::results::draw(f, app, content_area);
            }
        }
        Panel::Chart => panels::chart::draw(f, app, content_area),
        Panel::Help => panels::help::draw(f, app, content_area),
    }

    // Draw status bar
    draw_status(f, app, chunks[2]);

    // Startup modal overlay (draw last so it's on top)
    if app.startup.active {
        draw_startup_modal(f, app);
    }

    // YOLO config modal overlay
    if app.yolo.show_config {
        draw_yolo_config_modal(f, app);
    }
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
        Panel::Data => "↑↓: Select symbol  Enter: Load data  R: Reset defaults  Tab: Next panel",
        Panel::Strategy => "↑↓: Select field  ←→: Adjust value  R: Reset defaults  Tab: Next panel",
        Panel::Sweep => "Enter: Start sweep  Esc: Cancel  ↑↓: Select param  R: Reset defaults",
        Panel::Results => {
            "↑↓: Select result  Enter: View chart  R: Reset defaults  Tab: Next panel"
        }
        Panel::Chart => {
            "←→: Scroll  ↑↓: Zoom  m: Mode  v: Volume  c: Crosshair  d: Drawdown  R: Reset defaults"
        }
        Panel::Help => {
            if app.help.search_mode {
                "Type to search  Enter: Confirm  Esc: Cancel  n/N: Next/Prev match"
            } else {
                "←→: Section  j/k: Scroll  /: Search  gg/G: Top/Bottom  Ctrl+d/u: Page"
            }
        }
    };

    let mut spans = vec![Span::styled(
        &app.status_message,
        Style::default().fg(colors::GREEN),
    )];
    if app.random_defaults.enabled {
        spans.push(Span::styled(
            format!("  [seed {}]", app.random_defaults.seed),
            Style::default().fg(colors::YELLOW),
        ));
    }
    spans.push(Span::styled(" | ", Style::default().fg(colors::FG_DARK)));
    spans.push(Span::styled(
        help_text,
        Style::default().fg(colors::FG_DARK),
    ));

    let status_line = Line::from(spans);

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

fn draw_startup_modal(f: &mut Frame, app: &App) {
    use crate::app::{StartupMode, StrategySelection};
    use trendlab_core::SweepDepth;

    // Centered popup
    let area = centered_rect(80, 70, f.area());

    let mode = app.startup.mode;
    let left_style = if mode == StartupMode::Manual {
        Style::default()
            .fg(colors::GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::FG_DARK)
    };
    let right_style = if mode == StartupMode::FullAuto {
        Style::default()
            .fg(colors::GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::FG_DARK)
    };

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                "Startup",
                Style::default()
                    .fg(colors::MAGENTA)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  —  choose mode", Style::default().fg(colors::FG_DARK)),
        ]),
        Line::from(""),
    ];

    lines.push(Line::from(vec![
        Span::styled("Mode: ", Style::default().fg(colors::FG_DARK)),
        Span::styled("Manual", left_style),
        Span::styled("  |  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("Full-Auto", right_style),
    ]));
    lines.push(Line::from(vec![
        Span::styled("←→", Style::default().fg(colors::CYAN)),
        Span::styled(" to change mode, ", Style::default().fg(colors::FG_DARK)),
        Span::styled("Enter", Style::default().fg(colors::YELLOW)),
        Span::styled(" to continue, ", Style::default().fg(colors::FG_DARK)),
        Span::styled("Esc", Style::default().fg(colors::RED)),
        Span::styled(" to dismiss", Style::default().fg(colors::FG_DARK)),
    ]));
    lines.push(Line::from(""));

    if mode == StartupMode::FullAuto {
        lines.push(Line::from(vec![Span::styled(
            "Strategy: ",
            Style::default().fg(colors::FG_DARK),
        )]));

        // Show strategy selection list
        let options = StrategySelection::all_options();
        for (i, opt) in options.iter().enumerate() {
            let is_selected = i == app.startup.selected_strategy_index;
            let marker = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::FG_DARK)
            };

            // Add description for All Strategies
            let name = opt.name();
            let suffix = if matches!(opt, StrategySelection::AllStrategies) {
                " (compare all strategies)"
            } else {
                ""
            };

            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(name, style),
                Span::styled(
                    suffix,
                    Style::default()
                        .fg(colors::FG_DARK)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(colors::CYAN)),
            Span::styled(" to choose strategy", Style::default().fg(colors::FG_DARK)),
        ]));
        lines.push(Line::from(""));

        // Sweep depth selector
        lines.push(Line::from(vec![Span::styled(
            "Sweep Depth:",
            Style::default().fg(colors::FG_DARK),
        )]));

        // Show depth options horizontally
        let depths = SweepDepth::all();
        let mut depth_spans = vec![Span::raw("  ")];
        for (i, depth) in depths.iter().enumerate() {
            if i > 0 {
                depth_spans.push(Span::styled("  |  ", Style::default().fg(colors::FG_DARK)));
            }
            let is_selected = *depth == app.startup.sweep_depth;
            let style = if is_selected {
                Style::default()
                    .fg(colors::GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::FG_DARK)
            };
            depth_spans.push(Span::styled(depth.name(), style));
        }
        lines.push(Line::from(depth_spans));

        // Show estimated config count for selected depth
        lines.push(Line::from(vec![
            Span::styled("  ~", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{}", app.startup.sweep_depth.estimated_configs()),
                Style::default().fg(colors::ORANGE),
            ),
            Span::styled(" configurations", Style::default().fg(colors::FG_DARK)),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("[ ]", Style::default().fg(colors::CYAN)),
            Span::styled(
                " to change sweep depth",
                Style::default().fg(colors::FG_DARK),
            ),
        ]));
        lines.push(Line::from(""));

        // Show description based on selection
        let desc = if matches!(
            app.startup.strategy_selection,
            StrategySelection::AllStrategies
        ) {
            "Full-Auto will: select all tickers → run all 5 strategies → show strategy comparison chart."
        } else {
            "Full-Auto will: select all tickers → run selected strategy → show combined chart."
        };
        lines.push(Line::from(vec![Span::styled(
            desc,
            Style::default().fg(colors::FG),
        )]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            "Manual mode: use panels to pick tickers, configure strategy, then run sweeps.",
            Style::default().fg(colors::FG),
        )]));
    }

    // Clear the area first to avoid overlap with underlying panels
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_ACTIVE))
        .style(Style::default().bg(colors::BG)) // Add background color
        .title(Span::styled(
            " Start ",
            Style::default()
                .fg(colors::BLUE)
                .add_modifier(Modifier::BOLD),
        ));

    let para = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(colors::FG).bg(colors::BG));
    f.render_widget(para, area);
}

/// Create a centered rect using percentage-based constraints.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    let vertical = popup_layout[1];
    let horizontal_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical);

    horizontal_layout[1]
}

fn draw_yolo_config_modal(f: &mut Frame, app: &App) {
    use trendlab_core::SweepDepth;

    // Centered popup (smaller than startup modal)
    let area = centered_rect(60, 50, f.area());

    let config = &app.yolo.config;
    let focused = config.focused_field;

    let mut lines: Vec<Line> = vec![
        Line::from(vec![Span::styled(
            "YOLO Mode Configuration",
            Style::default()
                .fg(colors::MAGENTA)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ];

    // Start Date field
    let start_date_style = if focused == YoloConfigField::StartDate {
        Style::default()
            .fg(colors::GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::FG)
    };
    let start_marker = if focused == YoloConfigField::StartDate {
        "▶ "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::styled(start_marker, start_date_style),
        Span::styled("Start Date: ", Style::default().fg(colors::FG_DARK)),
        Span::styled(
            config.start_date.format("%Y-%m-%d").to_string(),
            start_date_style,
        ),
    ]));

    // End Date field
    let end_date_style = if focused == YoloConfigField::EndDate {
        Style::default()
            .fg(colors::GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::FG)
    };
    let end_marker = if focused == YoloConfigField::EndDate {
        "▶ "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::styled(end_marker, end_date_style),
        Span::styled("End Date:   ", Style::default().fg(colors::FG_DARK)),
        Span::styled(
            config.end_date.format("%Y-%m-%d").to_string(),
            end_date_style,
        ),
    ]));

    // Randomization percentage
    let random_style = if focused == YoloConfigField::Randomization {
        Style::default()
            .fg(colors::GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::FG)
    };
    let random_marker = if focused == YoloConfigField::Randomization {
        "▶ "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::styled(random_marker, random_style),
        Span::styled("Randomize:  ", Style::default().fg(colors::FG_DARK)),
        Span::styled(
            format!("{:.0}%", config.randomization_pct * 100.0),
            random_style,
        ),
    ]));

    // Sweep depth
    let depth_style = if focused == YoloConfigField::SweepDepth {
        Style::default()
            .fg(colors::GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors::FG)
    };
    let depth_marker = if focused == YoloConfigField::SweepDepth {
        "▶ "
    } else {
        "  "
    };
    lines.push(Line::from(vec![
        Span::styled(depth_marker, depth_style),
        Span::styled("Sweep Depth:", Style::default().fg(colors::FG_DARK)),
        Span::styled(format!(" {}", config.sweep_depth.name()), depth_style),
    ]));

    lines.push(Line::from(""));

    // Show sweep depth options horizontally when focused
    if focused == YoloConfigField::SweepDepth {
        let depths = SweepDepth::all();
        let mut depth_spans = vec![Span::raw("    ")];
        for (i, depth) in depths.iter().enumerate() {
            if i > 0 {
                depth_spans.push(Span::styled("  |  ", Style::default().fg(colors::FG_DARK)));
            }
            let is_selected = *depth == config.sweep_depth;
            let style = if is_selected {
                Style::default()
                    .fg(colors::CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::FG_DARK)
            };
            depth_spans.push(Span::styled(depth.name(), style));
        }
        lines.push(Line::from(depth_spans));
        lines.push(Line::from(""));
    }

    // Help text
    lines.push(Line::from(vec![
        Span::styled("↑↓", Style::default().fg(colors::CYAN)),
        Span::styled(" navigate  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("←→", Style::default().fg(colors::CYAN)),
        Span::styled(" adjust value", Style::default().fg(colors::FG_DARK)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Enter", Style::default().fg(colors::YELLOW)),
        Span::styled(" start YOLO  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("Esc", Style::default().fg(colors::RED)),
        Span::styled(" cancel", Style::default().fg(colors::FG_DARK)),
    ]));

    lines.push(Line::from(""));

    // Show what YOLO will do
    lines.push(Line::from(vec![Span::styled(
        format!(
            "Will sweep {} from {} to {}",
            config.sweep_depth.name().to_lowercase(),
            config.start_date.format("%Y-%m-%d"),
            config.end_date.format("%Y-%m-%d")
        ),
        Style::default().fg(colors::FG_DARK),
    )]));
    if config.randomization_pct > 0.0 {
        lines.push(Line::from(vec![Span::styled(
            format!(
                "with ±{:.0}% date randomization",
                config.randomization_pct * 100.0
            ),
            Style::default().fg(colors::ORANGE),
        )]));
    }

    // Clear the area first
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_ACTIVE))
        .style(Style::default().bg(colors::BG))
        .title(Span::styled(
            " YOLO Config ",
            Style::default()
                .fg(colors::BLUE)
                .add_modifier(Modifier::BOLD),
        ));

    let para = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(colors::FG).bg(colors::BG));
    f.render_widget(para, area);
}
