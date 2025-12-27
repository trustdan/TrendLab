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
                draw_empty_state(f, chunks[0], is_active);
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
    }

    // Help text
    draw_help(f, app, chunks[1], is_active);
}

fn draw_empty_state(f: &mut Frame, area: Rect, is_active: bool) {
    let lines = vec![
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
    ];

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
            Span::styled("Total configs tested: ", Style::default().fg(colors::FG_DARK)),
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
                Style::default()
                    .fg(colors::FG)
                    .add_modifier(Modifier::BOLD),
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
                Style::default()
                    .fg(colors::FG)
                    .add_modifier(Modifier::BOLD),
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

fn draw_help(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let view_mode = app.results.view_mode_name();

    let lines = vec![Line::from(vec![
        Span::styled("\u{2191}\u{2193}: ", Style::default().fg(colors::CYAN)),
        Span::styled("Select  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("Enter: ", Style::default().fg(colors::CYAN)),
        Span::styled("View chart  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("'s': ", Style::default().fg(colors::CYAN)),
        Span::styled("Sort  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("'v': ", Style::default().fg(colors::CYAN)),
        Span::styled("View ", Style::default().fg(colors::FG_DARK)),
        Span::styled(
            view_mode,
            Style::default()
                .fg(colors::YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
    ])];

    let para = Paragraph::new(lines).block(panel_block("Controls", is_active));

    f.render_widget(para, area);
}
