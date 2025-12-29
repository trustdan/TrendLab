//! Analysis panel - view statistical analysis of backtest results

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use trendlab_engine::app::App;
use crate::ui::{colors, panel_block};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    // Split into sections
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Return Distribution
            Constraint::Length(14), // Trade Analysis
            Constraint::Min(10),    // Regime Analysis
            Constraint::Length(2),  // Help
        ])
        .split(area);

    if let Some(ref analysis) = app.results.selected_analysis {
        draw_return_distribution(f, analysis, chunks[0]);
        draw_trade_analysis(f, analysis, chunks[1]);
        draw_regime_analysis(f, analysis, chunks[2]);
    } else {
        draw_no_analysis(f, chunks[0]);
        // Leave other areas empty
        f.render_widget(
            Paragraph::new("").block(panel_block("Trade Analysis", false)),
            chunks[1],
        );
        f.render_widget(
            Paragraph::new("").block(panel_block("Regime Analysis", false)),
            chunks[2],
        );
    }

    draw_help(f, chunks[3]);
}

fn draw_no_analysis(f: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "No analysis computed yet.",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Select a result in the Results panel and press 'a' to compute analysis.",
            Style::default().fg(colors::FG_DARK),
        )]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Return Distribution", false));
    f.render_widget(para, area);
}

fn draw_return_distribution(
    f: &mut Frame,
    analysis: &trendlab_core::StatisticalAnalysis,
    area: Rect,
) {
    let rd = &analysis.return_distribution;

    // Color helpers
    let skew_style = if rd.skewness < -0.5 {
        Style::default().fg(colors::RED) // Negative skew = bad
    } else if rd.skewness > 0.5 {
        Style::default().fg(colors::GREEN) // Positive skew = good
    } else {
        Style::default().fg(colors::YELLOW)
    };

    let kurt_style = if rd.kurtosis > 3.0 {
        Style::default().fg(colors::ORANGE) // Fat tails
    } else {
        Style::default().fg(colors::FG)
    };

    let lines = vec![
        // Header row
        Line::from(vec![
            Span::styled(
                "Risk Metrics",
                Style::default()
                    .fg(colors::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({} days)", rd.n_observations),
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
        Line::from(""),
        // VaR row
        Line::from(vec![
            Span::styled("VaR 95%: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{:.2}%", rd.var_95 * 100.0),
                Style::default().fg(colors::RED),
            ),
            Span::styled("    VaR 99%: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{:.2}%", rd.var_99 * 100.0),
                Style::default().fg(colors::RED),
            ),
        ]),
        // CVaR row
        Line::from(vec![
            Span::styled("CVaR 95%: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{:.2}%", rd.cvar_95 * 100.0),
                Style::default().fg(colors::RED),
            ),
            Span::styled("   CVaR 99%: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{:.2}%", rd.cvar_99 * 100.0),
                Style::default().fg(colors::RED),
            ),
        ]),
        Line::from(""),
        // Shape metrics
        Line::from(vec![
            Span::styled("Skewness: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(format!("{:.2}", rd.skewness), skew_style),
            Span::styled(
                if rd.skewness < -0.5 {
                    " (fat left tail)"
                } else if rd.skewness > 0.5 {
                    " (fat right tail)"
                } else {
                    ""
                },
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
        Line::from(vec![
            Span::styled("Kurtosis: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(format!("{:.2}", rd.kurtosis), kurt_style),
            Span::styled(
                if rd.kurtosis > 3.0 {
                    " (fat tails)"
                } else {
                    ""
                },
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
        Line::from(""),
        // Daily returns summary
        Line::from(vec![
            Span::styled("Daily: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("mean {:.2}%", rd.mean_return * 100.0),
                Style::default().fg(colors::FG),
            ),
            Span::styled(" / ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("std {:.2}%", rd.std_return * 100.0),
                Style::default().fg(colors::FG),
            ),
            Span::styled(" / ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("min {:.2}%", rd.min_return * 100.0),
                Style::default().fg(colors::RED),
            ),
            Span::styled(" / ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("max {:.2}%", rd.max_return * 100.0),
                Style::default().fg(colors::GREEN),
            ),
        ]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Return Distribution", true));
    f.render_widget(para, area);
}

fn draw_trade_analysis(f: &mut Frame, analysis: &trendlab_core::StatisticalAnalysis, area: Rect) {
    let ta = &analysis.trade_analysis;

    // Build holding period histogram as a mini bar chart
    let mut hist_spans = vec![Span::styled("Hold: ", Style::default().fg(colors::FG_DARK))];
    for bucket in &ta.holding_period.histogram {
        let bar_len = (bucket.pct * 10.0).round() as usize; // Max 10 chars
        let bar = "\u{2588}".repeat(bar_len.min(10));
        hist_spans.push(Span::styled(
            format!(
                "{} ",
                &bucket.label[..bucket.label.find(' ').unwrap_or(bucket.label.len())]
            ),
            Style::default().fg(colors::FG_DARK),
        ));
        hist_spans.push(Span::styled(bar, Style::default().fg(colors::BLUE)));
        hist_spans.push(Span::styled(" ", Style::default().fg(colors::FG)));
    }

    // Edge ratio styling
    let edge_style = if ta.edge_ratio.mean > 1.5 {
        Style::default().fg(colors::GREEN)
    } else if ta.edge_ratio.mean < 1.0 {
        Style::default().fg(colors::RED)
    } else {
        Style::default().fg(colors::YELLOW)
    };

    let lines = vec![
        // Header
        Line::from(vec![
            Span::styled(
                "Trade Analysis",
                Style::default()
                    .fg(colors::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({} trades)", ta.n_trades),
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
        Line::from(""),
        // Holding period stats
        Line::from(vec![
            Span::styled("Holding Period: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!(
                    "mean {:.0}d / median {:.0}d / range {}-{}d",
                    ta.holding_period.mean,
                    ta.holding_period.median,
                    ta.holding_period.min,
                    ta.holding_period.max
                ),
                Style::default().fg(colors::FG),
            ),
        ]),
        // Mini histogram
        Line::from(hist_spans),
        Line::from(""),
        // MAE/MFE
        Line::from(vec![
            Span::styled("MAE: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("mean {:.2}%", ta.mae.mean * 100.0),
                Style::default().fg(colors::RED),
            ),
            Span::styled(" / ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("p90 {:.2}%", ta.mae.p90 * 100.0),
                Style::default().fg(colors::RED),
            ),
            Span::styled(" / ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("max {:.2}%", ta.mae.max * 100.0),
                Style::default().fg(colors::RED),
            ),
        ]),
        Line::from(vec![
            Span::styled("MFE: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("mean {:.2}%", ta.mfe.mean * 100.0),
                Style::default().fg(colors::GREEN),
            ),
            Span::styled(" / ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("p90 {:.2}%", ta.mfe.p90 * 100.0),
                Style::default().fg(colors::GREEN),
            ),
            Span::styled(" / ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("max {:.2}%", ta.mfe.max * 100.0),
                Style::default().fg(colors::GREEN),
            ),
        ]),
        Line::from(""),
        // Edge ratio
        Line::from(vec![
            Span::styled(
                "Edge Ratio (MFE/MAE): ",
                Style::default().fg(colors::FG_DARK),
            ),
            Span::styled(format!("{:.2}", ta.edge_ratio.mean), edge_style),
            Span::styled(
                format!(
                    "  ({:.0}% trades MFE>MAE)",
                    ta.edge_ratio.pct_favorable * 100.0
                ),
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
        // Winners vs losers edge
        Line::from(vec![
            Span::styled("  Winners: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{:.2}", ta.edge_ratio.winners_mean),
                Style::default().fg(colors::GREEN),
            ),
            Span::styled("  Losers: ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{:.2}", ta.edge_ratio.losers_mean),
                Style::default().fg(colors::RED),
            ),
        ]),
    ];

    let para = Paragraph::new(lines).block(panel_block("Trade Analysis", true));
    f.render_widget(para, area);
}

fn draw_regime_analysis(f: &mut Frame, analysis: &trendlab_core::StatisticalAnalysis, area: Rect) {
    let ra = &analysis.regime_analysis;

    // Helper to draw regime row
    fn regime_line(
        name: &str,
        metrics: &trendlab_core::RegimeMetrics,
        name_color: ratatui::style::Color,
    ) -> Line<'static> {
        let wr_style = if metrics.win_rate > 0.55 {
            Style::default().fg(colors::GREEN)
        } else if metrics.win_rate < 0.45 {
            Style::default().fg(colors::RED)
        } else {
            Style::default().fg(colors::YELLOW)
        };

        let ret_style = if metrics.avg_trade_return > 0.01 {
            Style::default().fg(colors::GREEN)
        } else if metrics.avg_trade_return < 0.0 {
            Style::default().fg(colors::RED)
        } else {
            Style::default().fg(colors::FG)
        };

        Line::from(vec![
            Span::styled(
                format!("{:<8}", name),
                Style::default().fg(name_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>5.0}% days", metrics.pct_days * 100.0),
                Style::default().fg(colors::FG_DARK),
            ),
            Span::styled(" | ", Style::default().fg(colors::BORDER)),
            Span::styled(
                format!("{:>3} trades", metrics.n_trades_entered),
                Style::default().fg(colors::FG),
            ),
            Span::styled(" | ", Style::default().fg(colors::BORDER)),
            Span::styled("WR ", Style::default().fg(colors::FG_DARK)),
            Span::styled(format!("{:.0}%", metrics.win_rate * 100.0), wr_style),
            Span::styled(" | ", Style::default().fg(colors::BORDER)),
            Span::styled("Avg ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{:+.2}%", metrics.avg_trade_return * 100.0),
                ret_style,
            ),
            Span::styled(" | ", Style::default().fg(colors::BORDER)),
            Span::styled("Sharpe ", Style::default().fg(colors::FG_DARK)),
            Span::styled(
                format!("{:.2}", metrics.sharpe),
                Style::default().fg(colors::FG),
            ),
        ])
    }

    let lines = vec![
        // Header
        Line::from(vec![
            Span::styled(
                "Regime Analysis",
                Style::default()
                    .fg(colors::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  (ATR {}, median {:.2}%)",
                    ra.atr_period,
                    ra.median_atr * 100.0
                ),
                Style::default().fg(colors::FG_DARK),
            ),
        ]),
        Line::from(""),
        // Column headers
        Line::from(vec![Span::styled(
            "Regime   % Time   |  Trades  |  Win Rate  |  Avg Ret   |  Sharpe",
            Style::default().fg(colors::FG_DARK),
        )]),
        Line::from(vec![Span::styled(
            "\u{2500}".repeat(70),
            Style::default().fg(colors::BORDER),
        )]),
        // High volatility
        regime_line("High", &ra.high_vol, colors::RED),
        // Neutral
        regime_line("Neutral", &ra.neutral_vol, colors::YELLOW),
        // Low volatility
        regime_line("Low", &ra.low_vol, colors::GREEN),
    ];

    let para = Paragraph::new(lines).block(panel_block("Regime Analysis", true));
    f.render_widget(para, area);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let lines = vec![Line::from(vec![
        Span::styled("'a': ", Style::default().fg(colors::CYAN)),
        Span::styled("Toggle analysis  ", Style::default().fg(colors::FG_DARK)),
        Span::styled("Esc: ", Style::default().fg(colors::CYAN)),
        Span::styled("Back to results", Style::default().fg(colors::FG_DARK)),
    ])];

    let para = Paragraph::new(lines);
    f.render_widget(para, area);
}
