//! Results panel - view backtest results with per-ticker and aggregated views

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
    Frame,
};

use crate::app::{App, Panel, ResultsViewMode};
use crate::ui::{colors, panel_block};
use trendlab_core::ConfidenceGrade;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    // Split for table and help text
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(area);

    let is_active = app.active_panel == Panel::Results;

    // Draw based on view mode
    match app.results.view_mode {
        ResultsViewMode::SingleSymbol => {
            if app.results.results.is_empty() {
                draw_empty_state(f, app, chunks[0], is_active);
            } else {
                draw_single_symbol_table(f, app, chunks[0], is_active);
            }
        }
        ResultsViewMode::PerTicker => {
            if app.results.ticker_summaries.is_empty() {
                draw_no_multi_sweep_state(f, chunks[0], is_active);
            } else {
                draw_per_ticker_table(f, app, chunks[0], is_active);
            }
        }
        ResultsViewMode::Aggregated => {
            draw_aggregated_view(f, app, chunks[0], is_active);
        }
        ResultsViewMode::Leaderboard => {
            draw_leaderboard_table(f, app, chunks[0], is_active);
        }
    }

    // Help text
    draw_help(f, app, chunks[1], is_active);
}

fn draw_empty_state(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let has_yolo_results = !app.yolo.leaderboard().entries.is_empty()
        || app
            .yolo
            .cross_symbol_leaderboard()
            .is_some_and(|lb| !lb.entries.is_empty());

    let lines = if has_yolo_results {
        vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "No single-symbol results in this view.",
                Style::default().fg(colors::FG_DARK),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "YOLO results available! ",
                    Style::default().fg(colors::GREEN),
                ),
                Span::styled("Press ", Style::default().fg(colors::FG_DARK)),
                Span::styled("'v'", Style::default().fg(colors::CYAN)),
                Span::styled(
                    " to switch to Leaderboard view.",
                    Style::default().fg(colors::FG_DARK),
                ),
            ]),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "No results yet.",
                Style::default().fg(colors::FG_DARK),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Run a sweep in the Sweep panel (Tab to switch, Enter to start)",
                Style::default().fg(colors::FG_DARK),
            )]),
        ]
    };

    let para = Paragraph::new(lines).block(panel_block("Results", is_active));

    f.render_widget(para, area);
}

fn draw_no_multi_sweep_state(f: &mut Frame, area: Rect, is_active: bool) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "No multi-symbol sweep results.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Select multiple tickers in Data panel and run a sweep.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press 'v' to toggle view modes.",
            Style::default().fg(colors::FG_DARK),
        )]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Results - Per-Ticker", is_active));

    f.render_widget(para, area);
}

/// Draw single-symbol sweep results (original behavior)
fn draw_single_symbol_table(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let sort_col = app.results.sort_column;

    let header_style = Style::default()
        .fg(colors::MAGENTA)
        .add_modifier(Modifier::BOLD);

    let sort_indicator = |col: usize| {
        if col == sort_col {
            " \u{25bc}"
        } else {
            ""
        }
    };

    let header = Row::new(vec![
        Cell::from(format!("Entry{}", sort_indicator(0))).style(header_style),
        Cell::from(format!("Exit{}", sort_indicator(1))).style(header_style),
        Cell::from(format!("CAGR %{}", sort_indicator(0))).style(if sort_col == 0 {
            header_style.fg(colors::YELLOW)
        } else {
            header_style
        }),
        Cell::from(format!("Sharpe{}", sort_indicator(1))).style(if sort_col == 1 {
            header_style.fg(colors::YELLOW)
        } else {
            header_style
        }),
        Cell::from(format!("MaxDD %{}", sort_indicator(2))).style(if sort_col == 2 {
            header_style.fg(colors::YELLOW)
        } else {
            header_style
        }),
        Cell::from(format!("Trades{}", sort_indicator(3))).style(if sort_col == 3 {
            header_style.fg(colors::YELLOW)
        } else {
            header_style
        }),
        Cell::from("Win %").style(header_style),
    ]);

    let rows: Vec<Row> = app
        .results
        .results
        .iter()
        .enumerate()
        .map(|(i, result)| {
            let is_selected = i == app.results.selected_index;
            let base_style = if is_selected && is_active {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::FG)
            };

            // Rank indicator for top 3
            let rank_prefix = match i {
                0 => "\u{1f947} ",
                1 => "\u{1f948} ",
                2 => "\u{1f949} ",
                _ => "   ",
            };

            let cagr_val = result.metrics.cagr;
            let cagr_style = if cagr_val > 0.2 {
                Style::default().fg(colors::GREEN)
            } else if cagr_val < 0.0 {
                Style::default().fg(colors::RED)
            } else {
                base_style
            };

            let sharpe_val = result.metrics.sharpe;
            let sharpe_style = if sharpe_val > 1.0 {
                Style::default().fg(colors::GREEN)
            } else if sharpe_val < 0.5 {
                Style::default().fg(colors::ORANGE)
            } else {
                base_style
            };

            let dd_style = Style::default().fg(colors::RED);

            Row::new(vec![
                Cell::from(format!(
                    "{}{}",
                    rank_prefix, result.config_id.entry_lookback
                ))
                .style(base_style),
                Cell::from(format!("{}", result.config_id.exit_lookback)).style(base_style),
                Cell::from(format!("{:.1}", cagr_val * 100.0)).style(cagr_style),
                Cell::from(format!("{:.2}", sharpe_val)).style(sharpe_style),
                Cell::from(format!("{:.1}", result.metrics.max_drawdown * 100.0)).style(dd_style),
                Cell::from(format!("{}", result.metrics.num_trades)).style(base_style),
                Cell::from(format!("{:.0}", result.metrics.win_rate * 100.0)).style(base_style),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(6),
    ];

    let title = format!("Results ({} configs)", app.results.results.len());
    let table = Table::new(rows, widths)
        .header(header)
        .block(panel_block(&title, is_active))
        .row_highlight_style(
            Style::default()
                .fg(colors::BLUE)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(table, area);
}

/// Draw per-ticker results table
fn draw_per_ticker_table(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let header_style = Style::default()
        .fg(colors::MAGENTA)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Cell::from("Symbol").style(header_style),
        Cell::from("Entry").style(header_style),
        Cell::from("Exit").style(header_style),
        Cell::from("CAGR %").style(header_style),
        Cell::from("Sharpe").style(header_style),
        Cell::from("MaxDD %").style(header_style),
        Cell::from("Trades").style(header_style),
    ]);

    let rows: Vec<Row> = app
        .results
        .ticker_summaries
        .iter()
        .enumerate()
        .map(|(i, summary)| {
            let is_selected = i == app.results.selected_ticker_index;
            let base_style = if is_selected && is_active {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::FG)
            };

            let cagr_style = if summary.cagr > 0.2 {
                Style::default().fg(colors::GREEN)
            } else if summary.cagr < 0.0 {
                Style::default().fg(colors::RED)
            } else {
                base_style
            };

            let sharpe_style = if summary.sharpe > 1.0 {
                Style::default().fg(colors::GREEN)
            } else if summary.sharpe < 0.5 {
                Style::default().fg(colors::ORANGE)
            } else {
                base_style
            };

            let dd_style = Style::default().fg(colors::RED);

            // Cursor prefix
            let symbol_prefix = if is_selected { "\u{25b8} " } else { "  " };

            Row::new(vec![
                Cell::from(format!("{}{}", symbol_prefix, summary.symbol)).style(base_style),
                Cell::from(format!("{}", summary.best_config_entry)).style(base_style),
                Cell::from(format!("{}", summary.best_config_exit)).style(base_style),
                Cell::from(format!("{:.1}", summary.cagr * 100.0)).style(cagr_style),
                Cell::from(format!("{:.2}", summary.sharpe)).style(sharpe_style),
                Cell::from(format!("{:.1}", summary.max_drawdown * 100.0)).style(dd_style),
                Cell::from(format!("{}", summary.num_trades)).style(base_style),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(12),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(8),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(7),
    ];

    let title = format!(
        "Per-Ticker Results ({} symbols)",
        app.results.ticker_summaries.len()
    );
    let table = Table::new(rows, widths)
        .header(header)
        .block(panel_block(&title, is_active))
        .row_highlight_style(
            Style::default()
                .fg(colors::BLUE)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(table, area);
}

/// Draw aggregated portfolio view
fn draw_aggregated_view(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let mut lines = vec![];

    if let Some(ref multi) = app.results.multi_sweep_result {
        // Header
        lines.push(Line::from(vec![Span::styled(
            "Aggregated Portfolio Results",
            Style::default()
                .fg(colors::CYAN)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        // Symbol count
        let symbol_count = multi.symbol_results.len();
        lines.push(Line::from(vec![
            Span::styled("Symbols: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{}", symbol_count),
                Style::default().fg(colors::YELLOW),
            ),
        ]));

        // Total configs
        let total_configs: usize = multi
            .symbol_results
            .values()
            .map(|r| r.config_results.len())
            .sum();
        lines.push(Line::from(vec![
            Span::styled(
                "Total configs tested: ",
                Style::default().fg(colors::FG_DARK),
            ),
            Span::styled(
                format!("{}", total_configs),
                Style::default().fg(colors::YELLOW),
            ),
        ]));

        lines.push(Line::from(""));

        // Aggregated metrics (if available)
        if let Some(ref agg) = multi.aggregated {
            lines.push(Line::from(vec![Span::styled(
                "Portfolio Metrics:",
                Style::default().fg(colors::FG).add_modifier(Modifier::BOLD),
            )]));

            let cagr_style = if agg.metrics.cagr > 0.2 {
                Style::default().fg(colors::GREEN)
            } else if agg.metrics.cagr < 0.0 {
                Style::default().fg(colors::RED)
            } else {
                Style::default().fg(colors::YELLOW)
            };

            lines.push(Line::from(vec![
                Span::styled("  CAGR: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(format!("{:.1}%", agg.metrics.cagr * 100.0), cagr_style),
            ]));

            let sharpe_style = if agg.metrics.sharpe > 1.0 {
                Style::default().fg(colors::GREEN)
            } else if agg.metrics.sharpe < 0.5 {
                Style::default().fg(colors::ORANGE)
            } else {
                Style::default().fg(colors::YELLOW)
            };

            lines.push(Line::from(vec![
                Span::styled("  Sharpe: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(format!("{:.2}", agg.metrics.sharpe), sharpe_style),
            ]));

            lines.push(Line::from(vec![
                Span::styled("  Max DD: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.1}%", agg.metrics.max_drawdown * 100.0),
                    Style::default().fg(colors::RED),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled("  Trades: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{}", agg.metrics.num_trades),
                    Style::default().fg(colors::FG),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled("  Win Rate: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.0}%", agg.metrics.win_rate * 100.0),
                    Style::default().fg(colors::FG),
                ),
            ]));
        } else {
            // Compute summary stats from per-ticker results
            lines.push(Line::from(vec![Span::styled(
                "Per-Symbol Statistics:",
                Style::default().fg(colors::FG).add_modifier(Modifier::BOLD),
            )]));

            // Calculate average and best metrics
            let summaries = &app.results.ticker_summaries;
            if !summaries.is_empty() {
                let avg_cagr: f64 =
                    summaries.iter().map(|s| s.cagr).sum::<f64>() / summaries.len() as f64;
                let avg_sharpe: f64 =
                    summaries.iter().map(|s| s.sharpe).sum::<f64>() / summaries.len() as f64;
                let worst_dd = summaries
                    .iter()
                    .map(|s| s.max_drawdown)
                    .fold(0.0_f64, f64::min);

                let best_cagr = summaries
                    .iter()
                    .max_by(|a, b| a.cagr.partial_cmp(&b.cagr).unwrap());
                let best_sharpe = summaries
                    .iter()
                    .max_by(|a, b| a.sharpe.partial_cmp(&b.sharpe).unwrap());

                lines.push(Line::from(vec![
                    Span::styled("  Avg CAGR: ", Style::default().fg(colors::FG_DARK)),
                    Span::styled(
                        format!("{:.1}%", avg_cagr * 100.0),
                        Style::default().fg(colors::YELLOW),
                    ),
                ]));

                lines.push(Line::from(vec![
                    Span::styled("  Avg Sharpe: ", Style::default().fg(colors::FG_DARK)),
                    Span::styled(
                        format!("{:.2}", avg_sharpe),
                        Style::default().fg(colors::YELLOW),
                    ),
                ]));

                lines.push(Line::from(vec![
                    Span::styled("  Worst DD: ", Style::default().fg(colors::FG_DARK)),
                    Span::styled(
                        format!("{:.1}%", worst_dd * 100.0),
                        Style::default().fg(colors::RED),
                    ),
                ]));

                lines.push(Line::from(""));

                if let Some(best) = best_cagr {
                    lines.push(Line::from(vec![
                        Span::styled("  Best CAGR: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(
                            format!("{} ({:.1}%)", best.symbol, best.cagr * 100.0),
                            Style::default().fg(colors::GREEN),
                        ),
                    ]));
                }

                if let Some(best) = best_sharpe {
                    lines.push(Line::from(vec![
                        Span::styled("  Best Sharpe: ", Style::default().fg(colors::FG_DARK)),
                        Span::styled(
                            format!("{} ({:.2})", best.symbol, best.sharpe),
                            Style::default().fg(colors::GREEN),
                        ),
                    ]));
                }
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Press Enter to view portfolio equity curve in Chart panel",
            Style::default().fg(colors::FG_DARK),
        )]));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "No multi-symbol sweep results.",
            Style::default().fg(colors::FG_DARK),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Select multiple tickers and run a sweep first.",
            Style::default().fg(colors::FG_DARK),
        )]));
    }

    let para = Paragraph::new(lines).block(panel_block("Portfolio Summary", is_active));

    f.render_widget(para, area);
}

/// Draw YOLO leaderboard table (cross-symbol aggregated results)
fn draw_leaderboard_table(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    // Check if we have cross-symbol leaderboard data
    let cross_symbol = match app.yolo.cross_symbol_leaderboard() {
        Some(lb) if !lb.entries.is_empty() => lb,
        _ => {
            // Fallback: show per-symbol leaderboard if no cross-symbol data
            if app.yolo.leaderboard().entries.is_empty() {
                draw_empty_leaderboard(f, app, area, is_active);
                return;
            }
            draw_per_symbol_leaderboard(f, app, area, is_active);
            return;
        }
    };

    let header_style = Style::default()
        .fg(colors::MAGENTA)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Cell::from("#").style(header_style),
        Cell::from("Conf").style(header_style), // Confidence grade
        Cell::from("Strategy").style(header_style),
        Cell::from("Config").style(header_style),
        Cell::from("Syms").style(header_style),
        Cell::from("Avg Sharpe").style(header_style),
        Cell::from("Min Sharpe").style(header_style),
        Cell::from("Hit Rate").style(header_style),
        Cell::from("Avg CAGR%").style(header_style),
    ]);

    let rows: Vec<Row> = cross_symbol
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = i == app.results.selected_leaderboard_index;
            let base_style = if is_selected && is_active {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::FG)
            };

            // Rank indicator for top 3
            let rank_cell = match entry.rank {
                1 => Cell::from("\u{1f947}").style(base_style),
                2 => Cell::from("\u{1f948}").style(base_style),
                3 => Cell::from("\u{1f949}").style(base_style),
                r => Cell::from(format!("{}", r)).style(base_style),
            };

            let sharpe_style = if entry.aggregate_metrics.avg_sharpe > 1.0 {
                Style::default().fg(colors::GREEN)
            } else if entry.aggregate_metrics.avg_sharpe < 0.5 {
                Style::default().fg(colors::ORANGE)
            } else {
                base_style
            };

            let hit_style = if entry.aggregate_metrics.hit_rate > 0.7 {
                Style::default().fg(colors::GREEN)
            } else if entry.aggregate_metrics.hit_rate < 0.5 {
                Style::default().fg(colors::RED)
            } else {
                base_style
            };

            // Confidence badge with color
            let (conf_text, conf_style) = match entry.confidence_grade {
                Some(ConfidenceGrade::High) => ("✓✓", Style::default().fg(colors::GREEN)),
                Some(ConfidenceGrade::Medium) => ("✓", Style::default().fg(colors::YELLOW)),
                Some(ConfidenceGrade::Low) => ("○", Style::default().fg(colors::ORANGE)),
                Some(ConfidenceGrade::Insufficient) => ("?", Style::default().fg(colors::FG_DARK)),
                None => ("-", base_style),
            };

            Row::new(vec![
                rank_cell,
                Cell::from(conf_text).style(conf_style),
                Cell::from(entry.strategy_type.name()).style(base_style),
                Cell::from(truncate_config(&entry.config_id.display(), 18)).style(base_style),
                Cell::from(format!("{}", entry.symbols.len())).style(base_style),
                Cell::from(format!("{:.3}", entry.aggregate_metrics.avg_sharpe)).style(sharpe_style),
                Cell::from(format!("{:.3}", entry.aggregate_metrics.min_sharpe)).style(base_style),
                Cell::from(format!("{:.1}%", entry.aggregate_metrics.hit_rate * 100.0)).style(hit_style),
                Cell::from(format!("{:.1}%", entry.aggregate_metrics.avg_cagr * 100.0)).style(base_style),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(3),  // Rank
        Constraint::Length(4),  // Confidence
        Constraint::Length(14), // Strategy
        Constraint::Length(20), // Config
        Constraint::Length(5),  // Symbols
        Constraint::Length(10), // Avg Sharpe
        Constraint::Length(10), // Min Sharpe
        Constraint::Length(9),  // Hit Rate
        Constraint::Length(10), // Avg CAGR
    ];

    let scope_label = app.yolo.view_scope.display_name();
    let title = format!(
        "YOLO Leaderboard [{}] ({} configs)",
        scope_label,
        app.yolo.configs_tested()
    );
    let table = Table::new(rows, widths)
        .header(header)
        .block(panel_block(&title, is_active));

    f.render_widget(table, area);
}

/// Draw empty leaderboard state
fn draw_empty_leaderboard(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let scope_label = app.yolo.view_scope.display_name();
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("No {} YOLO results yet.", scope_label),
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Start YOLO mode in the Sweep panel to discover best configs.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press ", Style::default().fg(colors::FG_DARK)),
            Span::styled("'t'", Style::default().fg(colors::CYAN)),
            Span::styled(" to toggle Session/All-Time, ", Style::default().fg(colors::FG_DARK)),
            Span::styled("'Y'", Style::default().fg(colors::MAGENTA)),
            Span::styled(" to start YOLO mode.", Style::default().fg(colors::FG_DARK)),
        ]),
    ];

    let title = format!("YOLO Leaderboard [{}]", scope_label);
    let para = Paragraph::new(lines).block(panel_block(&title, is_active));
    f.render_widget(para, area);
}

/// Draw per-symbol leaderboard (fallback when no cross-symbol data)
fn draw_per_symbol_leaderboard(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let leaderboard = app.yolo.leaderboard();

    let header_style = Style::default()
        .fg(colors::MAGENTA)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Cell::from("#").style(header_style),
        Cell::from("Conf").style(header_style), // Confidence grade
        Cell::from("Strategy").style(header_style),
        Cell::from("Symbol").style(header_style),
        Cell::from("Sharpe").style(header_style),
        Cell::from("CAGR%").style(header_style),
        Cell::from("MaxDD%").style(header_style),
        Cell::from("Win%").style(header_style),
    ]);

    let rows: Vec<Row> = leaderboard
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = i == app.results.selected_leaderboard_index;
            let base_style = if is_selected && is_active {
                Style::default()
                    .fg(colors::BLUE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::FG)
            };

            let rank_cell = match entry.rank {
                1 => Cell::from("\u{1f947}").style(base_style),
                2 => Cell::from("\u{1f948}").style(base_style),
                3 => Cell::from("\u{1f949}").style(base_style),
                r => Cell::from(format!("{}", r)).style(base_style),
            };

            // Confidence badge with color
            let (conf_text, conf_style) = match entry.confidence_grade {
                Some(ConfidenceGrade::High) => ("✓✓", Style::default().fg(colors::GREEN)),
                Some(ConfidenceGrade::Medium) => ("✓", Style::default().fg(colors::YELLOW)),
                Some(ConfidenceGrade::Low) => ("○", Style::default().fg(colors::ORANGE)),
                Some(ConfidenceGrade::Insufficient) => ("?", Style::default().fg(colors::FG_DARK)),
                None => ("-", base_style),
            };

            let symbol = entry.symbol.as_deref().unwrap_or("-");
            Row::new(vec![
                rank_cell,
                Cell::from(conf_text).style(conf_style),
                Cell::from(entry.strategy_type.name()).style(base_style),
                Cell::from(symbol).style(base_style),
                Cell::from(format!("{:.3}", entry.metrics.sharpe)).style(base_style),
                Cell::from(format!("{:.1}%", entry.metrics.cagr * 100.0)).style(base_style),
                Cell::from(format!("{:.1}%", entry.metrics.max_drawdown * 100.0)).style(base_style),
                Cell::from(format!("{:.1}%", entry.metrics.win_rate * 100.0)).style(base_style),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(3),  // Rank
        Constraint::Length(4),  // Confidence
        Constraint::Length(14), // Strategy
        Constraint::Length(8),  // Symbol
        Constraint::Length(8),  // Sharpe
        Constraint::Length(8),  // CAGR
        Constraint::Length(8),  // MaxDD
        Constraint::Length(6),  // Win%
    ];

    let scope_label = app.yolo.view_scope.display_name();
    let title = format!(
        "Per-Symbol Leaderboard [{}] ({} configs)",
        scope_label,
        app.yolo.configs_tested()
    );
    let table = Table::new(rows, widths)
        .header(header)
        .block(panel_block(&title, is_active));

    f.render_widget(table, area);
}

/// Truncate a config string to fit in limited space
fn truncate_config(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn draw_help(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let view_mode = app.results.view_mode_name();
    let scope_label = app.yolo.view_scope.display_name();

    let lines = vec![Line::from(vec![
        Span::styled("\u{2191}\u{2193}: ", Style::default().fg(colors::CYAN)),
        Span::styled("Select  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("Enter: ", Style::default().fg(colors::CYAN)),
        Span::styled("Chart  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("'s': ", Style::default().fg(colors::CYAN)),
        Span::styled("Sort  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("'v': ", Style::default().fg(colors::CYAN)),
        Span::styled(
            format!("{}  ", view_mode),
            Style::default()
                .fg(colors::YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("'t': ", Style::default().fg(colors::CYAN)),
        Span::styled(
            scope_label,
            Style::default()
                .fg(colors::MAGENTA)
                .add_modifier(Modifier::BOLD),
        ),
    ])];

    let para = Paragraph::new(lines).block(panel_block("Controls", is_active));

    f.render_widget(para, area);
}
