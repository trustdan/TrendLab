//! Help panel - keyboard shortcuts and feature documentation

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::ui::colors;
use trendlab_engine::app::{App, HelpSection, Panel};

/// A single help entry with key binding and description
struct HelpEntry {
    key: &'static str,
    description: &'static str,
}

/// Help content for a section
struct HelpContent {
    title: &'static str,
    entries: &'static [HelpEntry],
    details: &'static str,
}

// ============================================================================
// Help Content Definitions
// ============================================================================

const GLOBAL_HELP: HelpContent = HelpContent {
    title: "Global Navigation",
    entries: &[
        HelpEntry {
            key: "1-6",
            description: "Jump to panel (Data, Strategy, Sweep, Results, Chart, Help)",
        },
        HelpEntry {
            key: "Tab",
            description: "Next panel",
        },
        HelpEntry {
            key: "Shift+Tab",
            description: "Previous panel",
        },
        HelpEntry {
            key: "?",
            description: "Open Help panel",
        },
        HelpEntry {
            key: "q",
            description: "Quit application",
        },
        HelpEntry {
            key: "Esc",
            description: "Cancel / dismiss modal",
        },
        HelpEntry {
            key: "R",
            description: "Reset defaults (randomize if enabled)",
        },
    ],
    details: r#"
TrendLab uses a panel-based interface with vim-style navigation throughout.
Each panel is accessible via number keys (1-6) or Tab cycling.

The startup modal (shown on launch) offers two modes:
- Manual: Configure tickers, strategy, and sweep parameters step by step
- Full-Auto: Select all tickers and run a complete sweep automatically

Press 'R' to reset parameters. If random defaults are enabled (seed shown in
status bar), this randomizes parameters for exploration.
"#,
};

const DATA_HELP: HelpContent = HelpContent {
    title: "Data Panel",
    entries: &[
        HelpEntry {
            key: "j/↓",
            description: "Move down in list",
        },
        HelpEntry {
            key: "k/↑",
            description: "Move up in list",
        },
        HelpEntry {
            key: "h/←",
            description: "Back to sectors",
        },
        HelpEntry {
            key: "l/→/Enter",
            description: "Enter sector / expand",
        },
        HelpEntry {
            key: "Space",
            description: "Toggle ticker selection",
        },
        HelpEntry {
            key: "a",
            description: "Select all tickers (in sector or global)",
        },
        HelpEntry {
            key: "n",
            description: "Deselect all (none)",
        },
        HelpEntry {
            key: "f",
            description: "Fetch data for selected tickers",
        },
        HelpEntry {
            key: "s",
            description: "Search symbols (Yahoo Finance)",
        },
    ],
    details: r#"
The Data panel displays a sector/ticker hierarchy for selecting instruments.
Tickers are organized by sector (Technology, Healthcare, Finance, etc.).

Selection workflow:
1. Navigate sectors with ↑/↓, press → or Enter to view tickers
2. In ticker view, use Space to toggle individual selections
3. Use 'a' for select all, 'n' for none
4. Press 'f' to fetch historical data from Yahoo Finance

The green dot (●) indicates data is loaded in cache. Search with 's' to find
any Yahoo Finance symbol not in the default universe.
"#,
};

const STRATEGY_HELP: HelpContent = HelpContent {
    title: "Strategy Panel",
    entries: &[
        HelpEntry {
            key: "j/↓",
            description: "Next category/field",
        },
        HelpEntry {
            key: "k/↑",
            description: "Previous category/field",
        },
        HelpEntry {
            key: "h/←",
            description: "Decrease value / collapse",
        },
        HelpEntry {
            key: "l/→",
            description: "Increase value / expand",
        },
        HelpEntry {
            key: "Enter",
            description: "Expand/collapse category",
        },
        HelpEntry {
            key: "e",
            description: "Toggle ensemble mode",
        },
        HelpEntry {
            key: "Space",
            description: "Toggle strategy in ensemble",
        },
    ],
    details: r#"
Configure backtest parameters across 5 strategy categories:

1. SMA Cross - Simple moving average crossover
2. EMA Cross - Exponential moving average crossover
3. Donchian Breakout - Channel breakout system
4. TSMOM - Time-series momentum
5. Mean Reversion - Bollinger band mean reversion

Each strategy has configurable parameters (fast/slow periods, ATR multipliers,
lookback windows, etc.). Expand a category to adjust its parameters.

Ensemble Mode (press 'e'):
Run multiple strategies together and compare results. Toggle strategies
with Space when ensemble mode is active.
"#,
};

const SWEEP_HELP: HelpContent = HelpContent {
    title: "Sweep Panel",
    entries: &[
        HelpEntry {
            key: "Enter",
            description: "Start parameter sweep",
        },
        HelpEntry {
            key: "Esc",
            description: "Cancel running sweep",
        },
        HelpEntry {
            key: "j/↓",
            description: "Next sweep depth",
        },
        HelpEntry {
            key: "k/↑",
            description: "Previous sweep depth",
        },
        HelpEntry {
            key: "y",
            description: "YOLO mode (configure and run)",
        },
    ],
    details: r#"
Parameter sweeps test multiple configurations to find optimal settings.

Sweep Depths:
- Quick: ~50 configs - Fast exploration, minimal parameters
- Normal: ~200 configs - Balanced coverage
- Deep: ~500 configs - Thorough parameter search
- Insane: ~2000 configs - Exhaustive (time-consuming)

YOLO Mode (press 'y'):
Opens configuration modal for custom date ranges, randomization percentage,
sweep depth, and warmup iterations. Useful for stress-testing strategies
across different time periods.

YOLO Config Modal Settings:
- Start/End Date: Backtest period (auto-fetches missing data)
- Randomization %: Parameter jitter strength (default 30%)
- WF Sharpe Threshold: Min Sharpe for walk-forward validation (0.25)
- Sweep Depth: Quick/Standard/Comprehensive grid density
- Polars/Outer Threads: Parallelism caps (auto = system default)
- Warmup Iters: Iterations before winner exploitation (default 50)

Warmup Period:
During warmup (first N iterations), YOLO focuses on exploration:
- No ExploitWinner mode - only MaximizeCoverage, PureRandom, LocalJitter
- 2.5x wider jitter for broader parameter space coverage
- Coverage and leaderboard updates happen every iteration

After warmup, ExploitWinner mode unlocks (5-25% chance):
- Picks a random winner from the top 5 leaderboard entries
- Centers the grid on that winner's exact parameters
- Applies tight 50% jitter around the winning config

This ensures statistical significance before exploiting early winners.

Artifact Auto-Export:
When YOLO discovers a new top cross-symbol config, it auto-exports a
StrategyArtifact JSON to artifacts/exports/{session}/. These artifacts
contain everything needed to generate Pine Scripts via /pine:generate.

Results are automatically sorted by weighted score and displayed in the
Results panel when the sweep completes.
"#,
};

const RESULTS_HELP: HelpContent = HelpContent {
    title: "Results Panel",
    entries: &[
        HelpEntry {
            key: "j/↓",
            description: "Next result",
        },
        HelpEntry {
            key: "k/↑",
            description: "Previous result",
        },
        HelpEntry {
            key: "Enter",
            description: "View result in Chart panel",
        },
        HelpEntry {
            key: "s",
            description: "Cycle sort column",
        },
        HelpEntry {
            key: "v",
            description: "Cycle view mode (list/detailed)",
        },
        HelpEntry {
            key: "t",
            description: "Toggle analysis view",
        },
        HelpEntry {
            key: "p",
            description: "Cycle risk profile",
        },
        HelpEntry {
            key: "P",
            description: "Export Pine Script (Leaderboard view)",
        },
        HelpEntry {
            key: "gg",
            description: "Jump to top",
        },
        HelpEntry {
            key: "G",
            description: "Jump to bottom",
        },
    ],
    details: r#"
Browse and analyze sweep results with sorting and filtering.

Metrics displayed:
- CAGR: Compound annual growth rate
- Sharpe: Risk-adjusted return (higher is better)
- Sortino: Downside risk-adjusted return
- Max DD: Maximum drawdown percentage
- Win Rate: Percentage of winning trades
- Trades: Number of trades executed

Risk Profiles (press 'p'):
Apply different weighting schemes to rank results:
- Balanced: Equal weight to return and risk metrics
- Conservative: Emphasizes low drawdown, stability
- Aggressive: Prioritizes high returns
- Sharpe-focused: Optimizes for risk-adjusted returns

Analysis View (press 't'):
Shows statistical analysis including regime performance, streak analysis,
and robustness metrics.

Pine Script Export (press 'P' in Leaderboard view):
Exports strategy configuration for TradingView Pine Script generation.
Output goes to: pine-scripts/strategies/<strategy>/<config>.pine
"#,
};

const CHART_HELP: HelpContent = HelpContent {
    title: "Chart Panel",
    entries: &[
        HelpEntry {
            key: "h/←",
            description: "Scroll left (earlier)",
        },
        HelpEntry {
            key: "l/→",
            description: "Scroll right (later)",
        },
        HelpEntry {
            key: "j/↓",
            description: "Zoom out",
        },
        HelpEntry {
            key: "k/↑",
            description: "Zoom in",
        },
        HelpEntry {
            key: "m",
            description: "Cycle chart mode",
        },
        HelpEntry {
            key: "v",
            description: "Toggle volume subplot",
        },
        HelpEntry {
            key: "c",
            description: "Toggle crosshair",
        },
        HelpEntry {
            key: "d",
            description: "Toggle drawdown overlay",
        },
        HelpEntry {
            key: "0",
            description: "Reset view to default",
        },
    ],
    details: r#"
Visualize price data and backtest results with interactive charts.

Chart Modes (press 'm'):
- Candlestick: OHLC candlestick chart with entry/exit markers
- Line: Simple closing price line
- Equity: Portfolio equity curve over time
- Combined: Price chart with equity curve overlay

Display Options:
- Volume (v): Show/hide volume bars below price
- Crosshair (c): Enable/disable cursor tracking
- Drawdown (d): Overlay drawdown percentage from peak

Navigation:
- ←/→: Pan through time (scroll)
- ↑/↓: Zoom in/out (more/fewer bars visible)
- 0: Reset to default zoom and position

Entry signals shown as green triangles, exits as red triangles.
"#,
};

const FEATURES_HELP: HelpContent = HelpContent {
    title: "Features Overview",
    entries: &[
        HelpEntry {
            key: "Risk Profiles",
            description: "Weighted ranking for different trading styles",
        },
        HelpEntry {
            key: "YOLO Mode",
            description: "Adaptive exploration with warmup + winner exploitation",
        },
        HelpEntry {
            key: "Statistical Analysis",
            description: "Regime splits, streak analysis, robustness",
        },
        HelpEntry {
            key: "Ensemble Mode",
            description: "Compare multiple strategies simultaneously",
        },
        HelpEntry {
            key: "Symbol Search",
            description: "Find any Yahoo Finance symbol",
        },
    ],
    details: r#"
Key Features:

RISK PROFILES
Apply weighted scoring to rank results by your trading style:
- Balanced: 1.0x Sharpe, 1.0x Sortino, 0.3x MaxDD, 0.2x WinRate, 0.2x CAGR
- Conservative: 0.8x Sharpe, 1.2x Sortino, 0.5x MaxDD, 0.3x WinRate, 0.1x CAGR
- Aggressive: 0.6x Sharpe, 0.6x Sortino, 0.1x MaxDD, 0.1x WinRate, 0.4x CAGR
- Sharpe-focused: 1.5x Sharpe, 0.5x Sortino, 0.2x MaxDD, 0.1x WinRate, 0.1x CAGR

YOLO MODE
Adaptive strategy discovery with intelligent exploration:
- Warmup Period (default 50 iters): Pure exploration, no winner exploitation
  - Higher probability of MaximizeCoverage and PureRandom modes
  - 2.5x wider jitter for broader parameter space coverage
- Post-Warmup: Adaptive mode based on coverage metrics
  - ExploitWinner: Picks top configs from leaderboard, jitters around them
  - MaximizeCoverage: Targets unexplored parameter regions
  - PureRandom: Random sampling for diversity
  - LocalJitter: Small perturbations around current config
- Auto-fetches missing data from Yahoo Finance
- Auto-exports StrategyArtifact JSON for top cross-symbol performers

STATISTICAL ANALYSIS
Deep analysis of backtest results:
- Regime performance (bull/bear market splits)
- Streak analysis (consecutive wins/losses)
- Return distribution statistics
- Trade timing analysis

ENSEMBLE MODE
Run multiple strategies on the same data:
- Compare performance side-by-side
- Identify strategy correlation
- Find diversification opportunities
"#,
};

// ============================================================================
// Drawing Functions
// ============================================================================

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.active_panel == Panel::Help;

    // Split into section tabs and content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Draw section tabs
    draw_section_tabs(f, app, chunks[0], is_active);

    // Draw content area
    draw_content(f, app, chunks[1], is_active);
}

fn draw_section_tabs(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let sections = [
        (HelpSection::Global, "Global"),
        (HelpSection::Data, "Data"),
        (HelpSection::Strategy, "Strategy"),
        (HelpSection::Sweep, "Sweep"),
        (HelpSection::Results, "Results"),
        (HelpSection::Chart, "Chart"),
        (HelpSection::Features, "Features"),
    ];

    let mut spans: Vec<Span> = vec![Span::raw(" ")];

    for (i, (section, name)) in sections.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(colors::FG_DARK)));
        }

        let is_selected = *section == app.help.active_section;
        let style = if is_selected {
            Style::default()
                .fg(colors::BLUE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::FG_DARK)
        };

        let bracket_style = if is_selected {
            Style::default().fg(colors::YELLOW)
        } else {
            Style::default().fg(colors::FG_DARK)
        };

        spans.push(Span::styled("[", bracket_style));
        spans.push(Span::styled(*name, style));
        spans.push(Span::styled("]", bracket_style));
    }

    let tabs_line = Line::from(spans);

    let border_color = if is_active {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(if is_active {
                    colors::BLUE
                } else {
                    colors::FG_DARK
                })
                .add_modifier(Modifier::BOLD),
        ));

    let para = Paragraph::new(tabs_line).block(block);
    f.render_widget(para, area);
}

fn draw_content(f: &mut Frame, app: &App, area: Rect, is_active: bool) {
    let content = get_section_content(app.help.active_section);
    let mut lines: Vec<Line> = Vec::new();

    // Search bar if active
    if app.help.search_mode {
        let cursor = "▎";
        lines.push(Line::from(vec![
            Span::styled("Search: ", Style::default().fg(colors::CYAN)),
            Span::styled(
                &app.help.search_query,
                Style::default()
                    .fg(colors::YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::default().fg(colors::YELLOW)),
        ]));

        if !app.help.search_matches.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                format!(
                    "  {} match{} (n/N to navigate)",
                    app.help.search_matches.len(),
                    if app.help.search_matches.len() == 1 {
                        ""
                    } else {
                        "es"
                    }
                ),
                Style::default().fg(colors::FG_DARK),
            )]));
        }
        lines.push(Line::from(""));
    }

    // Title
    lines.push(Line::from(vec![Span::styled(
        content.title,
        Style::default()
            .fg(colors::MAGENTA)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(content.title.len()),
        Style::default().fg(colors::FG_DARK),
    )]));
    lines.push(Line::from(""));

    // Quick reference entries
    lines.push(Line::from(vec![Span::styled(
        "Quick Reference:",
        Style::default()
            .fg(colors::CYAN)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    for entry in content.entries {
        let key_width = 16;
        let key_padded = format!("{:<width$}", entry.key, width = key_width);

        // Check if this line matches search
        let is_match = !app.help.search_query.is_empty()
            && (entry
                .key
                .to_lowercase()
                .contains(&app.help.search_query.to_lowercase())
                || entry
                    .description
                    .to_lowercase()
                    .contains(&app.help.search_query.to_lowercase()));

        let key_style = if is_match {
            Style::default()
                .fg(colors::YELLOW)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::GREEN)
        };

        let desc_style = if is_match {
            Style::default()
                .fg(colors::YELLOW)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::FG)
        };

        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(key_padded, key_style),
            Span::styled(entry.description, desc_style),
        ]));
    }

    // Detailed description
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Details:",
        Style::default()
            .fg(colors::CYAN)
            .add_modifier(Modifier::BOLD),
    )]));

    // Parse and add detail lines
    for detail_line in content.details.trim().lines() {
        // Check if this line matches search
        let is_match = !app.help.search_query.is_empty()
            && detail_line
                .to_lowercase()
                .contains(&app.help.search_query.to_lowercase());

        let style = if is_match {
            Style::default()
                .fg(colors::YELLOW)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::FG_DARK)
        };

        lines.push(Line::from(vec![Span::styled(detail_line, style)]));
    }

    // Calculate max scroll based on content
    let visible_height = area.height.saturating_sub(2) as usize; // subtract border
    let total_lines = lines.len();

    // Apply scroll offset
    let scroll_offset = app
        .help
        .scroll_offset
        .min(total_lines.saturating_sub(visible_height));
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll_offset)
        .take(visible_height)
        .collect();

    let border_color = if is_active {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER
    };

    // Build title with scroll indicator
    let scroll_info = if total_lines > visible_height {
        format!(
            " [{}/{}] ",
            scroll_offset + 1,
            total_lines.saturating_sub(visible_height) + 1
        )
    } else {
        String::new()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" {} {}", content.title, scroll_info),
            Style::default().fg(if is_active {
                colors::BLUE
            } else {
                colors::FG_DARK
            }),
        ));

    let para = Paragraph::new(visible_lines)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn get_section_content(section: HelpSection) -> &'static HelpContent {
    match section {
        HelpSection::Global => &GLOBAL_HELP,
        HelpSection::Data => &DATA_HELP,
        HelpSection::Strategy => &STRATEGY_HELP,
        HelpSection::Sweep => &SWEEP_HELP,
        HelpSection::Results => &RESULTS_HELP,
        HelpSection::Chart => &CHART_HELP,
        HelpSection::Features => &FEATURES_HELP,
    }
}

/// Get the total line count for a section (for scroll calculation)
#[allow(dead_code)]
pub fn get_section_line_count(section: HelpSection) -> usize {
    let content = get_section_content(section);
    // Quick reference lines + detail lines + headers + spacing
    let entry_lines = content.entries.len();
    let detail_lines = content.details.trim().lines().count();
    // 3 for title/underline/blank, 2 for "Quick Reference:" header, 2 for "Details:" header
    7 + entry_lines + detail_lines
}
