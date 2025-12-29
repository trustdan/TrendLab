# TrendLab Repository Audit Plan

## Overview

This document outlines a systematic, phase-by-phase audit plan to identify and catalog:
- Loose ends and unimplemented features
- Unfinished business and TODOs
- Bugs and inconsistencies
- Mismatches between documentation and implementation
- Issues specific to GUI and TUI modes
- Error handling gaps
- Code quality issues

## Audit Methodology

The audit follows the application flow, starting from entry points and moving through each layer:
1. **Entry Points** → Main application initialization
2. **Core Domain** → Business logic, invariants, correctness
3. **UI Layer (TUI)** → Terminal interface implementation
4. **UI Layer (GUI)** → Desktop interface implementation
5. **Integration Points** → Data flow, IPC, state management
6. **Documentation** → Consistency with implementation

Each phase produces a checklist of issues found, categorized by severity and type.

---

## Phase 1: Entry Points & Application Initialization

**Goal**: Verify all entry points work correctly and handle errors gracefully.

### 1.1 Main Entry Points Audit

**Files to Review:**
- `crates/trendlab-cli/src/main.rs` - CLI entry point
- `crates/trendlab-tui/src/main.rs` - TUI entry point
- `apps/trendlab-gui/src-tauri/src/main.rs` - GUI entry point
- `crates/trendlab-launcher/src/main.rs` - Unified launcher

**Checklist:**
- [ ] All entry points handle command-line arguments correctly
- [ ] Environment variable parsing is consistent across modes
- [ ] Error messages are user-friendly and actionable
- [ ] Initialization errors are caught and reported properly
- [ ] Resource cleanup on exit (file handles, network connections)
- [ ] Logging initialization is consistent
- [ ] Configuration file loading handles missing/invalid files gracefully

**Known Issues to Verify:**
- [ ] Launcher mode selection works correctly
- [ ] Companion mode IPC setup (if implemented)
- [ ] TUI startup modal (Manual vs Full-Auto) handles all edge cases

### 1.2 State Initialization Audit

**Files to Review:**
- `crates/trendlab-tui/src/app.rs` - TUI state
- `apps/trendlab-gui/src-tauri/src/state.rs` - GUI state
- `apps/trendlab-gui/src-tauri/src/jobs.rs` - Job management

**Checklist:**
- [ ] State initialization is deterministic
- [ ] Default values are sensible and documented
- [ ] State persistence (if any) loads correctly on startup
- [ ] Race conditions in state access are handled (RwLock usage)
- [ ] State cleanup on application exit

**Known Issues:**
- [ ] Random defaults feature (`RandomDefaults`) - verify seed handling
- [ ] Launch count persistence (`tui_launch_count.txt`) - verify file I/O errors handled

---

## Phase 2: Core Domain Logic

**Goal**: Verify correctness, invariants, and completeness of core business logic.

### 2.1 Data Layer Audit

**Files to Review:**
- `crates/trendlab-core/src/data/` - All data-related modules
- `crates/trendlab-cli/src/data.rs` - Data CLI commands

**Checklist:**
- [ ] Data loading handles missing files gracefully
- [ ] Parquet schema validation is comprehensive
- [ ] Missing bar handling is consistent (gaps vs forward-fill)
- [ ] Data quality checks catch all edge cases
- [ ] Yahoo Finance provider handles rate limits and errors
- [ ] Cache invalidation logic is correct
- [ ] Data normalization (adjustments) is applied consistently

**Known Issues to Verify:**
- [ ] Adjustment policy (raw vs adjusted_close) - verify documentation matches implementation
- [ ] Missing bar policy - verify gaps don't cause lookahead bugs
- [ ] Data refresh with `--force` flag works correctly

### 2.2 Strategy & Indicator Audit

**Files to Review:**
- `crates/trendlab-core/src/strategies/` - All strategy implementations
- `crates/trendlab-core/src/indicators.rs` - Indicator functions
- `crates/trendlab-core/src/indicators_polars.rs` - Polars indicators
- `crates/trendlab-core/src/indicator_cache.rs` - Caching layer

**Checklist:**
- [ ] All 20 V2 strategies are fully implemented
- [ ] No lookahead bugs (signals at T use only bars ≤ T)
- [ ] Indicator calculations match documented formulas
- [ ] Edge cases handled (empty data, insufficient bars, NaN values)
- [ ] Indicator caching works correctly and doesn't introduce bugs
- [ ] Strategy parameters are validated (e.g., fast MA < slow MA)
- [ ] Trading modes (LongOnly, ShortOnly, LongShort) work for all strategies

**Known Issues:**
- [ ] TODO in `indicator_cache.rs` line 575: "Implement caching for these if profiling shows benefit" - verify if needed
- [ ] Verify all strategies have BDD test coverage

### 2.3 Backtest Engine Audit

**Files to Review:**
- `crates/trendlab-core/src/backtest_polars.rs` - Polars backend
- `crates/trendlab-core/src/backtest.rs` - Sequential backend (if exists)
- `crates/trendlab-core/src/fills.rs` - Fill model
- `crates/trendlab-core/src/equity.rs` - Accounting

**Checklist:**
- [ ] Fill model is applied consistently (next-bar open)
- [ ] Accounting identity holds: Cash + PositionValue = Equity at all times
- [ ] Transaction costs are calculated correctly
- [ ] Position tracking handles all edge cases (partial fills, pyramiding)
- [ ] Short position P&L calculation is correct
- [ ] Polars and sequential backends produce identical results (when applicable)
- [ ] Determinism: same inputs → same outputs

**Known Issues:**
- [ ] Verify fill model documentation matches implementation
- [ ] Check for any unwrap() calls that could panic in production

### 2.4 Metrics & Statistics Audit

**Files to Review:**
- `crates/trendlab-core/src/metrics.rs` - Performance metrics
- `crates/trendlab-core/src/statistics.rs` - Statistical analysis
- `crates/trendlab-core/src/analysis.rs` - Trade analysis
- `crates/trendlab-core/src/analysis_polars.rs` - Polars analysis

**Checklist:**
- [ ] All metrics calculations are correct (Sharpe, CAGR, drawdown, etc.)
- [ ] Statistical functions handle edge cases (empty data, single value)
- [ ] Bootstrap confidence intervals are computed correctly
- [ ] Walk-forward validation logic is sound
- [ ] FDR correction (Benjamini-Hochberg, Holm-Bonferroni) is correct
- [ ] Regime analysis classification is consistent

**Known Issues:**
- [ ] Many unwrap() calls in statistics.rs tests - verify these are test-only
- [ ] Verify statistical functions don't panic on edge cases

### 2.5 Sweep & Leaderboard Audit

**Files to Review:**
- `crates/trendlab-core/src/sweep.rs` - Sweep configuration
- `crates/trendlab-core/src/sweep_polars.rs` - Polars sweep execution
- `crates/trendlab-core/src/leaderboard.rs` - Leaderboard management

**Checklist:**
- [ ] Parameter grid generation is correct
- [ ] Sweep execution handles cancellation gracefully
- [ ] Parallel execution (Rayon) doesn't introduce race conditions
- [ ] Leaderboard ranking is consistent across risk profiles
- [ ] Cross-symbol aggregation is correct
- [ ] YOLO mode randomization doesn't break determinism (when seed is set)
- [ ] Session persistence works correctly

**Known Issues:**
- [ ] TODO in `worker.rs` line 2107: "Pass session_id from YoloState in Phase 1"
- [ ] TODO in `worker.rs` line 2163: "Look up sector from universe in Phase 2"
- [ ] Verify YOLO mode leaderboard persistence

---

## Phase 3: TUI Implementation

**Goal**: Verify TUI works correctly, handles errors, and matches documented behavior.

### 3.1 TUI State Management Audit

**Files to Review:**
- `crates/trendlab-tui/src/app.rs` - Application state (3842 lines - large file!)
- `crates/trendlab-tui/src/worker.rs` - Async worker thread

**Checklist:**
- [ ] State transitions are valid and don't leave app in inconsistent state
- [ ] Panel navigation works correctly (1-6 keys, Tab/Shift+Tab)
- [ ] Keyboard shortcuts match documentation exactly
- [ ] Worker thread handles cancellation correctly
- [ ] Error messages are displayed to user (not just logged)
- [ ] Long-running operations show progress
- [ ] State persistence (if any) works correctly

**Known Issues:**
- [ ] Large app.rs file (3842 lines) - consider refactoring
- [ ] Verify all keyboard shortcuts work as documented
- [ ] Check for dead code (marked with `#[allow(dead_code)]`)

### 3.2 TUI Panel Implementation Audit

**Files to Review:**
- `crates/trendlab-tui/src/panels/` - All panel modules
  - `data.rs`
  - `strategy.rs`
  - `sweep.rs`
  - `results.rs`
  - `chart.rs`
  - `help.rs` (if exists)

**Checklist:**
- [ ] Data panel: ticker selection, sector navigation, data fetching
- [ ] Strategy panel: parameter editing, category expansion, ensemble mode
- [ ] Sweep panel: progress display, cancellation, depth selection
- [ ] Results panel: sorting, filtering, view modes, risk profile cycling
- [ ] Chart panel: all view modes (Single, Candlestick, Multi-Ticker, etc.)
- [ ] Help panel: navigation, search, context-sensitive opening
- [ ] All panels handle empty states gracefully
- [ ] All panels handle errors gracefully

**Known Issues:**
- [ ] Verify chart panel volume subplot toggle works
- [ ] Verify chart panel drawdown overlay works
- [ ] Verify results panel statistical analysis view works

### 3.3 TUI Worker Thread Audit

**Files to Review:**
- `crates/trendlab-tui/src/worker.rs` - Worker implementation

**Checklist:**
- [ ] All async operations are cancellable
- [ ] Progress updates are sent correctly
- [ ] Error handling doesn't leave worker in bad state
- [ ] YOLO mode implementation is complete
- [ ] Data fetching handles network errors
- [ ] Sweep execution handles all error cases

**Known Issues:**
- [ ] TODO comments about session_id and sector lookup
- [ ] Verify YOLO mode leaderboard updates are sent correctly
- [ ] Check for unwrap() calls that could panic

---

## Phase 4: GUI Implementation

**Goal**: Verify GUI works correctly, matches TUI functionality, and handles errors.

### 4.1 GUI Backend (Rust) Audit

**Files to Review:**
- `apps/trendlab-gui/src-tauri/src/commands/` - All command modules
  - `data.rs`
  - `strategy.rs`
  - `sweep.rs`
  - `results.rs`
  - `chart.rs`
  - `yolo.rs`
  - `jobs.rs`
  - `system.rs`
- `apps/trendlab-gui/src-tauri/src/state.rs` - State management
- `apps/trendlab-gui/src-tauri/src/events.rs` - Event types
- `apps/trendlab-gui/src-tauri/src/error.rs` - Error handling

**Checklist:**
- [ ] All commands return proper error envelopes
- [ ] State access uses RwLock correctly (no deadlocks)
- [ ] Event emission is consistent and complete
- [ ] Job cancellation works correctly
- [ ] Long-running operations emit progress events
- [ ] Error handling converts core errors to GUI errors correctly
- [ ] Commands match TUI functionality (feature parity)

**Known Issues:**
- [ ] Many unwrap() calls on RwLock - verify these are safe or handle errors
- [ ] Verify job lifecycle management is complete
- [ ] Check for missing error handling in command handlers

### 4.2 GUI Frontend (TypeScript/React) Audit

**Files to Review:**
- `apps/trendlab-gui/ui/src/` - All frontend code
  - `App.tsx` - Main app component
  - `components/panels/` - All panel components
  - `store/slices/` - Zustand state slices
  - `hooks/` - Custom hooks
  - `types/` - TypeScript type definitions

**Checklist:**
- [ ] TypeScript types match Rust types (no mismatches)
- [ ] Error handling in UI displays user-friendly messages
- [ ] Loading states are shown for async operations
- [ ] Keyboard navigation matches TUI shortcuts
- [ ] Panel navigation works correctly (1-5 keys, Tab)
- [ ] Chart components render correctly (TradingView Lightweight Charts)
- [ ] State management doesn't have race conditions
- [ ] Event listeners are cleaned up on unmount

**Known Issues:**
- [ ] Verify all panels are implemented (check roadmap completion)
- [ ] Check for TypeScript type mismatches with Rust
- [ ] Verify keyboard shortcuts work in all contexts
- [ ] Check for memory leaks (event listeners, chart instances)

### 4.3 GUI-TUI Feature Parity Audit

**Goal**: Ensure GUI has feature parity with TUI.

**Checklist:**
- [ ] All TUI panels have GUI equivalents
- [ ] All TUI keyboard shortcuts work in GUI
- [ ] All TUI features are accessible in GUI
- [ ] YOLO mode works in GUI
- [ ] Help panel exists in GUI (if TUI has it)
- [ ] Startup modal works correctly
- [ ] Full-Auto mode works in GUI

**Known Issues:**
- [ ] Roadmap shows Phase 7 (Polish) and Phase 8 (Launcher) as complete - verify
- [ ] YOLO mode implementation status in GUI

---

## Phase 5: Integration & Data Flow

**Goal**: Verify data flows correctly between layers and components.

### 5.1 IPC & Communication Audit

**Files to Review:**
- `apps/trendlab-gui/src-tauri/src/commands/` - Command handlers
- `apps/trendlab-gui/src-tauri/src/events.rs` - Event definitions
- `apps/trendlab-gui/ui/src/hooks/useTauriCommand.ts` - Command hook
- `apps/trendlab-gui/ui/src/hooks/useTauriEvents.ts` - Event hook

**Checklist:**
- [ ] Command request/response types match between Rust and TypeScript
- [ ] Event payloads are correctly serialized/deserialized
- [ ] Error envelopes are parsed correctly in frontend
- [ ] Progress events are received and displayed
- [ ] Job cancellation signals are handled correctly
- [ ] No memory leaks from event listeners

**Known Issues:**
- [ ] Verify type synchronization strategy (manual vs generated)

### 5.2 State Synchronization Audit

**Checklist:**
- [ ] GUI state stays in sync with backend state
- [ ] TUI state updates correctly from worker thread
- [ ] No race conditions in state updates
- [ ] State persistence (if any) works correctly
- [ ] Undo/redo (if implemented) works correctly

### 5.3 Data Flow Audit

**Checklist:**
- [ ] Data flows correctly: Data Panel → Strategy Panel → Sweep Panel → Results Panel → Chart Panel
- [ ] Selected tickers persist across panel navigation
- [ ] Selected strategies persist across panel navigation
- [ ] Sweep results are available in Results panel
- [ ] Selected result is displayed in Chart panel
- [ ] Chart data loads correctly for selected result

---

## Phase 6: Error Handling & Robustness

**Goal**: Identify error handling gaps and potential panic points.

### 6.1 Unwrap/Expect/Panic Audit

**Files to Review:**
- All Rust files (use grep results from audit)

**Checklist:**
- [ ] Categorize unwrap() calls:
  - [ ] Test-only (acceptable)
  - [ ] Safe in context (document why)
  - [ ] Should be replaced with proper error handling
- [ ] expect() calls have meaningful messages
- [ ] No panic!() calls in production code
- [ ] unreachable!() calls are truly unreachable

**Known Issues:**
- [ ] 1061 matches for unwrap/expect/panic found - need to categorize
- [ ] Many in test files (acceptable)
- [ ] Many in GUI state access (RwLock unwrap) - verify safety

### 6.2 Error Propagation Audit

**Checklist:**
- [ ] Core errors propagate correctly through layers
- [ ] GUI errors are converted from core errors correctly
- [ ] TUI errors are displayed to user (not just logged)
- [ ] Network errors are handled gracefully
- [ ] File I/O errors are handled gracefully
- [ ] Invalid user input is caught and reported

### 6.3 Edge Case Handling Audit

**Checklist:**
- [ ] Empty data sets handled gracefully
- [ ] Invalid date ranges handled gracefully
- [ ] Invalid strategy parameters caught and reported
- [ ] Missing configuration files handled gracefully
- [ ] Concurrent operations handled correctly
- [ ] Resource exhaustion handled gracefully (memory, file handles)

---

## Phase 7: Documentation Consistency

**Goal**: Verify documentation matches implementation.

### 7.1 README Audit

**Files to Review:**
- `README.md` - Main documentation

**Checklist:**
- [ ] All documented features are implemented
- [ ] All documented keyboard shortcuts work
- [ ] All documented commands work
- [ ] Examples in README are correct and runnable
- [ ] Version numbers match actual versions
- [ ] Dependencies are listed correctly

### 7.2 Architecture Documentation Audit

**Files to Review:**
- `docs/architecture.md`
- `docs/PROJECT_OVERVIEW.md`
- `docs/assumptions.md`
- `docs/schema.md`

**Checklist:**
- [ ] Architecture diagrams match actual structure
- [ ] Data flow descriptions match implementation
- [ ] Invariants are documented and tested
- [ ] Assumptions are documented and validated
- [ ] Schema documentation matches actual schemas

### 7.3 Roadmap Audit

**Files to Review:**
- `docs/roadmap-tauri-gui.md`
- `docs/roadmap-v2-strategies.md`
- Any other roadmap files

**Checklist:**
- [ ] Completed items are actually complete
- [ ] Incomplete items are clearly marked
- [ ] Future items are clearly marked as future
- [ ] No orphaned roadmap items

---

## Phase 8: Code Quality & Technical Debt

**Goal**: Identify code quality issues and technical debt.

### 8.1 Code Organization Audit

**Checklist:**
- [ ] Large files are split appropriately (app.rs is 3842 lines!)
- [ ] Module boundaries are clear
- [ ] Dependencies are minimal and correct
- [ ] No circular dependencies
- [ ] Dead code is removed (or marked with allow(dead_code) with reason)

### 8.2 Test Coverage Audit

**Files to Review:**
- `crates/trendlab-bdd/tests/` - BDD tests
- `crates/trendlab-core/src/` - Unit tests (if any)

**Checklist:**
- [ ] All strategies have BDD test coverage
- [ ] All indicators have test coverage
- [ ] Critical invariants are tested (no lookahead, accounting identity)
- [ ] Edge cases are tested
- [ ] Error cases are tested

### 8.3 Performance Audit

**Checklist:**
- [ ] No obvious performance bottlenecks
- [ ] Memory usage is reasonable
- [ ] Lazy evaluation is used where appropriate (Polars)
- [ ] Parallel execution is used where appropriate (Rayon)
- [ ] No unnecessary data copying

---

## Phase 9: GUI-Specific Issues

**Goal**: Deep dive into GUI problems mentioned by user.

### 9.1 GUI Functionality Audit

**Checklist:**
- [ ] All panels load and render correctly
- [ ] Data fetching works in GUI
- [ ] Strategy selection works in GUI
- [ ] Sweep execution works in GUI
- [ ] Results display works in GUI
- [ ] Chart rendering works in GUI
- [ ] YOLO mode works in GUI (if implemented)
- [ ] Keyboard navigation works in GUI
- [ ] Error messages are displayed in GUI

### 9.2 GUI-TUI Consistency Audit

**Checklist:**
- [ ] Same operations produce same results in GUI and TUI
- [ ] GUI and TUI use same backend logic
- [ ] GUI and TUI have same feature set
- [ ] GUI and TUI have consistent behavior

### 9.3 GUI Build & Distribution Audit

**Checklist:**
- [ ] GUI builds successfully on all platforms
- [ ] GUI runs correctly on all platforms
- [ ] GUI packaging works correctly
- [ ] GUI dependencies are correct

---

## Phase 10: TUI-Specific Issues

**Goal**: Deep dive into TUI problems mentioned by user.

### 10.1 TUI Functionality Audit

**Checklist:**
- [ ] All panels work correctly
- [ ] Keyboard shortcuts work correctly
- [ ] Worker thread operations complete successfully
- [ ] Error messages are displayed correctly
- [ ] Progress indicators work correctly
- [ ] Chart rendering works correctly
- [ ] YOLO mode works correctly

### 10.2 TUI Robustness Audit

**Checklist:**
- [ ] TUI handles terminal resize correctly
- [ ] TUI handles terminal color support correctly
- [ ] TUI handles Unicode correctly
- [ ] TUI handles mouse input correctly (if supported)
- [ ] TUI handles Ctrl+C gracefully
- [ ] TUI cleanup on exit is correct

---

## Audit Execution Plan

### Step 1: Preparation
1. Create issue tracking document (spreadsheet or markdown)
2. Set up test environment
3. Review this audit plan and adjust as needed

### Step 2: Systematic Review
1. Go through each phase in order
2. For each checklist item:
   - Review relevant code
   - Test functionality (if applicable)
   - Document findings
   - Categorize by severity (Critical, High, Medium, Low)
   - Categorize by type (Bug, Missing Feature, Inconsistency, Documentation, Technical Debt)

### Step 3: Prioritization
1. Group findings by severity and type
2. Identify quick wins (easy fixes)
3. Identify blockers (critical bugs)
4. Identify technical debt items

### Step 4: Reporting
1. Create summary report with:
   - Executive summary
   - Findings by category
   - Prioritized action items
   - Recommendations

---

## Issue Categories

### Severity Levels
- **Critical**: Blocks core functionality, causes data loss, or crashes
- **High**: Major feature broken or inconsistent behavior
- **Medium**: Minor feature broken or inconsistency
- **Low**: Cosmetic issue or minor improvement

### Issue Types
- **Bug**: Something that doesn't work as intended
- **Missing Feature**: Documented but not implemented
- **Inconsistency**: Different behavior in different places
- **Documentation**: Docs don't match implementation
- **Technical Debt**: Code quality issues, refactoring needed
- **Performance**: Performance issues or bottlenecks

---

## Notes

- This audit plan is comprehensive but may need adjustment based on findings
- Some phases can be done in parallel
- Focus on critical and high-severity issues first
- Document all findings, even if they seem minor
- Keep track of false positives (things that look wrong but are actually correct)

