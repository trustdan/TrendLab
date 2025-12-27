# TrendLab Tauri GUI Roadmap

A desktop GUI for TrendLab using Tauri v2 + React + TypeScript, mirroring the TUI experience with polished web aesthetics.

---

## Guiding Principles (avoid a rewrite)

- **GUI is a shell**: TypeScript owns layout, navigation, focus, and visualization only.
- **Rust is authoritative**: All domain logic stays in `trendlab-core` (data, indicators, backtest, sweep, aggregation, ranking, artifact export).
- **Single contract**: Commands + events have explicit request/response/payload types and a consistent error model.
- **Jobs are first-class**: Long-running work (fetch/sweep) is a cancellable job with `job_id`, progress events, and a final completion event.

---

## Phase 0: Structure + Contract + Plumbing (do this first)

### Project Structure Decision (Tauri v2)
- [ ] Decide if GUI will follow **standard Tauri layout** (recommended):
  - [ ] `apps/trendlab-gui/ui/` (Vite/React)
  - [ ] `apps/trendlab-gui/src-tauri/` (Tauri Rust)
  - [ ] `apps/trendlab-gui/src-tauri/Cargo.toml` depends on `trendlab-core`
- [ ] Or, if we keep it under the workspace crates:
  - [ ] `crates/trendlab-gui/ui/`
  - [ ] `crates/trendlab-gui/src-tauri/`
  - [ ] Document any tooling quirks (paths, build scripts, CI)

### Command + Event Contract (typed + stable)
- [ ] Define a shared **event envelope** used for all events:
  - [ ] `event`: string (e.g., `data:progress`, `sweep:progress`)
  - [ ] `job_id`: string
  - [ ] `ts`: RFC3339 or epoch millis
  - [ ] `payload`: typed payload
- [ ] Define a shared **error envelope** for command failures and job terminal failures:
  - [ ] `code`: stable enum-like string (e.g., `InvalidInput`, `Io`, `ProviderError`)
  - [ ] `message`: human-readable
  - [ ] `details`: optional JSON for debugging
  - [ ] `retryable`: bool
- [ ] Pick a strategy to keep Rust ↔ TS types in sync:
  - [ ] Option A (manual, fastest start): hand-maintained `ui/src/types/*` + Rust mirror structs
  - [ ] Option B (recommended soon): generate TS types from Rust (e.g., `specta` or `ts-rs`) for all command/event payloads

### Jobs + Cancellation Model
- [ ] Define job lifecycle: `queued → running → completed|failed|cancelled`
- [ ] Implement `cancel_job(job_id)` semantics (idempotent; cancelling a finished job is a no-op)
- [ ] Decide how multiple jobs interact (e.g., one sweep at a time, many fetches allowed, etc.)
- [ ] Ensure job completion always emits a terminal event (`*:complete` or `*:failed|*:cancelled`)

### Testing Scope (pragmatic)
- [ ] Prioritize tests at the Rust command boundary:
  - [ ] Unit/integration tests for command handlers + core behavior
- [ ] Keep GUI BDD (tauri-driver) for a small number of high-value end-to-end flows (smoke tests), not exhaustive coverage

---

## Phase 1: Foundation

### Crate Setup
- [ ] Create GUI app root (choose one from Phase 0):
  - [ ] `apps/trendlab-gui/` (recommended), or
  - [ ] `crates/trendlab-gui/`
- [ ] Add it to workspace `Cargo.toml` (if applicable)
- [ ] Create `src-tauri/Cargo.toml` with Tauri v2 dependencies
- [ ] Create `src-tauri/src/main.rs` with Tauri app entry point
- [ ] Create `src-tauri/src/lib.rs` with command module exports
- [ ] Create `src-tauri/tauri.conf.json` with window configuration
- [ ] Create `src-tauri/src/state.rs` with `AppState` struct (RwLock wrappers)
- [ ] Create `src-tauri/src/events.rs` with event payload types
- [ ] Add `src-tauri/src/jobs.rs` with `JobId`, lifecycle state, and cancellation primitives
- [ ] Add `src-tauri/src/error.rs` with GUI-facing error types (error envelope)

### Frontend Scaffolding
- [ ] Initialize React + Vite + TypeScript in `ui/` directory
- [ ] Install dependencies: react, @tauri-apps/api, lightweight-charts, zustand
- [ ] Create `ui/src/main.tsx` entry point
- [ ] Create `ui/src/App.tsx` with panel routing
- [ ] Create `ui/src/styles/theme.css` with Tokyo Night variables
- [ ] Create `ui/src/styles/global.css` with base styles

### TypeScript Types
- [ ] Create `ui/src/types/bar.ts` (Bar, CandleData)
- [ ] Create `ui/src/types/metrics.ts` (Metrics)
- [ ] Create `ui/src/types/strategy.ts` (StrategyTypeId, StrategyConfigId, StrategyParams)
- [ ] Create `ui/src/types/backtest.ts` (BacktestConfig, Trade, Fill, EquityPoint)
- [ ] Create `ui/src/types/sweep.ts` (SweepResult, MultiSweepResult, SweepProgress)
- [ ] Create `ui/src/types/error.ts` (ErrorEnvelope)
- [ ] Create `ui/src/types/events.ts` (EventEnvelope + per-event payload unions)
- [ ] Create `ui/src/types/index.ts` re-exporting all types

### Navigation Shell
- [ ] Create `ui/src/components/Navigation.tsx` (5 tab panels)
- [ ] Create `ui/src/components/StatusBar.tsx`
- [ ] Create `ui/src/components/panels/DataPanel.tsx` (placeholder)
- [ ] Create `ui/src/components/panels/StrategyPanel.tsx` (placeholder)
- [ ] Create `ui/src/components/panels/SweepPanel.tsx` (placeholder)
- [ ] Create `ui/src/components/panels/ResultsPanel.tsx` (placeholder)
- [ ] Create `ui/src/components/panels/ChartPanel.tsx` (placeholder)
- [ ] Implement Tab/number key navigation between panels

### Zustand Store
- [ ] Create `ui/src/store/index.ts` with combined store
- [ ] Create `ui/src/store/slices/navigation.ts` (activePanel)
- [ ] Create `ui/src/store/slices/status.ts` (statusMessage, operationState)
- [ ] Create `ui/src/store/slices/jobs.ts` (job state by `job_id`, last errors)

### Tauri Hooks
- [ ] Create `ui/src/hooks/useTauriCommand.ts` (type-safe invoke wrapper)
- [ ] Create `ui/src/hooks/useTauriEvents.ts` (event listener hook)
- [ ] Ensure hooks standardize: error envelope parsing, job tracking, unsubscribe on unmount

---

## Phase 2: Data Panel

### BDD First
- [ ] Create `tests/features/gui_data_panel.feature`
  - [ ] Scenario: View cached symbols on startup
  - [ ] Scenario: Search for symbols via Yahoo
  - [ ] Scenario: Select/deselect tickers
  - [ ] Scenario: Fetch data for selected tickers
  - [ ] Scenario: View data fetch progress

### Rust Commands
- [ ] Create `src-tauri/src/commands/data.rs`
- [ ] Implement `get_cached_symbols()` command
- [ ] Implement `get_universe()` command
- [ ] Implement `search_symbols(query)` async command
- [ ] Implement `fetch_data(symbols, start, end, force)` async command returning `job_id` + emitting progress events
- [ ] Implement `cancel_fetch(job_id)` command (or reuse `cancel_job(job_id)`)
- [ ] Implement `load_cached_data(symbols)` async command
- [ ] Register commands in `src-tauri/src/lib.rs`

### State Updates
- [ ] Add `BarsCache`, `Universe`, `SelectedTickers` to `AppState`
- [ ] Create `ui/src/store/slices/data.ts`

### React Components
- [ ] Create `ui/src/components/panels/data/SectorList.tsx`
- [ ] Create `ui/src/components/panels/data/TickerList.tsx`
- [ ] Create `ui/src/components/panels/data/SymbolSearch.tsx` (autocomplete)
- [ ] Create `ui/src/components/panels/data/FetchButton.tsx`
- [ ] Create `ui/src/components/panels/data/FetchProgress.tsx`
- [ ] Wire up DataPanel with all subcomponents

---

## Phase 3: Strategy Panel

### BDD First
- [ ] Create `tests/features/gui_strategy_panel.feature`
  - [ ] Scenario: View strategy categories
  - [ ] Scenario: Expand/collapse category
  - [ ] Scenario: Select individual strategy
  - [ ] Scenario: Select all in category
  - [ ] Scenario: Edit strategy parameters
  - [ ] Scenario: Toggle ensemble mode

### Rust Commands
- [ ] Create `src-tauri/src/commands/strategy.rs`
- [ ] Implement `get_strategy_types()` command
- [ ] Implement `get_strategy_categories()` command
- [ ] Implement `get_strategy_grid(depth)` command
- [ ] Implement `update_strategy_selection(selected)` command
- [ ] Register commands in `src-tauri/src/lib.rs`

### State Updates
- [ ] Add `SelectedStrategies`, `StrategyGrid` to `AppState`
- [ ] Create `ui/src/store/slices/strategy.ts`

### React Components
- [ ] Create `ui/src/components/panels/strategy/CategoryAccordion.tsx`
- [ ] Create `ui/src/components/panels/strategy/StrategyCheckbox.tsx`
- [ ] Create `ui/src/components/panels/strategy/ParameterEditor.tsx`
- [ ] Create `ui/src/components/panels/strategy/EnsembleConfig.tsx`
- [ ] Wire up StrategyPanel with all subcomponents

---

## Phase 4: Sweep Panel

### BDD First
- [ ] Create `tests/features/gui_sweep.feature`
  - [ ] Scenario: View selected symbols and strategies summary
  - [ ] Scenario: Select sweep depth
  - [ ] Scenario: Configure cost model
  - [ ] Scenario: Start sweep and see progress
  - [ ] Scenario: Cancel running sweep
  - [ ] Scenario: Sweep completion notification

### Rust Commands
- [ ] Create `src-tauri/src/commands/sweep.rs`
- [ ] Implement `start_sweep(symbols, strategies, depth, config)` async command returning `job_id`
- [ ] Implement `cancel_sweep(job_id)` command (or reuse `cancel_job(job_id)`)
- [ ] Implement `get_job_status(job_id)` command (or `get_sweep_status(job_id)`)
- [ ] Emit `sweep:started`, `sweep:progress`, `sweep:complete|sweep:failed|sweep:cancelled` events
- [ ] Register commands in `src-tauri/src/lib.rs`

### State Updates
- [ ] Add `SweepResult` (or references/keys to persisted results) to `AppState`
- [ ] Track sweep job state via the shared jobs registry (no sweep-specific `CancelFlag`)
- [ ] Create `ui/src/store/slices/sweep.ts`

### React Components
- [ ] Create `ui/src/components/panels/sweep/SelectionSummary.tsx`
- [ ] Create `ui/src/components/panels/sweep/DepthSelector.tsx`
- [ ] Create `ui/src/components/panels/sweep/CostModelEditor.tsx`
- [ ] Create `ui/src/components/panels/sweep/SweepControls.tsx` (Start/Cancel)
- [ ] Create `ui/src/components/panels/sweep/ProgressBar.tsx`
- [ ] Wire up SweepPanel with all subcomponents

---

## Phase 5: Results Panel

### BDD First
- [ ] Create `tests/features/gui_results.feature`
  - [ ] Scenario: View results table after sweep
  - [ ] Scenario: Sort by different metrics
  - [ ] Scenario: Toggle view modes (PerTicker, ByStrategy, AllConfigs)
  - [ ] Scenario: Select result row
  - [ ] Scenario: Navigate to chart for selected result
  - [ ] Scenario: Export artifact for selected result

### Rust Commands
- [ ] Create `src-tauri/src/commands/results.rs`
- [ ] Implement `get_sweep_results()` command
- [ ] Implement `get_results_summary(metric, ascending, limit)` command
- [ ] Implement `get_ticker_summaries()` command
- [ ] Implement `get_strategy_comparison()` command
- [ ] Implement `export_artifact(symbol, strategy, config_id)` command
- [ ] Register commands in `src-tauri/src/lib.rs`

### State Updates
- [ ] Create `ui/src/store/slices/results.ts`

### React Components
- [ ] Create `ui/src/components/panels/results/ResultsTable.tsx`
- [ ] Create `ui/src/components/panels/results/MetricHeader.tsx` (sortable)
- [ ] Create `ui/src/components/panels/results/ViewModeToggle.tsx`
- [ ] Create `ui/src/components/panels/results/ResultDetail.tsx`
- [ ] Create `ui/src/components/panels/results/ExportButton.tsx`
- [ ] Wire up ResultsPanel with all subcomponents

---

## Phase 6: Chart Panel

### BDD First
- [ ] Create `tests/features/gui_chart.feature`
  - [ ] Scenario: View equity curve for selected config
  - [ ] Scenario: View candlestick chart
  - [ ] Scenario: Switch between chart modes
  - [ ] Scenario: Toggle drawdown overlay
  - [ ] Scenario: Toggle volume subplot
  - [ ] Scenario: Use crosshair with tooltips
  - [ ] Scenario: Zoom and pan chart
  - [ ] Scenario: View trade markers

### Rust Commands
- [ ] Create `src-tauri/src/commands/chart.rs`
- [ ] Implement `get_equity_curve(symbol, strategy, config_id)` command
- [ ] Implement `get_candle_data(symbol)` command
- [ ] Implement `get_multi_ticker_curves()` command
- [ ] Implement `get_portfolio_curve()` command
- [ ] Implement `get_strategy_curves()` command
- [ ] Implement `get_trades(symbol, strategy, config_id)` command
- [ ] Register commands in `src-tauri/src/lib.rs`

### State Updates
- [ ] Create `ui/src/store/slices/chart.ts`

### React Components (TradingView Lightweight Charts)
- [ ] Create `ui/src/components/charts/useChart.ts` (chart instance hook)
- [ ] Create `ui/src/components/charts/CandlestickChart.tsx`
- [ ] Create `ui/src/components/charts/EquityChart.tsx`
- [ ] Create `ui/src/components/charts/MultiTickerChart.tsx`
- [ ] Create `ui/src/components/charts/PortfolioChart.tsx`
- [ ] Create `ui/src/components/charts/StrategyComparisonChart.tsx`
- [ ] Create `ui/src/components/charts/ChartControls.tsx`
- [ ] Create `ui/src/components/charts/ChartLegend.tsx`
- [ ] Create `ui/src/components/charts/TradeMarkers.tsx`
- [ ] Create `ui/src/components/panels/chart/TradesTable.tsx`
- [ ] Wire up ChartPanel with mode switching

---

## Phase 7: Polish & Integration

### Keyboard Navigation (TUI Parity)

Match the TUI keyboard shortcuts exactly for muscle-memory consistency.

#### Global Navigation

| Key         | Action                                              |
| ----------- | --------------------------------------------------- |
| `q`         | Quit application                                    |
| `Esc`       | Cancel current operation / close modal / exit search |
| `Tab`       | Next panel (or toggle focus within Strategy panel)  |
| `Shift+Tab` | Previous panel                                      |
| `1-5`       | Direct panel access (Data, Strategy, Sweep, Results, Chart) |

#### Vim-Style List Navigation

| Key          | Action                     |
| ------------ | -------------------------- |
| `j` / `Down` | Move down in list          |
| `k` / `Up`   | Move up in list            |
| `h` / `Left` | Collapse / navigate left   |
| `l` / `Right`| Expand / navigate right    |
| `Enter`      | Confirm / expand / collapse |

#### Selection

| Key     | Action                       |
| ------- | ---------------------------- |
| `Space` | Toggle item selection        |
| `a`     | Select all in current context |
| `n`     | Deselect all (select none)   |

#### Panel-Specific Actions

| Key | Panel    | Action                                        |
| --- | -------- | --------------------------------------------- |
| `f` | Data     | Fetch data for selected tickers               |
| `s` | Data     | Enter search mode                             |
| `s` | Results  | Cycle sort column                             |
| `v` | Results  | Cycle view mode (PerTicker/ByStrategy/AllConfigs) |
| `e` | Strategy | Toggle ensemble mode                          |
| `d` | Chart    | Toggle drawdown overlay                       |
| `m` | Chart    | Cycle chart mode (single/multi-ticker/portfolio) |
| `v` | Chart    | Toggle volume subplot                         |
| `c` | Chart    | Toggle crosshair                              |

#### Misc

| Key | Action                                 |
| --- | -------------------------------------- |
| `R` | Reset to canonical defaults            |
| `?` | Show keyboard shortcuts help (optional) |

Implementation notes:

- [ ] Create `ui/src/hooks/useKeyboardNavigation.ts` with centralized key handler
- [ ] Context-aware: different actions per panel
- [ ] Ignore keypresses when focus is in input/textarea elements
- [ ] Show keyboard hints in StatusBar and panel headers

### Startup Modal
- [ ] Create `ui/src/components/StartupModal.tsx`
- [ ] Manual mode selection
- [ ] Full-Auto mode with strategy/depth selection
- [ ] Remember last mode preference

### Accessibility
- [ ] ARIA labels on interactive elements
- [ ] Focus management for modals
- [ ] Screen reader announcements for status updates
- [ ] Keyboard-only navigation verification

### Performance
- [ ] Virtualize long lists (results table)
- [ ] Debounce search input
- [ ] Lazy load chart data
- [ ] Memoize expensive computations (UI-only; domain computations remain in Rust)

### Cross-Platform Testing
- [ ] Windows build and test
- [ ] macOS build and test (if available)
- [ ] Linux build and test (if available)
- [ ] Package for distribution

### Documentation
- [ ] Update CLAUDE.md with trendlab-gui in structure
- [ ] Add GUI-specific slash commands if needed
- [ ] Document keyboard shortcuts
- [ ] Add screenshots to README

---

## Phase 8: Unified Launcher + Companion Mode

A single `trendlab` binary that offers TUI or GUI, with the terminal staying alive as a "companion" when GUI is launched.

### User Experience

```
$ trendlab

  ╭─────────────────────────────────────╮
  │         T R E N D L A B             │
  │   Trend-Following Backtest Lab      │
  │                                     │
  │   Press [T] for Terminal UI         │
  │   Press [G] for Desktop GUI         │
  │                                     │
  │   (or --tui / --gui to skip)        │
  ╰─────────────────────────────────────╯
```

**TUI selected**: Terminal transforms into full TUI (process replaced)

**GUI selected**: GUI window opens, terminal becomes companion:

```
╭─ TrendLab Companion ──────────────────────────────────────╮
│ GUI: Running (PID 12345)                    [q] quit      │
├───────────────────────────────────────────────────────────┤
│ Active Job: sweep-1735312456                              │
│ Progress: [████████████░░░░░░░░] 62%  (186/300 configs)   │
│ Current:  MSFT × donchian_breakout_30                     │
│ Elapsed:  2m 34s                                          │
├───────────────────────────────────────────────────────────┤
│ Recent Results:                                           │
│  AAPL  donchian_20       Sharpe 1.24  CAGR 18.2%  DD -12% │
│  MSFT  ma_cross_50_200   Sharpe 0.87  CAGR 14.1%  DD -18% │
│  GOOGL momentum_12       Sharpe 1.02  CAGR 16.8%  DD -15% │
│  TSLA  atr_breakout_14   Sharpe 0.65  CAGR 22.4%  DD -31% │
├───────────────────────────────────────────────────────────┤
│ Log:                                                      │
│  [12:34:56] Sweep started: 5 symbols × 12 strategies      │
│  [12:34:57] Fetching AAPL data... done (2,516 bars)       │
│  [12:35:02] AAPL: 24 configs complete, best Sharpe 1.24   │
╰───────────────────────────────────────────────────────────╯
```

### Binary Layout

| Command | Description |
| ------- | ----------- |
| `trendlab` | Unified launcher (prompts for TUI/GUI) |
| `trendlab --tui` | Skip prompt, launch TUI directly |
| `trendlab --gui` | Skip prompt, launch GUI directly |
| `trendlab-tui` | Direct TUI binary (still available) |
| `trendlab-gui` | Direct GUI binary (still available) |

### Crate Structure

```
crates/
├── trendlab-launcher/       # NEW: Unified launcher
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs          # Entry point, mode selection
│       ├── prompt.rs        # Interactive T/G prompt
│       ├── companion.rs     # Companion view when GUI running
│       ├── ipc.rs           # IPC client for receiving events
│       └── display.rs       # Ratatui rendering for companion
```

### IPC Architecture (GUI → Companion)

#### Option A: Named Pipe / Unix Socket (Recommended)

```
GUI (Rust backend)                    Companion (Terminal)
      │                                      │
      │  ┌─────────────────────────┐         │
      ├──│ /tmp/trendlab-{pid}.sock│─────────┤
      │  └─────────────────────────┘         │
      │                                      │
  emit_to_companion(event)          recv() → update display
```

- Fast, local-only, no network exposure
- Cross-platform: Unix sockets on Linux/macOS, named pipes on Windows
- Clean shutdown: socket removed when companion exits

#### Option B: Localhost TCP

```rust
// Companion listens on ephemeral port, passes to GUI via env
let listener = TcpListener::bind("127.0.0.1:0")?;
let port = listener.local_addr()?.port();
env::set_var("TRENDLAB_COMPANION_PORT", port.to_string());
Command::new("trendlab-gui").spawn()?;
```

- Simpler cross-platform implementation
- Slightly more overhead than sockets
- Firewall might prompt on first run (Windows)

#### Option C: Shared Memory / mmap (Future)

- Highest performance for high-frequency updates
- More complex implementation
- Overkill for current needs

### IPC Message Types

```rust
/// Events sent from GUI to Companion
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CompanionEvent {
    /// GUI window opened
    Started { pid: u32, version: String },

    /// GUI window closed
    Shutdown,

    /// Job lifecycle
    JobStarted { job_id: String, job_type: String, description: String },
    JobProgress { job_id: String, current: u64, total: u64, message: String },
    JobComplete { job_id: String, summary: String },
    JobFailed { job_id: String, error: String },
    JobCancelled { job_id: String },

    /// Sweep results (streamed as they complete)
    SweepResult {
        ticker: String,
        strategy: String,
        config_id: String,
        sharpe: f64,
        cagr: f64,
        max_dd: f64,
    },

    /// Log line
    Log { level: String, message: String },

    /// Status update
    Status { message: String },
}
```

### Implementation Tasks

#### Launcher Core

- [ ] Create `crates/trendlab-launcher/` crate
- [ ] Add to workspace `Cargo.toml`
- [ ] Implement mode selection prompt (crossterm for raw input)
- [ ] Handle `--tui` and `--gui` CLI flags (clap)
- [ ] TUI mode: `exec()` on Unix, `spawn().wait()` on Windows
- [ ] GUI mode: spawn process + enter companion mode

#### Companion View

- [ ] Create `companion.rs` with ratatui-based UI
- [ ] Layout: status bar, progress, recent results, log
- [ ] Handle terminal resize
- [ ] Keyboard: `q` to quit (sends shutdown signal to GUI)
- [ ] Keyboard: `Esc` to minimize to single status line
- [ ] Auto-exit when GUI process terminates

#### IPC Implementation

- [ ] Define `CompanionEvent` enum in shared types
- [ ] Implement socket/pipe server in companion
- [ ] Implement socket/pipe client in GUI's Rust backend
- [ ] Helper: `emit_to_companion(event)` function
- [ ] Graceful handling of companion not running (no-op)
- [ ] Reconnection logic if companion restarts

#### GUI Integration

- [ ] On startup: check for `TRENDLAB_COMPANION_SOCKET` env var
- [ ] If set, connect and send `Started` event
- [ ] Hook into existing event emission to also send to companion
- [ ] On shutdown: send `Shutdown` event, close connection

#### Testing

- [ ] Unit tests for IPC serialization
- [ ] Integration test: launcher → TUI mode
- [ ] Integration test: launcher → GUI + companion
- [ ] Test companion display with mock events
- [ ] Test graceful shutdown (GUI closed, companion exits)
- [ ] Test companion closed while GUI running (GUI continues)

### Dependencies

```toml
# crates/trendlab-launcher/Cargo.toml
[dependencies]
clap = { version = "4", features = ["derive"] }
crossterm = "0.27"
ratatui = "0.28"
tokio = { version = "1", features = ["full", "net"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
interprocess = "2"  # Cross-platform IPC (named pipes, Unix sockets)

# Shared types (if extracted)
trendlab-ipc = { path = "../trendlab-ipc" }  # Optional: shared event types
```

### Future Enhancements

- [ ] Companion can send commands back to GUI (cancel job, etc.)
- [ ] Multiple GUI instances, single companion aggregates
- [ ] Companion persists log to file
- [ ] Companion shows notifications (toast) on job complete
- [ ] Web-based companion option (WebSocket server)

---

## Future: YOLO Mode (Auto-Optimization)

YOLO Mode is a continuous auto-optimization loop that runs multi-strategy sweeps indefinitely, applying parameter randomization each iteration and maintaining a top-4 leaderboard ranked by Sharpe ratio. This feature is already implemented in the TUI and should be ported to the GUI.

### TUI Implementation Reference

The TUI implementation lives in:
- `crates/trendlab-core/src/leaderboard.rs` - Leaderboard and LeaderboardEntry types
- `crates/trendlab-tui/src/worker.rs` - handle_yolo_mode handler, jitter functions
- `crates/trendlab-tui/src/app.rs` - YoloState, start_yolo_mode method

### GUI Implementation Tasks

#### Rust Commands
- [ ] Create `src-tauri/src/commands/yolo.rs`
- [ ] Implement `start_yolo_mode(symbols, strategies, config, randomization_pct)` returning `job_id`
- [ ] Implement `stop_yolo_mode(job_id)` command
- [ ] Implement `get_leaderboard()` command
- [ ] Emit `yolo:started`, `yolo:iteration_complete`, `yolo:progress`, `yolo:stopped` events
- [ ] Register commands in `src-tauri/src/lib.rs`

#### State Updates
- [ ] Add `Leaderboard`, `YoloState` to `AppState`
- [ ] Create `ui/src/store/slices/yolo.ts`

#### React Components
- [ ] Create `ui/src/components/panels/sweep/YoloControls.tsx`
  - [ ] Start/Stop YOLO mode button
  - [ ] Randomization percentage slider (±5% to ±25%)
  - [ ] Current iteration counter
  - [ ] Total configs tested display
- [ ] Create `ui/src/components/panels/sweep/Leaderboard.tsx`
  - [ ] Top 4 strategies table
  - [ ] Strategy type, config display, Sharpe, CAGR, Max DD columns
  - [ ] Click to view equity curve
  - [ ] Visual rank indicators (🥇🥈🥉)
- [ ] Create `ui/src/components/charts/LeaderboardChart.tsx`
  - [ ] Overlay top 4 equity curves with different colors
  - [ ] Legend with strategy names and metrics
  - [ ] Animated updates as new leaders discovered
- [ ] Wire into SweepPanel and ChartPanel

#### Keyboard Shortcuts
- [ ] `Y` key to toggle YOLO mode (when in Sweep panel)
- [ ] Update keyboard navigation hook

#### Persistence
- [ ] Load existing `artifacts/leaderboard.json` on startup
- [ ] Save after each iteration (handled by Rust backend)
- [ ] Show load/save status in UI

---

## BDD Test Infrastructure

### Setup
- [ ] Add tauri-driver dev dependency
- [ ] Create `tests/gui_bdd.rs` cucumber runner
- [ ] Create `tests/steps/mod.rs` step definitions module
- [ ] Create `tests/steps/common.rs` (app launch, navigation)
- [ ] Configure CI for headless GUI tests
- [ ] Keep BDD as smoke tests; assert correctness via Rust tests on commands/core

### Step Definition Modules
- [ ] `tests/steps/data_steps.rs`
- [ ] `tests/steps/strategy_steps.rs`
- [ ] `tests/steps/sweep_steps.rs`
- [ ] `tests/steps/results_steps.rs`
- [ ] `tests/steps/chart_steps.rs`

---

## Dependencies

### Rust (Cargo.toml)
```toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
trendlab-core = { path = "../../crates/trendlab-core" } # if `apps/trendlab-gui/src-tauri/`
# trendlab-core = { path = "../trendlab-core" } # if `crates/trendlab-gui/src-tauri/`

[dev-dependencies]
tauri-driver = "2"
cucumber = "0.21"
```

### Frontend (package.json)
```json
{
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "@tauri-apps/api": "^2.0.0",
    "lightweight-charts": "^4.1.0",
    "zustand": "^4.4.0",
    "react-icons": "^5.0.0"
  },
  "devDependencies": {
    "vite": "^5.0.0",
    "@vitejs/plugin-react": "^4.2.0",
    "typescript": "^5.3.0",
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0"
  }
}
```

---

## Reference Files

| TUI File | Purpose | GUI Equivalent |
|----------|---------|----------------|
| `trendlab-tui/src/app.rs` | State structure | `state.rs` + Zustand slices |
| `trendlab-tui/src/worker.rs` | Async operations | Tauri commands + events |
| `trendlab-tui/src/panels/*.rs` | Panel rendering | React panel components |
| `trendlab-tui/src/ui.rs` | Layout + colors | theme.css + App.tsx |
