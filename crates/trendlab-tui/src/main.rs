//! TrendLab TUI - Interactive Terminal User Interface
//!
//! Provides a rich terminal interface for backtesting exploration:
//! - Data panel: View loaded data and symbols
//! - Strategy panel: Configure strategy parameters
//! - Sweep panel: Run parameter sweeps
//! - Results panel: View backtest results
//! - Chart panel: Visualize equity curves

use anyhow::Result;
use crossterm::{
    event::{
        self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;
use trendlab_logging::LogConfig;

mod panels;
mod ui;

use trendlab_engine::app::{
    App, AutoStage, ChartViewMode, OperationState, Panel, ResultsViewMode, SearchSuggestion,
    StartupMode, StrategyCurve, StrategyFocus, StrategySelection, StrategyType, TickerBestStrategy,
    TickerCurve, WinningConfig, YoloConfigField,
};
use trendlab_engine::worker::{spawn_worker, WorkerChannels, WorkerCommand, WorkerUpdate};

fn main() -> Result<()> {
    // Initialize logging from environment (set by launcher)
    // TUI logs to file only to avoid interfering with the terminal UI
    let log_config = LogConfig::from_env();

    let _log_guard = if log_config.enabled {
        let guard = trendlab_logging::init_tui_logging(&log_config);
        tracing::info!(
            filter = %log_config.filter,
            log_dir = %log_config.log_dir.display(),
            "TUI starting with logging enabled"
        );
        guard
    } else {
        None
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Spawn background worker thread
    let (channels, worker_handle) = spawn_worker();

    // Create app and run
    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app, &channels);

    // Cleanup: signal worker to shutdown
    let _ = channels.command_tx.send(WorkerCommand::Shutdown);
    let _ = worker_handle.join();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

/// Main event loop - polls for input and worker updates.
fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    channels: &WorkerChannels,
) -> Result<()> {
    loop {
        // 1. Render current state
        terminal.draw(|f| ui::draw(f, app))?;

        // 2. Poll for input (non-blocking, 16ms timeout for ~60fps)
        if poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        // Handle Ctrl+d/u for Help panel page navigation
                        if app.active_panel == Panel::Help
                            && key.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            match key.code {
                                KeyCode::Char('d') => {
                                    app.help_page_down();
                                    continue;
                                }
                                KeyCode::Char('u') => {
                                    app.help_page_up();
                                    continue;
                                }
                                _ => {}
                            }
                        }

                        match handle_key(app, key.code, channels) {
                            KeyResult::Quit => return Ok(()),
                            KeyResult::Continue => {}
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::Moved => {
                            app.update_cursor_position(mouse.column, mouse.row);
                        }
                        MouseEventKind::ScrollUp => {
                            // Zoom in on chart when in Chart panel
                            if app.active_panel == Panel::Chart {
                                app.chart.zoom_in_animated();
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            // Zoom out on chart when in Chart panel
                            if app.active_panel == Panel::Chart {
                                app.chart.zoom_out_animated();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Tick animations (for smooth zoom/pan)
        app.tick_animations();

        // 3. Drain all pending updates from worker (non-blocking)
        while let Ok(update) = channels.update_rx.try_recv() {
            apply_update(app, update, channels);
        }
    }
}

/// Result of handling a key press.
enum KeyResult {
    Quit,
    Continue,
}

/// Save all-time leaderboards to disk before exiting.
/// This ensures leaderboard data persists across TUI sessions.
fn save_leaderboards_on_exit(yolo: &trendlab_engine::app::YoloState) {
    let per_symbol_path = Path::new("artifacts/leaderboard.json");
    let cross_symbol_path = Path::new("artifacts/cross_symbol_leaderboard.json");

    if let Err(e) = yolo.all_time_leaderboard.save(per_symbol_path) {
        tracing::warn!("Failed to save all-time per-symbol leaderboard: {}", e);
    } else {
        tracing::info!("Saved all-time per-symbol leaderboard on exit");
    }

    if let Some(ref cross) = yolo.all_time_cross_symbol_leaderboard {
        if let Err(e) = cross.save(cross_symbol_path) {
            tracing::warn!("Failed to save all-time cross-symbol leaderboard: {}", e);
        } else {
            tracing::info!("Saved all-time cross-symbol leaderboard on exit");
        }
    }
}

/// Handle a key press event.
fn handle_key(app: &mut App, code: KeyCode, channels: &WorkerChannels) -> KeyResult {
    // Handle startup modal
    if app.startup.active {
        return handle_startup_key(app, code, channels);
    }

    // Handle YOLO config modal
    if app.yolo.show_config {
        return handle_yolo_config_key(app, code, channels);
    }

    // Handle search mode in Data panel
    if app.active_panel == Panel::Data && app.data.search_mode {
        return handle_search_key(app, code, channels);
    }

    // Handle Help panel search mode
    if app.active_panel == Panel::Help && app.help.search_mode {
        return handle_help_search_key(app, code);
    }

    match code {
        KeyCode::Char('q') => {
            // Save all-time leaderboards before quitting to ensure persistence
            save_leaderboards_on_exit(&app.yolo);
            KeyResult::Quit
        }

        KeyCode::Esc => {
            // Signal cancellation to worker (stops YOLO mode and any running sweep)
            channels.cancel_flag.store(true, Ordering::SeqCst);
            let _ = channels.command_tx.send(WorkerCommand::Cancel);
            // If YOLO mode is running, it will send YoloStopped update
            // which will set yolo.enabled = false
            app.handle_escape();
            KeyResult::Continue
        }

        KeyCode::Tab => {
            // In Strategy panel, Tab toggles between strategy list and parameters
            if !app.toggle_strategy_focus() {
                app.next_panel();
            }
            KeyResult::Continue
        }

        KeyCode::BackTab => {
            // In Strategy panel, BackTab goes from params back to strategy selection
            if app.active_panel == Panel::Strategy
                && app.strategy.focus == StrategyFocus::Parameters
            {
                app.strategy.focus = StrategyFocus::Selection;
                app.strategy.editing_strategy = true;
            } else {
                app.prev_panel();
            }
            KeyResult::Continue
        }

        KeyCode::Char('1') => {
            app.select_panel(0);
            KeyResult::Continue
        }
        KeyCode::Char('2') => {
            app.select_panel(1);
            KeyResult::Continue
        }
        KeyCode::Char('3') => {
            app.select_panel(2);
            KeyResult::Continue
        }
        KeyCode::Char('4') => {
            app.select_panel(3);
            KeyResult::Continue
        }
        KeyCode::Char('5') => {
            app.select_panel(4);
            KeyResult::Continue
        }
        KeyCode::Char('6') | KeyCode::Char('?') => {
            app.select_panel(5);
            KeyResult::Continue
        }

        KeyCode::Up | KeyCode::Char('k') => {
            app.handle_up();
            KeyResult::Continue
        }

        KeyCode::Down | KeyCode::Char('j') => {
            app.handle_down();
            KeyResult::Continue
        }

        KeyCode::Left | KeyCode::Char('h') => {
            app.handle_left();
            KeyResult::Continue
        }

        KeyCode::Right | KeyCode::Char('l') => {
            app.handle_right();
            KeyResult::Continue
        }

        KeyCode::Enter => {
            // Enter to expand/collapse categories or confirm actions
            if app.active_panel == Panel::Strategy
                && app.strategy.focus == StrategyFocus::Selection
                && app.strategy.focus_on_category
            {
                app.handle_strategy_enter();
            } else {
                app.handle_enter_with_channels(channels);
            }
            KeyResult::Continue
        }

        KeyCode::Char('f') => {
            // 'f' for fetch data
            app.handle_fetch(channels);
            KeyResult::Continue
        }

        KeyCode::Char('s') => {
            // 's' for sort (in results panel) or search (in data panel)
            if app.active_panel == Panel::Data {
                // Enter search mode
                app.data.search_mode = true;
                app.data.search_input.clear();
                app.data.search_suggestions.clear();
                app.data.search_selected = 0;
                app.status_message = "Type to search symbols...".to_string();
            } else {
                app.handle_sort();
            }
            KeyResult::Continue
        }

        KeyCode::Char('y') | KeyCode::Char('Y') => {
            // 'y' for YOLO mode - show config modal
            if !app.yolo.enabled && !app.sweep.is_running {
                // Initialize config from current app state
                app.yolo.config.start_date = app.fetch_range.0;
                app.yolo.config.end_date = app.fetch_range.1;
                app.yolo.config.randomization_pct = app.yolo.randomization_pct;
                app.yolo.config.focused_field = YoloConfigField::StartDate;
                app.yolo.show_config = true;
            } else if app.yolo.enabled {
                app.status_message = "YOLO mode running. Press ESC to stop.".to_string();
            } else if app.sweep.is_running {
                app.status_message =
                    "Sweep already running. Press ESC to cancel first.".to_string();
            }
            KeyResult::Continue
        }

        KeyCode::Char('d') => {
            // 'd' for toggle drawdown (in chart panel)
            app.handle_toggle_drawdown();
            KeyResult::Continue
        }

        KeyCode::Char('m') => {
            // 'm' for toggle chart mode (single vs multi-ticker vs portfolio)
            app.handle_toggle_chart_mode();
            KeyResult::Continue
        }

        KeyCode::Char('v') => {
            // 'v' for toggle view mode (in results panel) or volume (in chart panel)
            if app.active_panel == Panel::Chart {
                app.handle_toggle_volume();
            } else {
                app.handle_toggle_view();
            }
            KeyResult::Continue
        }

        KeyCode::Char('t') => {
            // 't' for toggle leaderboard scope (Session vs All-Time) in Results panel
            if app.active_panel == Panel::Results {
                app.yolo.toggle_scope();
            }
            KeyResult::Continue
        }

        KeyCode::Char('p') => {
            // 'p' for cycle risk profile (Balanced -> Conservative -> Aggressive -> TrendOptions)
            if app.active_panel == Panel::Results {
                app.yolo.risk_profile = app.yolo.risk_profile.next();
                app.status_message = format!(
                    "Risk profile: {} (affects composite score ranking)",
                    app.yolo.risk_profile.display_name()
                );
            }
            KeyResult::Continue
        }

        KeyCode::Char('P') => {
            // 'P' (Shift+P) for Pine Script generation from selected result
            if app.active_panel == Panel::Results {
                app.handle_pine_export();
            }
            KeyResult::Continue
        }

        KeyCode::Char('R') => {
            // Reset to canonical defaults (lookbacks, grids, fetch range)
            app.reset_ui_defaults();
            KeyResult::Continue
        }

        KeyCode::Char('c') => {
            // 'c' for toggle crosshair (in chart panel)
            if app.active_panel == Panel::Chart {
                app.handle_toggle_crosshair();
            }
            KeyResult::Continue
        }

        KeyCode::Char(' ') => {
            // Space to toggle selection
            if app.active_panel == Panel::Strategy && app.strategy.focus == StrategyFocus::Selection
            {
                app.handle_strategy_space();
            } else {
                app.handle_space();
            }
            KeyResult::Continue
        }

        KeyCode::Char('a') => {
            // 'a' to select all in current context, or toggle analysis in Results panel
            if app.active_panel == Panel::Results {
                // Toggle analysis view
                app.handle_toggle_analysis(channels);
            } else if app.active_panel == Panel::Strategy
                && app.strategy.focus == StrategyFocus::Selection
            {
                app.handle_strategy_select_all();
            } else {
                app.handle_select_all();
            }
            KeyResult::Continue
        }

        KeyCode::Char('n') => {
            // 'n' to deselect all in current context, or next search match in Help
            if app.active_panel == Panel::Help && !app.help.search_query.is_empty() {
                app.help_next_match();
            } else if app.active_panel == Panel::Strategy
                && app.strategy.focus == StrategyFocus::Selection
            {
                app.handle_strategy_select_none();
            } else {
                app.handle_select_none();
            }
            KeyResult::Continue
        }

        KeyCode::Char('N') => {
            // 'N' for previous search match in Help panel
            if app.active_panel == Panel::Help && !app.help.search_query.is_empty() {
                app.help_prev_match();
            }
            KeyResult::Continue
        }

        KeyCode::Char('/') => {
            // '/' to enter search mode in Help panel
            if app.active_panel == Panel::Help {
                app.help.search_mode = true;
                app.help.search_query.clear();
                app.help.search_matches.clear();
                app.help.search_index = 0;
            }
            KeyResult::Continue
        }

        KeyCode::Char('g') => {
            // 'g' for gg (jump to top) in Help panel - need to track double press
            if app.active_panel == Panel::Help {
                // Simple implementation: single 'g' jumps to top
                app.help.scroll_offset = 0;
            }
            KeyResult::Continue
        }

        KeyCode::Char('G') => {
            // 'G' to jump to bottom in Help panel
            if app.active_panel == Panel::Help {
                app.help_jump_to_bottom();
            }
            KeyResult::Continue
        }

        KeyCode::Char('e') => {
            // 'e' to toggle ensemble mode in Strategy panel
            if app.active_panel == Panel::Strategy {
                app.handle_toggle_ensemble();
            }
            KeyResult::Continue
        }

        _ => KeyResult::Continue,
    }
}

/// Handle keys when in search mode.
fn handle_search_key(app: &mut App, code: KeyCode, channels: &WorkerChannels) -> KeyResult {
    match code {
        KeyCode::Esc => {
            // Exit search mode
            app.data.search_mode = false;
            app.data.search_input.clear();
            app.data.search_suggestions.clear();
            app.status_message = "Search cancelled.".to_string();
            KeyResult::Continue
        }

        KeyCode::Enter => {
            // Select the highlighted suggestion
            if let Some(suggestion) = app.data.search_suggestions.get(app.data.search_selected) {
                let symbol = suggestion.symbol.clone();

                // Add to symbols list if not present
                if !app.data.symbols.contains(&symbol) {
                    app.data.symbols.push(symbol.clone());
                }

                // Select it
                if let Some(idx) = app.data.symbols.iter().position(|s| s == &symbol) {
                    app.data.selected_index = idx;
                }

                app.status_message = format!("Added {}", symbol);
            }

            // Exit search mode
            app.data.search_mode = false;
            app.data.search_input.clear();
            app.data.search_suggestions.clear();
            KeyResult::Continue
        }

        KeyCode::Up => {
            if app.data.search_selected > 0 {
                app.data.search_selected -= 1;
            }
            KeyResult::Continue
        }

        KeyCode::Down => {
            if !app.data.search_suggestions.is_empty()
                && app.data.search_selected < app.data.search_suggestions.len() - 1
            {
                app.data.search_selected += 1;
            }
            KeyResult::Continue
        }

        KeyCode::Backspace => {
            app.data.search_input.pop();
            trigger_search(app, channels);
            KeyResult::Continue
        }

        KeyCode::Char(c) => {
            app.data.search_input.push(c);
            trigger_search(app, channels);
            KeyResult::Continue
        }

        _ => KeyResult::Continue,
    }
}

/// Trigger a search if input is long enough.
fn trigger_search(app: &mut App, channels: &WorkerChannels) {
    if !app.data.search_input.is_empty() {
        app.data.search_loading = true;
        let _ = channels.command_tx.send(WorkerCommand::SearchSymbols {
            query: app.data.search_input.clone(),
        });
    } else {
        app.data.search_suggestions.clear();
        app.data.search_loading = false;
    }
}

/// Handle keys in Help panel search mode.
fn handle_help_search_key(app: &mut App, code: KeyCode) -> KeyResult {
    match code {
        KeyCode::Esc => {
            // Exit search mode
            app.help.search_mode = false;
            KeyResult::Continue
        }

        KeyCode::Enter => {
            // Confirm search and exit search mode
            app.help.search_mode = false;
            // Jump to first match if any
            if !app.help.search_matches.is_empty() {
                app.help.search_index = 0;
                if let Some(&line) = app.help.search_matches.first() {
                    app.help.scroll_offset = line;
                }
            }
            KeyResult::Continue
        }

        KeyCode::Backspace => {
            app.help.search_query.pop();
            app.update_help_search_matches();
            KeyResult::Continue
        }

        KeyCode::Char(c) => {
            app.help.search_query.push(c);
            app.update_help_search_matches();
            KeyResult::Continue
        }

        _ => KeyResult::Continue,
    }
}

/// Handle keys while the startup modal is active.
fn handle_startup_key(app: &mut App, code: KeyCode, channels: &WorkerChannels) -> KeyResult {
    use trendlab_core::SweepDepth;

    match code {
        KeyCode::Esc => {
            app.startup.active = false;
            app.status_message = "Manual mode. Use panels to configure and run sweeps.".to_string();
            KeyResult::Continue
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.startup.mode = StartupMode::Manual;
            KeyResult::Continue
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.startup.mode = StartupMode::FullAuto;
            KeyResult::Continue
        }
        KeyCode::Char('[') => {
            // Cycle sweep depth backward
            if app.startup.mode == StartupMode::FullAuto {
                let depths = SweepDepth::all();
                let current_idx = depths
                    .iter()
                    .position(|d| *d == app.startup.sweep_depth)
                    .unwrap_or(1);
                if current_idx > 0 {
                    app.startup.sweep_depth = depths[current_idx - 1];
                }
            }
            KeyResult::Continue
        }
        KeyCode::Char(']') => {
            // Cycle sweep depth forward
            if app.startup.mode == StartupMode::FullAuto {
                let depths = SweepDepth::all();
                let current_idx = depths
                    .iter()
                    .position(|d| *d == app.startup.sweep_depth)
                    .unwrap_or(1);
                if current_idx < depths.len() - 1 {
                    app.startup.sweep_depth = depths[current_idx + 1];
                }
            }
            KeyResult::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.startup.mode == StartupMode::FullAuto && app.startup.selected_strategy_index > 0
            {
                app.startup.selected_strategy_index -= 1;
                // Update strategy_selection based on index
                let options = StrategySelection::all_options();
                if let Some(sel) = options.get(app.startup.selected_strategy_index) {
                    app.startup.strategy_selection = *sel;
                }
            }
            KeyResult::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.startup.mode == StartupMode::FullAuto {
                let options = StrategySelection::all_options();
                let max_idx = options.len().saturating_sub(1);
                if app.startup.selected_strategy_index < max_idx {
                    app.startup.selected_strategy_index += 1;
                    // Update strategy_selection based on index
                    if let Some(sel) = options.get(app.startup.selected_strategy_index) {
                        app.startup.strategy_selection = *sel;
                    }
                }
            }
            KeyResult::Continue
        }
        KeyCode::Enter => {
            if app.startup.mode == StartupMode::Manual {
                app.startup.active = false;
                app.status_message =
                    "Manual mode. Select data, strategy, then run sweeps.".to_string();
                return KeyResult::Continue;
            }

            // Full-auto: set strategy_selection from selected index
            let options = StrategySelection::all_options();
            if let Some(sel) = options.get(app.startup.selected_strategy_index) {
                app.startup.strategy_selection = *sel;

                // Also set the strategy.selected_type if a single strategy is chosen
                if let StrategySelection::Single(st) = sel {
                    app.strategy.selected_type = *st;
                    // Find the index in StrategyType::all()
                    if let Some(idx) = StrategyType::all().iter().position(|s| s == st) {
                        app.strategy.selected_type_index = idx;
                    }
                }
            }

            app.startup.active = false;
            app.start_full_auto(channels);
            KeyResult::Continue
        }
        _ => KeyResult::Continue,
    }
}

/// Handle key input for YOLO config modal
fn handle_yolo_config_key(app: &mut App, code: KeyCode, channels: &WorkerChannels) -> KeyResult {
    use trendlab_core::SweepDepth;

    match code {
        KeyCode::Esc => {
            // Close modal without starting YOLO
            app.yolo.show_config = false;
            app.status_message = "YOLO config cancelled.".to_string();
            KeyResult::Continue
        }

        KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
            // Move to next field
            app.yolo.config.focused_field = app.yolo.config.focused_field.next();
            KeyResult::Continue
        }

        KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => {
            // Move to previous field
            app.yolo.config.focused_field = app.yolo.config.focused_field.prev();
            KeyResult::Continue
        }

        KeyCode::Left | KeyCode::Char('h') => {
            // Decrease value
            match app.yolo.config.focused_field {
                YoloConfigField::StartDate => {
                    // Move start date back 30 days
                    app.yolo.config.start_date -= chrono::Duration::days(30);
                }
                YoloConfigField::EndDate => {
                    // Move end date back 30 days (but not before start)
                    let new_end = app.yolo.config.end_date - chrono::Duration::days(30);
                    if new_end > app.yolo.config.start_date {
                        app.yolo.config.end_date = new_end;
                    }
                }
                YoloConfigField::Randomization => {
                    // Decrease by 5%
                    app.yolo.config.randomization_pct =
                        (app.yolo.config.randomization_pct - 0.05).max(0.0);
                }
                YoloConfigField::WfSharpeThreshold => {
                    // Decrease by 0.05 (floor at 0.15)
                    app.yolo.config.wf_sharpe_threshold =
                        (app.yolo.config.wf_sharpe_threshold - 0.05).max(0.15);
                }
                YoloConfigField::SweepDepth => {
                    // Cycle to previous depth
                    let depths = SweepDepth::all();
                    let current_idx = depths
                        .iter()
                        .position(|d| *d == app.yolo.config.sweep_depth)
                        .unwrap_or(0);
                    if current_idx > 0 {
                        app.yolo.config.sweep_depth = depths[current_idx - 1];
                    }
                }
                YoloConfigField::PolarsThreads => {
                    // Decrease or clear to auto
                    app.yolo.config.polars_max_threads = match app.yolo.config.polars_max_threads {
                        Some(v) if v > 1 => Some(v - 1),
                        _ => None,
                    };
                }
                YoloConfigField::OuterThreads => {
                    // Decrease or clear to auto
                    app.yolo.config.outer_threads = match app.yolo.config.outer_threads {
                        Some(v) if v > 1 => Some(v - 1),
                        _ => None,
                    };
                }
            }
            KeyResult::Continue
        }

        KeyCode::Right | KeyCode::Char('l') => {
            // Increase value
            match app.yolo.config.focused_field {
                YoloConfigField::StartDate => {
                    // Move start date forward 30 days (but not past end)
                    let new_start = app.yolo.config.start_date + chrono::Duration::days(30);
                    if new_start < app.yolo.config.end_date {
                        app.yolo.config.start_date = new_start;
                    }
                }
                YoloConfigField::EndDate => {
                    // Move end date forward 30 days (but not past today)
                    let today = chrono::Local::now().date_naive();
                    let new_end = app.yolo.config.end_date + chrono::Duration::days(30);
                    if new_end <= today {
                        app.yolo.config.end_date = new_end;
                    } else {
                        app.yolo.config.end_date = today;
                    }
                }
                YoloConfigField::Randomization => {
                    // Increase by 5%
                    app.yolo.config.randomization_pct =
                        (app.yolo.config.randomization_pct + 0.05).min(1.0);
                }
                YoloConfigField::WfSharpeThreshold => {
                    // Increase by 0.05 (ceiling at 0.50)
                    app.yolo.config.wf_sharpe_threshold =
                        (app.yolo.config.wf_sharpe_threshold + 0.05).min(0.50);
                }
                YoloConfigField::SweepDepth => {
                    // Cycle to next depth
                    let depths = SweepDepth::all();
                    let current_idx = depths
                        .iter()
                        .position(|d| *d == app.yolo.config.sweep_depth)
                        .unwrap_or(0);
                    if current_idx < depths.len() - 1 {
                        app.yolo.config.sweep_depth = depths[current_idx + 1];
                    }
                }
                YoloConfigField::PolarsThreads => {
                    let max_threads = std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(128);
                    let base = app.yolo.config.polars_max_threads.unwrap_or(4);
                    app.yolo.config.polars_max_threads = Some((base + 1).min(max_threads));
                }
                YoloConfigField::OuterThreads => {
                    let max_threads = std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(256);
                    let base = app.yolo.config.outer_threads.unwrap_or(8);
                    app.yolo.config.outer_threads = Some((base + 1).min(max_threads));
                }
            }
            KeyResult::Continue
        }

        KeyCode::Enter => {
            // Apply config and start YOLO mode
            app.fetch_range = (app.yolo.config.start_date, app.yolo.config.end_date);
            app.yolo.randomization_pct = app.yolo.config.randomization_pct;
            app.yolo.wf_sharpe_threshold = app.yolo.config.wf_sharpe_threshold;
            app.yolo.polars_max_threads = app.yolo.config.polars_max_threads;
            app.yolo.outer_threads = app.yolo.config.outer_threads;
            app.yolo.show_config = false;
            app.start_yolo_mode(channels);
            KeyResult::Continue
        }

        _ => KeyResult::Continue,
    }
}

/// Apply a worker update to the app state.
fn apply_update(app: &mut App, update: WorkerUpdate, channels: &WorkerChannels) {
    match update {
        WorkerUpdate::Ready => {
            app.status_message = "Worker ready.".to_string();
        }

        WorkerUpdate::Idle => {
            // Worker finished an operation
        }

        // Search updates
        WorkerUpdate::SearchResults { query, results } => {
            // Only apply if query matches current search input
            if app.data.search_input == query {
                app.data.search_loading = false;
                app.data.search_suggestions = results
                    .into_iter()
                    .map(|r| SearchSuggestion {
                        symbol: r.symbol,
                        name: r.name,
                        exchange: r.exchange,
                        type_disp: r.type_disp,
                    })
                    .collect();
                app.data.search_selected = 0;
            }
        }

        WorkerUpdate::SearchError { query, error } => {
            if app.data.search_input == query {
                app.data.search_loading = false;
                app.set_status_error(format!("Search error: {}", error));
            }
        }

        // Fetch updates
        WorkerUpdate::FetchStarted {
            symbol,
            index,
            total,
        } => {
            app.status_message = format!("Fetching {} ({}/{})", symbol, index + 1, total);
            app.operation = OperationState::FetchingData {
                current_symbol: symbol,
                completed: index,
                total,
            };
        }

        WorkerUpdate::FetchComplete {
            symbol,
            bars,
            quality,
        } => {
            let bar_count = bars.len();
            let issues = if quality.is_clean() {
                "clean".to_string()
            } else {
                format!("{} issues", quality.issues.len())
            };
            app.status_message = format!("{}: {} bars ({})", symbol, bar_count, issues);

            // Store bars in cache
            app.data.bars_cache.insert(symbol.clone(), bars);

            // Update symbols list if not already present
            if !app.data.symbols.contains(&symbol) {
                app.data.symbols.push(symbol);
            }
        }

        WorkerUpdate::FetchError { symbol, error } => {
            app.set_status_error(format!("Error fetching {}: {}", symbol, error));
        }

        WorkerUpdate::FetchAllComplete { symbols_fetched } => {
            app.set_status_success(format!("Fetch complete: {} symbols", symbols_fetched));
            app.operation = OperationState::Idle;

            // Full-auto: if we were fetching missing symbols, start the appropriate sweep.
            if app.auto.enabled && app.auto.stage == AutoStage::FetchingMissing {
                app.auto.stage = AutoStage::Sweeping;
                match app.startup.strategy_selection {
                    StrategySelection::AllStrategies => {
                        app.start_multi_strategy_sweep(channels);
                    }
                    StrategySelection::Single(_) => {
                        app.start_multi_sweep(channels);
                    }
                }
            }
        }

        // Cache load updates
        WorkerUpdate::CacheLoadStarted {
            symbol,
            index,
            total,
        } => {
            app.status_message = format!("Loading cache {} ({}/{})", symbol, index + 1, total);
        }

        WorkerUpdate::CacheLoadComplete { symbol, bars } => {
            let bar_count = bars.len();
            app.status_message = format!("Cache loaded {}: {} bars", symbol, bar_count);
            app.data.bars_cache.insert(symbol.clone(), bars);
            if !app.data.symbols.contains(&symbol) {
                app.data.symbols.push(symbol);
            }
        }

        WorkerUpdate::CacheLoadError { symbol, error } => {
            // For full-auto we treat missing cache as expected; we will fetch later.
            app.set_status_warning(format!("Cache miss {}: {}", symbol, error));
            if app.auto.enabled && app.auto.stage == AutoStage::LoadingCache {
                app.auto.pending_missing.push(symbol);
            }
        }

        WorkerUpdate::CacheLoadAllComplete {
            symbols_loaded,
            symbols_missing,
        } => {
            app.set_status_success(format!(
                "Cache load complete: {} loaded, {} missing",
                symbols_loaded, symbols_missing
            ));

            if app.auto.enabled && app.auto.stage == AutoStage::LoadingCache {
                // Fetch missing tickers (if any), then start the appropriate sweep.
                if !app.auto.pending_missing.is_empty() {
                    let missing = std::mem::take(&mut app.auto.pending_missing);
                    app.auto.stage = AutoStage::FetchingMissing;
                    let (start, end) = app.fetch_range();
                    let _ = channels.command_tx.send(WorkerCommand::FetchData {
                        symbols: missing.clone(),
                        start,
                        end,
                        force: false,
                    });
                    app.status_message =
                        format!("Full-Auto: fetching {} missing tickers...", missing.len());
                } else {
                    app.auto.stage = AutoStage::Sweeping;
                    match app.startup.strategy_selection {
                        StrategySelection::AllStrategies => {
                            app.start_multi_strategy_sweep(channels);
                        }
                        StrategySelection::Single(_) => {
                            app.start_multi_sweep(channels);
                        }
                    }
                }
            }
        }

        // Sweep updates
        WorkerUpdate::SweepStarted { total_configs } => {
            app.status_message = format!("Starting sweep: {} configs", total_configs);
            app.sweep.is_running = true;
            app.sweep.total_configs = total_configs;
            app.sweep.completed_configs = 0;
            app.sweep.progress = 0.0;
            app.operation = OperationState::RunningSweep {
                completed: 0,
                total: total_configs,
            };
        }

        WorkerUpdate::SweepProgress { completed, total } => {
            app.sweep.completed_configs = completed;
            app.sweep.total_configs = total;
            app.sweep.progress = completed as f64 / total as f64;
            app.status_message = format!("Sweep: {}/{} configs", completed, total);

            if let OperationState::RunningSweep {
                completed: ref mut c,
                total: ref mut t,
            } = app.operation
            {
                *c = completed;
                *t = total;
            }
        }

        WorkerUpdate::SweepComplete { result } => {
            let count = result.config_results.len();
            app.set_status_success(format!("Sweep complete: {} configs", count));
            app.sweep.is_running = false;
            app.sweep.progress = 1.0;
            app.operation = OperationState::Idle;

            // Transfer results to results panel
            app.results.results = result.config_results;
            app.results.selected_index = 0;
        }

        WorkerUpdate::SweepCancelled { completed } => {
            app.set_status_warning(format!("Sweep cancelled after {} configs", completed));
            app.sweep.is_running = false;
            app.operation = OperationState::Idle;
        }

        // Multi-sweep updates
        WorkerUpdate::MultiSweepStarted {
            total_symbols,
            configs_per_symbol,
        } => {
            app.status_message = format!(
                "Starting multi-sweep: {} symbols x {} configs",
                total_symbols, configs_per_symbol
            );
            app.sweep.is_running = true;
        }

        WorkerUpdate::MultiSweepSymbolStarted {
            symbol,
            symbol_index,
            total_symbols,
        } => {
            app.status_message = format!(
                "Sweeping {} ({}/{})",
                symbol,
                symbol_index + 1,
                total_symbols
            );
        }

        WorkerUpdate::MultiSweepSymbolComplete { symbol, result } => {
            app.status_message =
                format!("{}: {} configs tested", symbol, result.config_results.len());
        }

        WorkerUpdate::MultiSweepComplete { result } => {
            use trendlab_core::RankMetric;

            let symbol_count = result.symbol_count();
            let total_configs = result.total_configs();
            app.status_message = format!(
                "Multi-sweep complete: {} symbols, {} configs",
                symbol_count, total_configs
            );
            app.sweep.is_running = false;
            app.operation = OperationState::Idle;

            // Store multi-sweep result and derive summaries
            app.results.multi_sweep_result = Some(result.clone());
            app.results.update_ticker_summaries();
            app.results.view_mode = ResultsViewMode::PerTicker;
            app.results.selected_ticker_index = 0;

            // Also store first symbol's detailed results for drill-down
            if let Some((_symbol, sweep_result)) = result.symbol_results.iter().next() {
                app.results.results = sweep_result.config_results.clone();
                app.results.selected_index = 0;
            }

            // Populate synthesized multi-curve chart: best config per symbol + aggregated portfolio.
            let mut curves: Vec<TickerCurve> = Vec::new();
            for (symbol, sweep_result) in &result.symbol_results {
                if let Some(best) = sweep_result.top_n(1, RankMetric::Sharpe, false).first() {
                    let equity: Vec<f64> = best
                        .backtest_result
                        .equity
                        .iter()
                        .map(|p| p.equity)
                        .collect();
                    let dates: Vec<chrono::DateTime<chrono::Utc>> =
                        best.backtest_result.equity.iter().map(|p| p.ts).collect();
                    curves.push(TickerCurve {
                        symbol: symbol.clone(),
                        equity,
                        dates,
                    });
                }
            }
            curves.sort_by(|a, b| a.symbol.cmp(&b.symbol));
            app.chart.ticker_curves = curves;
            app.chart.portfolio_curve = result
                .aggregated
                .as_ref()
                .map(|a| a.equity_curve.clone())
                .unwrap_or_default();
            app.chart.view_mode = ChartViewMode::MultiTicker;

            if app.auto.enabled && app.auto.jump_to_chart_on_complete {
                app.active_panel = Panel::Chart;
                app.auto.stage = AutoStage::Idle;
                app.auto.enabled = false;
                app.status_message = format!(
                    "Full-Auto complete: showing combined chart ({} symbols)",
                    symbol_count
                );
            }
        }

        WorkerUpdate::MultiSweepCancelled { completed_symbols } => {
            app.status_message =
                format!("Multi-sweep cancelled after {} symbols", completed_symbols);
            app.sweep.is_running = false;
            app.operation = OperationState::Idle;
        }

        // Multi-strategy sweep updates
        WorkerUpdate::MultiStrategySweepStarted {
            total_symbols,
            total_strategies,
            total_configs,
        } => {
            app.status_message = format!(
                "Starting multi-strategy sweep: {} symbols x {} strategies ({} configs)",
                total_symbols, total_strategies, total_configs
            );
            app.sweep.is_running = true;
        }

        WorkerUpdate::MultiStrategySweepStrategyStarted {
            symbol,
            strategy_type,
        } => {
            app.status_message = format!("Sweeping {} with {}", symbol, strategy_type.name());
        }

        WorkerUpdate::MultiStrategySweepProgress {
            completed_configs,
            total_configs,
            current_strategy,
            current_symbol,
        } => {
            let pct = (completed_configs as f64 / total_configs as f64 * 100.0) as usize;
            app.status_message = format!(
                "Progress: {}% ({}/{}) - {} on {}",
                pct,
                completed_configs,
                total_configs,
                current_strategy.name(),
                current_symbol
            );
        }

        WorkerUpdate::MultiStrategySweepComplete { result } => {
            let symbol_count = result.best_per_symbol.len();
            let strategy_count = result.best_per_strategy.len();
            app.status_message = format!(
                "Multi-strategy sweep complete: {} symbols x {} strategies",
                symbol_count, strategy_count
            );
            app.sweep.is_running = false;
            app.operation = OperationState::Idle;

            // Store multi-strategy result
            app.results.multi_strategy_result = Some(result.clone());

            // Populate strategy comparison curves (best config per strategy)
            let mut strategy_curves: Vec<StrategyCurve> = Vec::new();
            for entry in &result.strategy_comparison {
                if let Some(best) = result.best_per_strategy.get(&entry.strategy_type) {
                    strategy_curves.push(StrategyCurve {
                        strategy_type: entry.strategy_type,
                        config_display: best.config_id.display(),
                        equity: best.equity_curve.clone(),
                        dates: best.dates.clone(),
                        metrics: best.metrics.clone(),
                    });
                }
            }
            app.chart.strategy_curves = strategy_curves;

            // Populate per-ticker best strategy curves
            let mut ticker_best: Vec<TickerBestStrategy> = Vec::new();
            for (symbol, best) in &result.best_per_symbol {
                ticker_best.push(TickerBestStrategy {
                    symbol: symbol.clone(),
                    strategy_type: best.strategy_type,
                    config_display: best.config_id.display(),
                    equity: best.equity_curve.clone(),
                    dates: best.dates.clone(),
                    metrics: best.metrics.clone(),
                });
            }
            ticker_best.sort_by(|a, b| a.symbol.cmp(&b.symbol));
            app.chart.ticker_best_strategies = ticker_best;

            // Set chart to strategy comparison view
            app.chart.view_mode = ChartViewMode::StrategyComparison;

            // Auto-jump to chart if in full-auto mode
            if app.auto.enabled && app.auto.jump_to_chart_on_complete {
                app.active_panel = Panel::Chart;
                app.auto.stage = AutoStage::Idle;
                app.auto.enabled = false;
                app.status_message = format!(
                    "Full-Auto complete: {} strategies across {} symbols",
                    strategy_count, symbol_count
                );
            }
        }

        WorkerUpdate::MultiStrategySweepCancelled { completed_configs } => {
            app.status_message = format!(
                "Multi-strategy sweep cancelled after {} configs",
                completed_configs
            );
            app.sweep.is_running = false;
            app.operation = OperationState::Idle;
        }

        // Statistical analysis updates
        WorkerUpdate::AnalysisStarted { analysis_id } => {
            app.status_message = format!("Computing analysis for {}...", analysis_id);
        }

        WorkerUpdate::AnalysisComplete {
            analysis_id,
            analysis,
        } => {
            app.status_message = format!(
                "Analysis complete for {}: VaR95={:.2}%, Skew={:.2}",
                analysis_id,
                analysis.return_distribution.var_95 * 100.0,
                analysis.return_distribution.skewness
            );
            // Store analysis in cache
            app.results
                .analysis_cache
                .insert(analysis_id.clone(), analysis.clone());
            // If this matches the currently selected config, update selected_analysis
            if app.results.selected_analysis_id.as_ref() == Some(&analysis_id) {
                app.results.selected_analysis = Some(analysis);
            }
        }

        WorkerUpdate::AnalysisError { analysis_id, error } => {
            app.status_message = format!("Analysis failed for {}: {}", analysis_id, error);
        }

        // YOLO Mode updates
        WorkerUpdate::YoloModeStarted {
            total_symbols,
            total_strategies,
        } => {
            app.status_message = format!(
                "YOLO Mode started: {} symbols x {} strategies (press ESC to stop)",
                total_symbols, total_strategies
            );
            app.sweep.is_running = true;
            app.yolo.enabled = true;
            app.yolo.started_at = Some(chrono::Utc::now());
        }

        WorkerUpdate::YoloDataRefresh {
            symbols_needing_refresh,
            total_symbols,
            requested_start,
            requested_end,
        } => {
            app.status_message = format!(
                "YOLO: Refreshing data for {} of {} symbols ({} to {})...",
                symbols_needing_refresh.len(),
                total_symbols,
                requested_start.format("%Y-%m-%d"),
                requested_end.format("%Y-%m-%d")
            );
        }

        WorkerUpdate::YoloDataRefreshProgress {
            symbol,
            index,
            total,
        } => {
            app.status_message = format!(
                "YOLO: Refreshing {} ({}/{} need data)...",
                symbol,
                index + 1,
                total
            );
        }

        WorkerUpdate::YoloDataRefreshComplete {
            symbols_refreshed,
            symbols_failed,
        } => {
            if symbols_failed > 0 {
                app.status_message = format!(
                    "YOLO: Data refresh done ({} OK, {} failed). Starting sweeps...",
                    symbols_refreshed, symbols_failed
                );
            } else {
                app.status_message = format!(
                    "YOLO: Data refresh complete ({} symbols). Starting sweeps...",
                    symbols_refreshed
                );
            }
        }

        WorkerUpdate::YoloIterationComplete {
            iteration,
            best_aggregate,
            best_per_symbol: _,
            cross_symbol_leaderboard,
            per_symbol_leaderboard,
            configs_tested_this_round,
        } => {
            app.yolo.iteration = iteration;
            // Update both session and all-time leaderboards
            app.yolo.update_leaderboards(
                per_symbol_leaderboard.clone(),
                cross_symbol_leaderboard.clone(),
                configs_tested_this_round,
            );

            // Update status message with cross-symbol best
            let best_avg_sharpe = cross_symbol_leaderboard.best_avg_sharpe().unwrap_or(0.0);
            app.status_message = format!(
                "YOLO iter {}: {} configs | Best Avg Sharpe: {:.3} | ESC to stop",
                iteration, configs_tested_this_round, best_avg_sharpe
            );

            // Update chart with top 4 equity curves from cross-symbol leaderboard
            let mut strategy_curves: Vec<StrategyCurve> = Vec::new();
            for entry in &cross_symbol_leaderboard.entries {
                strategy_curves.push(StrategyCurve {
                    strategy_type: entry.strategy_type,
                    config_display: entry.config_id.display(),
                    equity: entry.combined_equity_curve.clone(),
                    dates: entry.dates.clone(),
                    metrics: trendlab_core::Metrics {
                        total_return: entry.aggregate_metrics.avg_cagr,
                        cagr: entry.aggregate_metrics.avg_cagr,
                        sharpe: entry.aggregate_metrics.avg_sharpe,
                        sortino: 0.0, // Not aggregated
                        max_drawdown: entry.aggregate_metrics.worst_max_drawdown,
                        calmar: 0.0, // Not aggregated
                        win_rate: entry.aggregate_metrics.hit_rate,
                        profit_factor: 0.0, // Not aggregated
                        num_trades: 0,
                        turnover: 0.0,
                        max_consecutive_losses: 0,
                        max_consecutive_wins: 0,
                        avg_losing_streak: 0.0,
                    },
                });
            }
            app.chart.strategy_curves = strategy_curves;
            app.chart.view_mode = ChartViewMode::StrategyComparison;

            // Set winning config from best entry for display in Statistics panel
            if let Some(best) = cross_symbol_leaderboard.entries.first() {
                app.chart.winning_config = Some(WinningConfig {
                    strategy_name: best.strategy_type.name().to_string(),
                    config_display: best.config_id.display(),
                    symbol: Some(format!("{} symbols", best.symbols.len())),
                });
            }

            // Log best this round if available
            if let Some(best) = best_aggregate {
                app.status_message = format!(
                    "YOLO iter {}: NEW {} (avg Sharpe {:.3} across {} symbols) | ESC to stop",
                    iteration,
                    best.strategy_type.name(),
                    best.aggregate_metrics.avg_sharpe,
                    best.symbols.len()
                );
            }
        }

        WorkerUpdate::YoloProgress {
            iteration,
            phase,
            completed_configs,
            total_configs,
            jitter_summary,
        } => {
            let pct = if total_configs > 0 {
                (completed_configs as f64 / total_configs as f64 * 100.0) as usize
            } else {
                0
            };
            // Include jitter summary if not empty to show what's being tested
            let summary_part = if jitter_summary.is_empty() {
                String::new()
            } else {
                format!(" | {}", jitter_summary)
            };
            app.status_message = format!(
                "YOLO iter {} [{}]: {}% ({}/{}){}  ESC to stop",
                iteration, phase, pct, completed_configs, total_configs, summary_part
            );
        }

        WorkerUpdate::YoloStopped {
            cross_symbol_leaderboard,
            per_symbol_leaderboard,
            total_iterations,
            total_configs_tested,
        } => {
            app.yolo.enabled = false;
            // Final update to session leaderboards (all-time already updated incrementally)
            app.yolo.session_leaderboard = per_symbol_leaderboard.clone();
            app.yolo.session_cross_symbol_leaderboard = Some(cross_symbol_leaderboard.clone());
            // Merge final results into all-time
            for entry in per_symbol_leaderboard.entries.iter() {
                app.yolo.all_time_leaderboard.try_insert(entry.clone());
            }
            if let Some(ref mut all_time_cross) = app.yolo.all_time_cross_symbol_leaderboard {
                for entry in cross_symbol_leaderboard.entries.iter() {
                    all_time_cross.try_insert(entry.clone());
                }
                // Update requested dates to match the current session
                if let (Some(start), Some(end)) = (
                    cross_symbol_leaderboard.requested_start,
                    cross_symbol_leaderboard.requested_end,
                ) {
                    all_time_cross.set_requested_range(start, end);
                }
            } else {
                app.yolo.all_time_cross_symbol_leaderboard = Some(cross_symbol_leaderboard.clone());
            }
            let _ = total_configs_tested; // Suppress unused warning
            let _ = total_iterations;
            app.sweep.is_running = false;
            app.operation = OperationState::Idle;

            let best_avg_sharpe = cross_symbol_leaderboard.best_avg_sharpe().unwrap_or(0.0);
            app.status_message = format!(
                "YOLO stopped: {} iterations, {} configs, best avg Sharpe: {:.3}",
                total_iterations, total_configs_tested, best_avg_sharpe
            );

            // Keep the final cross-symbol leaderboard curves on chart
            let mut strategy_curves: Vec<StrategyCurve> = Vec::new();
            for entry in &cross_symbol_leaderboard.entries {
                strategy_curves.push(StrategyCurve {
                    strategy_type: entry.strategy_type,
                    config_display: entry.config_id.display(),
                    equity: entry.combined_equity_curve.clone(),
                    dates: entry.dates.clone(),
                    metrics: trendlab_core::Metrics {
                        total_return: entry.aggregate_metrics.avg_cagr,
                        cagr: entry.aggregate_metrics.avg_cagr,
                        sharpe: entry.aggregate_metrics.avg_sharpe,
                        sortino: 0.0,
                        max_drawdown: entry.aggregate_metrics.worst_max_drawdown,
                        calmar: 0.0,
                        win_rate: entry.aggregate_metrics.hit_rate,
                        profit_factor: 0.0,
                        num_trades: 0,
                        turnover: 0.0,
                        max_consecutive_losses: 0,
                        max_consecutive_wins: 0,
                        avg_losing_streak: 0.0,
                    },
                });
            }
            app.chart.strategy_curves = strategy_curves;

            // Set winning config from best entry for display in Statistics panel
            if let Some(best) = cross_symbol_leaderboard.entries.first() {
                app.chart.winning_config = Some(WinningConfig {
                    strategy_name: best.strategy_type.name().to_string(),
                    config_display: best.config_id.display(),
                    symbol: Some(format!("{} symbols", best.symbols.len())),
                });
            }
        }
    }
}
