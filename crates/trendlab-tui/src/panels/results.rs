//! Results panel - view backtest results with per-ticker and aggregated views

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, Wrap},
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
            // When a row is expanded, reserve a dedicated details pane so the content
            // is always visible (inline expanded rows can be clipped off-screen).
            if app.results.expanded_leaderboard_index.is_some() {
                let inner = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(5), Constraint::Length(8)])
                    .split(chunks[0]);
                draw_leaderboard_table(f, app, inner[0], is_active, false);
                draw_leaderboard_details(f, app, inner[1], is_active);
            } else {
                draw_leaderboard_table(f, app, chunks[0], is_active, true);
            }
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
                    .max_by(|a, b| a.cagr.partial_cmp(&b.cagr).unwrap_or(std::cmp::Ordering::Equal));
                let best_sharpe = summaries
                    .iter()
                    .max_by(|a, b| a.sharpe.partial_cmp(&b.sharpe).unwrap_or(std::cmp::Ordering::Equal));

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
fn draw_leaderboard_table(
    f: &mut Frame,
    app: &App,
    area: Rect,
    is_active: bool,
    show_inline_expansion: bool,
) {
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

    // Extract date range from the first entry's dates vector
    let date_range_str = cross_symbol
        .entries
        .first()
        .and_then(|e| {
            let start = e.dates.first()?;
            let end = e.dates.last()?;
            let years = (*end - *start).num_days() as f64 / 365.25;
            Some(format!(
                "{} -> {} ({:.1}y)",
                start.format("%Y-%m-%d"),
                end.format("%Y-%m-%d"),
                years
            ))
        })
        .unwrap_or_default();

    let header_style = Style::default()
        .fg(colors::MAGENTA)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Cell::from("#").style(header_style),
        Cell::from("Conf").style(header_style), // Confidence grade
        Cell::from("WF").style(header_style),   // Walk-forward grade
        Cell::from("Strategy").style(header_style),
        Cell::from("Config").style(header_style),
        Cell::from("Syms").style(header_style),
        Cell::from("Avg Sharpe").style(header_style),
        Cell::from("OOS Sharpe").style(header_style), // Walk-forward OOS Sharpe
        Cell::from("Min Sharpe").style(header_style),
        Cell::from("Hit Rate").style(header_style),
        Cell::from("FDR p").style(header_style), // FDR-adjusted p-value
    ]);

    // Build rows with expansion support (optional)
    let expanded_idx = app.results.expanded_leaderboard_index;
    let mut rows: Vec<Row> = Vec::new();

    for (i, entry) in cross_symbol.entries.iter().enumerate() {
        let is_selected = i == app.results.selected_leaderboard_index;
        let is_expanded = expanded_idx == Some(i);
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

        // Walk-forward grade with color
        let (wf_text, wf_style) = match entry.walk_forward_grade {
            Some('A') => ("A", Style::default().fg(colors::GREEN)),
            Some('B') => ("B", Style::default().fg(colors::GREEN)),
            Some('C') => ("C", Style::default().fg(colors::YELLOW)),
            Some('D') => ("D", Style::default().fg(colors::ORANGE)),
            Some('F') => ("F", Style::default().fg(colors::RED)),
            Some(_) => ("?", base_style), // Unexpected grade
            None => ("-", Style::default().fg(colors::FG_DARK)),
        };

        // OOS Sharpe with color
        let (oos_text, oos_style) = match entry.mean_oos_sharpe {
            Some(s) if s > 0.5 => (format!("{:.2}", s), Style::default().fg(colors::GREEN)),
            Some(s) if s > 0.0 => (format!("{:.2}", s), Style::default().fg(colors::YELLOW)),
            Some(s) => (format!("{:.2}", s), Style::default().fg(colors::RED)),
            None => ("-".to_string(), Style::default().fg(colors::FG_DARK)),
        };

        // FDR p-value with significance indicator
        let (fdr_text, fdr_style) = match entry.fdr_adjusted_p_value {
            Some(p) if p < 0.01 => (format!("{:.3}**", p), Style::default().fg(colors::GREEN)),
            Some(p) if p < 0.05 => (format!("{:.3}*", p), Style::default().fg(colors::GREEN)),
            Some(p) if p < 0.10 => (format!("{:.3}", p), Style::default().fg(colors::YELLOW)),
            Some(p) => (format!("{:.3}", p), Style::default().fg(colors::FG_DARK)),
            None => ("-".to_string(), Style::default().fg(colors::FG_DARK)),
        };

        // Main row
        rows.push(Row::new(vec![
            rank_cell,
            Cell::from(conf_text).style(conf_style),
            Cell::from(wf_text).style(wf_style),
            Cell::from(entry.strategy_type.name()).style(base_style),
            Cell::from(truncate_config(&entry.config_id.display(), 18)).style(base_style),
            Cell::from(format!("{}", entry.symbols.len())).style(base_style),
            Cell::from(format!("{:.3}", entry.aggregate_metrics.avg_sharpe)).style(sharpe_style),
            Cell::from(oos_text).style(oos_style),
            Cell::from(format!("{:.3}", entry.aggregate_metrics.min_sharpe)).style(base_style),
            Cell::from(format!("{:.1}%", entry.aggregate_metrics.hit_rate * 100.0))
                .style(hit_style),
            Cell::from(fdr_text).style(fdr_style),
        ]));

        // If expanded, add detail rows
        if show_inline_expansion && is_expanded {
            let detail_style = Style::default().fg(colors::FG_DARK);
            let highlight_style = Style::default().fg(colors::CYAN);

            // Row 1: Full config parameters (add "Params:" label for parsing)
            let config_detail = format!("\u{251c}\u{2500} Params: {}", entry.config_id.display());
            rows.push(detail_row(config_detail, highlight_style));

            // Row 2: Date range for this config
            let entry_date_range = entry
                .dates
                .first()
                .zip(entry.dates.last())
                .map(|(s, e)| {
                    let years = (*e - *s).num_days() as f64 / 365.25;
                    format!(
                        "\u{251c}\u{2500} Range: {} \u{2192} {} ({:.1}y)",
                        s.format("%Y-%m-%d"),
                        e.format("%Y-%m-%d"),
                        years
                    )
                })
                .unwrap_or_else(|| "\u{251c}\u{2500} Range: N/A".to_string());
            rows.push(detail_row(entry_date_range, detail_style));

            // Row 3: Best symbols by Sharpe
            let mut sorted_symbols: Vec<_> = entry.per_symbol_metrics.iter().collect();
            sorted_symbols.sort_by(|a, b| b.1.sharpe.partial_cmp(&a.1.sharpe).unwrap_or(std::cmp::Ordering::Equal));

            let best_symbols: String = sorted_symbols
                .iter()
                .take(3)
                .map(|(sym, m)| format!("{} ({:.2})", sym, m.sharpe))
                .collect::<Vec<_>>()
                .join(" ");
            let best_line = format!("\u{251c}\u{2500} Best: {}", best_symbols);
            rows.push(detail_row(best_line, Style::default().fg(colors::GREEN)));

            // Row 4: Worst symbols by Sharpe
            let worst_symbols: String = sorted_symbols
                .iter()
                .rev()
                .take(3)
                .map(|(sym, m)| format!("{} ({:.2})", sym, m.sharpe))
                .collect::<Vec<_>>()
                .join(" ");
            let worst_line = format!("\u{251c}\u{2500} Worst: {}", worst_symbols);
            rows.push(detail_row(worst_line, Style::default().fg(colors::RED)));

            // Row 5: Sector breakdown (if available)
            if !entry.per_symbol_sectors.is_empty() {
                let (best_sectors, worst_sectors) =
                    compute_sector_stats(&entry.per_symbol_metrics, &entry.per_symbol_sectors);
                // Show top 3 best sectors in inline view
                let best_str = best_sectors
                    .iter()
                    .take(3)
                    .map(|(s, r)| format!("{}({:.0}%)", s, r))
                    .collect::<Vec<_>>()
                    .join(" ");
                let worst_str = worst_sectors
                    .iter()
                    .take(3)
                    .map(|(s, r)| format!("{}({:.0}%)", s, r))
                    .collect::<Vec<_>>()
                    .join(" ");
                let sector_line = format!("\u{2514}\u{2500} Sectors Best: {} | Worst: {}", best_str, worst_str);
                rows.push(detail_row(sector_line, detail_style));
            } else {
                // Closing line without sectors
                let close_line = format!(
                    "\u{2514}\u{2500} Stats: {:.0} trades avg | {:.1}% maxDD",
                    entry.aggregate_metrics.avg_trades,
                    entry.aggregate_metrics.worst_max_drawdown * 100.0
                );
                rows.push(detail_row(close_line, detail_style));
            }
        }
    }

    let widths = [
        Constraint::Length(3),  // Rank
        Constraint::Length(4),  // Confidence
        Constraint::Length(3),  // WF (Walk-forward grade)
        Constraint::Length(14), // Strategy
        Constraint::Length(18), // Config
        Constraint::Length(5),  // Symbols
        Constraint::Length(10), // Avg Sharpe
        Constraint::Length(10), // OOS Sharpe
        Constraint::Length(10), // Min Sharpe
        Constraint::Length(9),  // Hit Rate
        Constraint::Length(7),  // FDR p
    ];

    let scope_label = app.yolo.view_scope.display_name();
    let expanded_debug = match expanded_idx {
        Some(i) => format!(" [EXP:{}]", i),
        None => String::new(),
    };
    let title = if date_range_str.is_empty() {
        format!(
            "YOLO Leaderboard [{}]{} ({} configs)",
            scope_label,
            expanded_debug,
            app.yolo.configs_tested()
        )
    } else {
        format!(
            "YOLO Leaderboard [{}]{} | {} ({} configs)",
            scope_label,
            expanded_debug,
            date_range_str,
            app.yolo.configs_tested()
        )
    };

    let table = Table::new(rows, widths)
        .header(header)
        .block(panel_block(&title, is_active));

    f.render_widget(table, area);
}

fn draw_leaderboard_details(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let expanded_idx = match app.results.expanded_leaderboard_index {
        Some(i) => i,
        None => return,
    };

    // Prefer cross-symbol leaderboard (primary), fallback to per-symbol leaderboard.
    let lines: Vec<Line> = if let Some(lb) = app.yolo.cross_symbol_leaderboard() {
        if let Some(entry) = lb.entries.get(expanded_idx) {
            let mut out = Vec::new();
            out.push(Line::from(vec![
                Span::styled("Config: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(entry.config_id.display(), Style::default().fg(colors::CYAN)),
            ]));

            // Date range
            let range = entry
                .dates
                .first()
                .zip(entry.dates.last())
                .map(|(s, e)| {
                    let years = (*e - *s).num_days() as f64 / 365.25;
                    format!(
                        "{} → {} ({:.1}y)",
                        s.format("%Y-%m-%d"),
                        e.format("%Y-%m-%d"),
                        years
                    )
                })
                .unwrap_or_else(|| "N/A".to_string());
            out.push(Line::from(vec![
                Span::styled("Range:  ", Style::default().fg(colors::FG_DARK)),
                Span::styled(range, Style::default().fg(colors::FG)),
            ]));

            // Summary stats
            out.push(Line::from(vec![
                Span::styled("Syms:   ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{}", entry.symbols.len()),
                    Style::default().fg(colors::FG),
                ),
                Span::styled("  Avg Sharpe: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.3}", entry.aggregate_metrics.avg_sharpe),
                    Style::default().fg(colors::FG),
                ),
                Span::styled("  Min: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.3}", entry.aggregate_metrics.min_sharpe),
                    Style::default().fg(colors::FG),
                ),
                Span::styled("  Hit: ", Style::default().fg(colors::FG_DARK)),
                Span::styled(
                    format!("{:.1}%", entry.aggregate_metrics.hit_rate * 100.0),
                    Style::default().fg(colors::FG),
                ),
            ]));

            // Best/Worst symbols
            let mut sorted_symbols: Vec<_> = entry.per_symbol_metrics.iter().collect();
            sorted_symbols.sort_by(|a, b| b.1.sharpe.partial_cmp(&a.1.sharpe).unwrap_or(std::cmp::Ordering::Equal));

            let best_symbols = sorted_symbols
                .iter()
                .take(5)
                .map(|(sym, m)| format!("{}({:.2})", sym, m.sharpe))
                .collect::<Vec<_>>()
                .join("  ");
            out.push(Line::from(vec![
                Span::styled("Best:   ", Style::default().fg(colors::FG_DARK)),
                Span::styled(best_symbols, Style::default().fg(colors::GREEN)),
            ]));

            let worst_symbols = sorted_symbols
                .iter()
                .rev()
                .take(5)
                .map(|(sym, m)| format!("{}({:.2})", sym, m.sharpe))
                .collect::<Vec<_>>()
                .join("  ");
            out.push(Line::from(vec![
                Span::styled("Worst:  ", Style::default().fg(colors::FG_DARK)),
                Span::styled(worst_symbols, Style::default().fg(colors::RED)),
            ]));

            // Sector breakdown (if present)
            if !entry.per_symbol_sectors.is_empty() {
                let (best_sectors, worst_sectors) =
                    compute_sector_stats(&entry.per_symbol_metrics, &entry.per_symbol_sectors);

                // Best sectors line
                let best_str = best_sectors
                    .iter()
                    .map(|(s, r)| format!("{}({:.0}%)", s, r))
                    .collect::<Vec<_>>()
                    .join("  ");
                out.push(Line::from(vec![
                    Span::styled("Best:   ", Style::default().fg(colors::FG_DARK)),
                    Span::styled(best_str, Style::default().fg(colors::GREEN)),
                ]));

                // Worst sectors line
                let worst_str = worst_sectors
                    .iter()
                    .map(|(s, r)| format!("{}({:.0}%)", s, r))
                    .collect::<Vec<_>>()
                    .join("  ");
                out.push(Line::from(vec![
                    Span::styled("Worst:  ", Style::default().fg(colors::FG_DARK)),
                    Span::styled(worst_str, Style::default().fg(colors::RED)),
                ]));
            } else {
                out.push(Line::from(vec![
                    Span::styled("Stats:  ", Style::default().fg(colors::FG_DARK)),
                    Span::styled(
                        format!(
                            "{:.0} trades avg | {:.1}% maxDD",
                            entry.aggregate_metrics.avg_trades,
                            entry.aggregate_metrics.worst_max_drawdown * 100.0
                        ),
                        Style::default().fg(colors::FG),
                    ),
                ]));
            }

            out
        } else {
            vec![Line::from(vec![Span::styled(
                "Expanded selection out of range. Press ←/h to collapse.",
                Style::default().fg(colors::FG_DARK),
            )])]
        }
    } else {
        // Per-symbol fallback does not currently support expansion;
        // show a friendly message instead of a blank pane.
        vec![Line::from(vec![Span::styled(
            "Details unavailable (per-symbol fallback).",
            Style::default().fg(colors::FG_DARK),
        )])]
    };

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(panel_block("Details (←/h to collapse)", is_active));
    f.render_widget(para, area);
}

/// Compute sector hit rate statistics from per-symbol metrics.
/// Returns (best_sectors, worst_sectors) as vectors of (sector_name, hit_rate_pct).
fn compute_sector_stats(
    per_symbol_metrics: &std::collections::HashMap<String, trendlab_core::Metrics>,
    per_symbol_sectors: &std::collections::HashMap<String, String>,
) -> (Vec<(String, f64)>, Vec<(String, f64)>) {
    use std::collections::HashMap;

    // Group symbols by sector and compute hit rate per sector
    let mut sector_stats: HashMap<&str, (usize, usize)> = HashMap::new(); // (profitable, total)

    for (symbol, metrics) in per_symbol_metrics {
        if let Some(sector) = per_symbol_sectors.get(symbol) {
            let entry = sector_stats.entry(sector.as_str()).or_insert((0, 0));
            entry.1 += 1; // total
            if metrics.sharpe > 0.0 {
                entry.0 += 1; // profitable
            }
        }
    }

    // Sort sectors by hit rate descending, with secondary sort by name for determinism
    let mut sector_list: Vec<_> = sector_stats
        .iter()
        .map(|(sector, (profitable, total))| {
            let hit_rate = *profitable as f64 / *total as f64 * 100.0;
            (sector.to_string(), hit_rate)
        })
        .collect();
    // Primary: hit rate descending, Secondary: sector name ascending (for stable ordering)
    sector_list.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    // Best sectors: top 5 by hit rate
    let best: Vec<(String, f64)> = sector_list.iter().take(5).cloned().collect();

    // Worst sectors: bottom 5 by hit rate (reverse order, sorted ascending)
    let mut worst: Vec<(String, f64)> = sector_list.iter().rev().take(5).cloned().collect();
    // Sort worst by hit rate ascending for display
    worst.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    (best, worst)
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
            Span::styled(
                " to toggle Session/All-Time, ",
                Style::default().fg(colors::FG_DARK),
            ),
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

/// Create a detail row with 9 cells (matching leaderboard column count).
/// Spreads content across multiple columns to have enough display width.
fn detail_row(text: String, style: Style) -> Row<'static> {
    // Strip leading whitespace from tree-drawing characters
    let text = text.trim_start();

    // Split into label (before colon) and data (after colon)
    // Format: "├─ Label: data content here"
    let (label, data) = if let Some(idx) = text.find(':') {
        let (l, d) = text.split_at(idx + 1);
        (l.to_string(), d.trim().to_string())
    } else {
        // No colon (e.g., "├─ config_params") - use as label
        (text.to_string(), String::new())
    };

    // For long data, split across remaining columns (approx 10 chars each)
    // Columns available for data: Config(20) + Syms(5) + AvgSharpe(10) + MinSharpe(10) + HitRate(9) + AvgCAGR(10)
    let (data1, data2) = if data.len() > 24 {
        // Split at a space near the middle
        let split_point = data[..24.min(data.len())]
            .rfind(' ')
            .unwrap_or(24.min(data.len()));
        let (d1, d2) = data.split_at(split_point);
        (d1.to_string(), d2.trim().to_string())
    } else {
        (data, String::new())
    };

    Row::new(vec![
        Cell::from(""),                 // Rank (3 chars)
        Cell::from(""),                 // Conf (4 chars)
        Cell::from(label).style(style), // Strategy (14 chars) - label
        Cell::from(data1).style(style), // Config (20 chars) - data part 1
        Cell::from(""),                 // Syms (5 chars)
        Cell::from(data2).style(style), // Avg Sharpe (10 chars) - data part 2 overflow
        Cell::from(""),                 // Min Sharpe (10 chars)
        Cell::from(""),                 // Hit Rate (9 chars)
        Cell::from(""),                 // Avg CAGR (10 chars)
    ])
}

fn draw_help(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let view_mode = app.results.view_mode_name();
    let scope_label = app.yolo.view_scope.display_name();

    // Build help text based on view mode
    let mut spans = vec![
        Span::styled("\u{2191}\u{2193}: ", Style::default().fg(colors::CYAN)),
        Span::styled("Select  ", Style::default().fg(colors::FG_DARK)),
    ];

    // Show expand/collapse help in Leaderboard view
    if app.results.view_mode == ResultsViewMode::Leaderboard {
        if app.results.expanded_leaderboard_index.is_some() {
            spans.push(Span::styled(
                "h/\u{2190}: ",
                Style::default().fg(colors::CYAN),
            ));
            spans.push(Span::styled(
                "Collapse  ",
                Style::default().fg(colors::FG_DARK),
            ));
        } else {
            spans.push(Span::styled(
                "l/\u{2192}: ",
                Style::default().fg(colors::CYAN),
            ));
            spans.push(Span::styled(
                "Expand  ",
                Style::default().fg(colors::FG_DARK),
            ));
        }
    }

    spans.extend([
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
    ]);

    let lines = vec![Line::from(spans)];
    let para = Paragraph::new(lines).block(panel_block("Controls", is_active));

    f.render_widget(para, area);
}
