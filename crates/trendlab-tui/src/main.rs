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
    event::{self, poll, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::atomic::Ordering;
use std::time::Duration;

mod app;
mod panels;
mod ui;
mod worker;

use app::App;
use worker::{spawn_worker, WorkerChannels, WorkerCommand, WorkerUpdate};

fn main() -> Result<()> {
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

        // 2. Poll for keyboard input (non-blocking, 16ms timeout for ~60fps)
        if poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match handle_key(app, key.code, channels) {
                        KeyResult::Quit => return Ok(()),
                        KeyResult::Continue => {}
                    }
                }
            }
        }

        // 3. Drain all pending updates from worker (non-blocking)
        while let Ok(update) = channels.update_rx.try_recv() {
            apply_update(app, update);
        }
    }
}

/// Result of handling a key press.
enum KeyResult {
    Quit,
    Continue,
}

/// Handle a key press event.
fn handle_key(app: &mut App, code: KeyCode, channels: &WorkerChannels) -> KeyResult {
    // Handle search mode in Data panel
    if app.active_panel == app::Panel::Data && app.data.search_mode {
        return handle_search_key(app, code, channels);
    }

    match code {
        KeyCode::Char('q') => KeyResult::Quit,

        KeyCode::Esc => {
            // Signal cancellation to worker
            channels.cancel_flag.store(true, Ordering::SeqCst);
            let _ = channels.command_tx.send(WorkerCommand::Cancel);
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
            // In Strategy panel, BackTab goes from params back to strategy list
            if app.active_panel == app::Panel::Strategy && !app.strategy.editing_strategy {
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
            app.handle_enter_with_channels(channels);
            KeyResult::Continue
        }

        KeyCode::Char('f') => {
            // 'f' for fetch data
            app.handle_fetch(channels);
            KeyResult::Continue
        }

        KeyCode::Char('s') => {
            // 's' for sort (in results panel) or search (in data panel)
            if app.active_panel == app::Panel::Data {
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
            // 'v' for toggle view mode (in results panel)
            app.handle_toggle_view();
            KeyResult::Continue
        }

        KeyCode::Char(' ') => {
            // Space to toggle ticker selection in Data panel
            app.handle_space();
            KeyResult::Continue
        }

        KeyCode::Char('a') => {
            // 'a' to select all tickers in current sector
            app.handle_select_all();
            KeyResult::Continue
        }

        KeyCode::Char('n') => {
            // 'n' to deselect all tickers in current sector
            app.handle_select_none();
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

/// Apply a worker update to the app state.
fn apply_update(app: &mut App, update: WorkerUpdate) {
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
                    .map(|r| app::SearchSuggestion {
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
                app.status_message = format!("Search error: {}", error);
            }
        }

        // Fetch updates
        WorkerUpdate::FetchStarted {
            symbol,
            index,
            total,
        } => {
            app.status_message = format!("Fetching {} ({}/{})", symbol, index + 1, total);
            app.operation = app::OperationState::FetchingData {
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
            app.status_message = format!("Error fetching {}: {}", symbol, error);
        }

        WorkerUpdate::FetchAllComplete { symbols_fetched } => {
            app.status_message = format!("Fetch complete: {} symbols", symbols_fetched);
            app.operation = app::OperationState::Idle;
        }

        // Sweep updates
        WorkerUpdate::SweepStarted { total_configs } => {
            app.status_message = format!("Starting sweep: {} configs", total_configs);
            app.sweep.is_running = true;
            app.sweep.total_configs = total_configs;
            app.sweep.completed_configs = 0;
            app.sweep.progress = 0.0;
            app.operation = app::OperationState::RunningSweep {
                completed: 0,
                total: total_configs,
            };
        }

        WorkerUpdate::SweepProgress { completed, total } => {
            app.sweep.completed_configs = completed;
            app.sweep.total_configs = total;
            app.sweep.progress = completed as f64 / total as f64;
            app.status_message = format!("Sweep: {}/{} configs", completed, total);

            if let app::OperationState::RunningSweep {
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
            app.status_message = format!("Sweep complete: {} configs", count);
            app.sweep.is_running = false;
            app.sweep.progress = 1.0;
            app.operation = app::OperationState::Idle;

            // Transfer results to results panel
            app.results.results = result.config_results;
            app.results.selected_index = 0;
        }

        WorkerUpdate::SweepCancelled { completed } => {
            app.status_message = format!("Sweep cancelled after {} configs", completed);
            app.sweep.is_running = false;
            app.operation = app::OperationState::Idle;
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
            app.status_message = format!(
                "{}: {} configs tested",
                symbol,
                result.config_results.len()
            );
        }

        WorkerUpdate::MultiSweepComplete { result } => {
            let symbol_count = result.symbol_count();
            let total_configs = result.total_configs();
            app.status_message = format!(
                "Multi-sweep complete: {} symbols, {} configs",
                symbol_count, total_configs
            );
            app.sweep.is_running = false;
            app.operation = app::OperationState::Idle;

            // Store multi-sweep result and derive summaries
            app.results.multi_sweep_result = Some(result.clone());
            app.results.update_ticker_summaries();
            app.results.view_mode = app::ResultsViewMode::PerTicker;
            app.results.selected_ticker_index = 0;

            // Also store first symbol's detailed results for drill-down
            if let Some((_symbol, sweep_result)) = result.symbol_results.iter().next() {
                app.results.results = sweep_result.config_results.clone();
                app.results.selected_index = 0;
            }
        }

        WorkerUpdate::MultiSweepCancelled { completed_symbols } => {
            app.status_message = format!("Multi-sweep cancelled after {} symbols", completed_symbols);
            app.sweep.is_running = false;
            app.operation = app::OperationState::Idle;
        }
    }
}
