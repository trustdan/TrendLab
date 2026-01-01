# TrendLab GUI Resurrection Roadmap

> **Status**: Deprecated as of 2026-01-01
> **GUI Code Location**: `apps/trendlab-gui/`
> **Reason**: Feature gaps, architectural mismatch, runtime issues

---

## Executive Summary

The TrendLab GUI (Tauri v2 + React desktop app) has been deprecated in favor of the TUI (Terminal UI). All GUI code remains intact in the repository for potential future resurrection.

**To use TrendLab, run:**
```bash
trendlab --tui
```

---

## Why the GUI Was Deprecated

### 1. Feature Gaps

The GUI was missing critical features available in the TUI:

| Feature | TUI Status | GUI Status |
|---------|------------|------------|
| YOLO Mode config modal | Full (custom dates, randomization %, sweep depth) | Basic controls only |
| Risk profile cycling | 4 profiles (Balanced, Conservative, Aggressive, Sharpe) | Not connected |
| Statistical analysis | Regime splits, OOS testing, significance | Commands exist, no UI |
| Help panel | Tab 6, context-sensitive, searchable | Missing entirely |
| Pine Script export | `P` key in Leaderboard mode | Not implemented |
| Per-strategy parameters | Full customization per strategy | Stub returns empty |
| Chart modes | 6 modes | Partial implementation |

### 2. Architectural Mismatch

The GUI was originally intended as a "wrapper" around TUI functionality, but evolved into a parallel implementation:

- **Duplicate state management**: GUI has its own `AppState` wrapping the engine instead of sharing state with TUI
- **Duplicate business logic**: Many features were re-implemented rather than reused
- **Divergent behavior**: Same inputs could produce different results in TUI vs GUI

### 3. Runtime Issues

- Startup crashes in certain configurations
- Event handling inconsistencies between Tauri events and TUI worker updates
- Memory leaks in long-running YOLO sessions

---

## Preserved Assets

All GUI code remains intact and can be resurrected:

### Rust Backend (`apps/trendlab-gui/src-tauri/`)

```
src-tauri/
├── src/
│   ├── commands/           # 8 Tauri command modules (~50 commands)
│   │   ├── data.rs         # Universe, cached symbols, fetch
│   │   ├── strategy.rs     # Categories, selection, params (stub)
│   │   ├── sweep.rs        # Depth, cost model, start/cancel
│   │   ├── results.rs      # Query, sort, filter, export
│   │   ├── chart.rs        # OHLC, equity curves, trades
│   │   ├── yolo.rs         # State, leaderboards, start/stop
│   │   ├── jobs.rs         # Cancel operations
│   │   └── system.rs       # Health check
│   ├── state.rs            # AppState wrapper (643 lines)
│   ├── events.rs           # Event emission helpers
│   ├── jobs.rs             # Job lifecycle management
│   └── error.rs            # GUI error types
└── Cargo.toml
```

### React Frontend (`apps/trendlab-gui/ui/`)

```
ui/
├── src/
│   ├── components/
│   │   ├── panels/         # 5 panel components
│   │   │   ├── DataPanel.tsx
│   │   │   ├── StrategyPanel.tsx
│   │   │   ├── SweepPanel.tsx
│   │   │   ├── ResultsPanel.tsx
│   │   │   └── ChartPanel.tsx
│   │   ├── charts/         # TradingView Lightweight Charts
│   │   ├── Navigation.tsx
│   │   ├── StatusBar.tsx
│   │   └── StartupModal.tsx
│   ├── store/
│   │   ├── index.ts        # Zustand store orchestration
│   │   └── slices/         # 9 state slices
│   ├── hooks/
│   │   ├── useTauriCommand.ts
│   │   ├── useKeyboardNavigation.ts
│   │   └── useFocusManagement.ts
│   └── types/
└── package.json
```

### Original Design Documents

- `docs/roadmap-tauri-gui.md` - Original implementation plan

---

## Resurrection Phases

### Phase 1: Stabilization (2-4 hours)

**Goal**: Get the GUI to build and run without crashes.

#### 1.1 Fix Build Issues
```bash
# From project root
cd apps/trendlab-gui/ui
npm install
npm run build

cd ../src-tauri
cargo build
```

**Common issues:**
- Tauri v2 API changes since last update
- Node module version conflicts
- Rust dependency updates needed

#### 1.2 Fix Runtime Crashes
- Check `apps/trendlab-gui/src-tauri/src/main.rs` for initialization errors
- Verify IPC companion connection in `main.rs`
- Test with `cargo tauri dev -c apps/trendlab-gui/src-tauri`

#### 1.3 Fix Event Handling
**File**: `apps/trendlab-gui/ui/src/components/panels/SweepPanel.tsx`

Review all `listen()` calls and ensure proper cleanup:
```typescript
useEffect(() => {
  const unlisten = listen('sweep:progress', handler);
  return () => { unlisten.then(fn => fn()); };
}, []);
```

---

### Phase 2: Help Panel (4-6 hours)

**Goal**: Add the missing Help panel (Tab 6).

#### 2.1 Create Help Panel Component
**New file**: `apps/trendlab-gui/ui/src/components/panels/HelpPanel.tsx`

Reference the TUI implementation:
- `crates/trendlab-tui/src/panels/help.rs` (707 lines of content)

Sections to include:
1. Global shortcuts (1-6, Tab, Esc, ?)
2. Data panel shortcuts (f, s, Space, a, n)
3. Strategy panel shortcuts (e, hjkl)
4. Sweep panel shortcuts (y, Enter)
5. Results panel shortcuts (v, s, p, a, P)
6. Chart panel shortcuts (m, d, v, c)

#### 2.2 Add Help State Slice
**New file**: `apps/trendlab-gui/ui/src/store/slices/help.ts`

```typescript
interface HelpState {
  activeSection: 'global' | 'data' | 'strategy' | 'sweep' | 'results' | 'chart';
  scrollOffset: number;
  searchQuery: string;
  searchMatches: number[];
}
```

#### 2.3 Wire Up Keyboard Shortcuts
**File**: `apps/trendlab-gui/ui/src/hooks/useKeyboardNavigation.ts`

Add handlers for:
- `?` - Open Help panel
- `6` - Open Help panel (direct access)

---

### Phase 3: Complete YOLO Mode (8-12 hours)

**Goal**: Full YOLO mode parity with TUI.

#### 3.1 YOLO Config Modal
**New file**: `apps/trendlab-gui/ui/src/components/modals/YoloConfigModal.tsx`

Fields:
- Start date (arrow keys adjust 30 days)
- End date (arrow keys adjust 30 days)
- Randomization % (0-100)
- Sweep depth (Quick/Normal/Deep/Insane)

Reference: `crates/trendlab-engine/src/app/yolo.rs:53-107`

```typescript
interface YoloConfig {
  startDate: string;  // YYYY-MM-DD
  endDate: string;    // YYYY-MM-DD
  randomPct: number;  // 0-100
  sweepDepth: 'quick' | 'normal' | 'deep' | 'insane';
}
```

#### 3.2 Risk Profile Cycling

**Backend** (`apps/trendlab-gui/src-tauri/src/commands/yolo.rs`):
```rust
#[tauri::command]
pub fn get_risk_profiles() -> Vec<String> {
    vec!["balanced", "conservative", "aggressive", "sharpe"]
}

#[tauri::command]
pub fn set_risk_profile(state: State<'_, AppState>, profile: String) -> Result<(), String> {
    // Delegate to engine
}
```

**Frontend** (`apps/trendlab-gui/ui/src/store/slices/yolo.ts`):
```typescript
interface YoloSlice {
  riskProfile: 'balanced' | 'conservative' | 'aggressive' | 'sharpe';
  cycleRiskProfile: () => void;
}
```

**Keyboard**: `p` key cycles through profiles.

#### 3.3 Leaderboard Scope Toggle

**Keyboard**: `t` key toggles between Session and AllTime views.

Update `ResultsPanel.tsx` to show which leaderboard is active and total configs tested.

---

### Phase 4: Statistical Analysis (6-8 hours)

**Goal**: Add statistical analysis view to Results panel.

#### 4.1 Backend Command
**File**: `apps/trendlab-gui/src-tauri/src/commands/analysis.rs` (may need creation)

```rust
#[derive(Serialize)]
pub struct AnalysisResult {
    pub var_95: f64,
    pub cvar_95: f64,
    pub skewness: f64,
    pub kurtosis: f64,
    pub regime_performance: RegimePerformance,
    pub trade_analysis: TradeAnalysis,
}

#[tauri::command]
pub fn get_analysis(state: State<'_, AppState>, result_id: String) -> Result<AnalysisResult, String> {
    // Use trendlab_core::analysis
}
```

#### 4.2 Analysis View Component
**New file**: `apps/trendlab-gui/ui/src/components/panels/results/AnalysisView.tsx`

Display:
- Return distribution (VaR, CVaR, skewness, kurtosis)
- Regime analysis (high/neutral/low volatility)
- Trade analysis (MAE, MFE, edge ratio)

#### 4.3 Toggle Integration
**Keyboard**: `a` key toggles analysis view in Results panel.

Reference: `crates/trendlab-tui/src/app/mod.rs:1569-1625`

---

### Phase 5: Pine Script Export (4-6 hours)

**Goal**: Enable Pine Script export from Leaderboard view.

#### 5.1 Export Command
**File**: `apps/trendlab-gui/src-tauri/src/commands/pine.rs`

```rust
#[tauri::command]
pub fn export_pine_script(
    state: State<'_, AppState>,
    config_id: String,
) -> Result<String, String> {
    // Generate Pine Script v6
    // Save to pine-scripts/strategies/...
    // Return file path
}
```

#### 5.2 Export UI
**Keyboard**: `P` (Shift+P) in Leaderboard view.

Show toast/status message with:
- Success: File path where script was saved
- Option to copy to clipboard

Reference: `crates/trendlab-tui/src/app/mod.rs:1631-1704`

---

### Phase 6: Per-Strategy Parameters (6-8 hours)

**Goal**: Allow editing strategy parameters before sweep.

#### 6.1 Fix Backend Stub
**File**: `apps/trendlab-gui/src-tauri/src/state.rs` (lines 394-409)

Current stub returns empty HashMap. Fix to:
- Store parameters per strategy in engine state
- Return actual parameter values and ranges

```rust
#[tauri::command]
pub fn get_strategy_params(
    state: State<'_, AppState>,
    strategy_id: String,
) -> Result<StrategyParams, String> {
    // Return actual params with min/max/step
}

#[tauri::command]
pub fn update_strategy_params(
    state: State<'_, AppState>,
    strategy_id: String,
    params: HashMap<String, f64>,
) -> Result<(), String> {
    // Persist to engine state
}
```

#### 6.2 Parameter Editor Component
**File**: `apps/trendlab-gui/ui/src/components/panels/strategy/ParameterEditor.tsx`

- Input fields for each configurable parameter
- Min/max constraints with validation
- Arrow keys to adjust values (TUI parity)

Reference: `crates/trendlab-tui/src/app/mod.rs:1024-1166`

---

### Phase 7: Polish (4-6 hours)

**Goal**: Final polish and documentation sync.

#### 7.1 Chart Mode Parity
Verify all 6 chart modes work:
1. Single - Single equity curve
2. Candlestick - OHLC with entry/exit markers
3. MultiTicker - Multiple ticker curves overlaid
4. Portfolio - Combined portfolio equity
5. StrategyComparison - Best config per strategy
6. PerTickerBestStrategy - Best strategy per ticker

**Keyboard**: `m` cycles through modes.

#### 7.2 Documentation Updates
- Update `README.md` GUI Keyboard Shortcuts section
- Update `CLAUDE.md` to remove deprecation notice
- Update Help panel content to match TUI exactly

#### 7.3 Performance Testing
- Test with 1000+ result configs
- Test with 50+ ticker multi-charts
- Profile memory usage in long YOLO sessions

#### 7.4 Cross-Platform Testing
- Windows: Primary platform
- macOS: If available
- Linux: If available

---

## Key Files Reference

| Component | TUI Reference | GUI Location |
|-----------|---------------|--------------|
| YOLO State | `crates/trendlab-engine/src/app/yolo.rs` | `src-tauri/src/commands/yolo.rs` |
| Help Panel | `crates/trendlab-tui/src/panels/help.rs` | Need to create |
| Risk Profiles | `RiskProfile` enum in engine | Need to expose |
| Analysis | `crates/trendlab-core/src/analysis.rs` | Partial in commands |
| Pine Export | `crates/trendlab-tui/src/app/mod.rs:1631-1704` | Need to create |
| Params | `crates/trendlab-tui/src/app/strategies.rs` | Stub at `state.rs:394-409` |

---

## Success Criteria

Before considering resurrection complete, verify:

- [ ] All 6 panels accessible (Data, Strategy, Sweep, Results, Chart, Help)
- [ ] Complete YOLO mode (config modal, risk profiles, scope toggle)
- [ ] Statistical analysis view functional
- [ ] Pine Script export works from Leaderboard
- [ ] Per-strategy parameter editing persists
- [ ] All TUI keyboard shortcuts work identically
- [ ] No runtime crashes in normal operation
- [ ] YOLO mode runs 10+ iterations without crash
- [ ] Sweep completes 500+ configs without crash
- [ ] Documentation synced (README.md, CLAUDE.md, Help panel)

---

## Architecture Recommendation

**Keep the parallel implementation** with better abstraction:

1. **Why not wrap TUI?**
   - Tauri's async command model differs from TUI's sync event loop
   - Web-based charting (TradingView) has different capabilities than ratatui
   - Would require embedding a terminal emulator

2. **Focus areas for improvement:**
   - Ensure GUI `AppState` properly delegates to engine
   - Use engine types directly (already happening)
   - Add missing engine features to GUI commands
   - Keep GUI and TUI in sync via shared engine updates

3. **Long-term maintenance:**
   - Accept that two UIs require maintenance
   - Add integration tests that verify GUI/TUI parity
   - Consider extracting more logic into `trendlab-engine`

---

## Build Commands

```bash
# Development (hot reload)
cargo tauri dev -c apps/trendlab-gui/src-tauri

# Production build
cargo tauri build -c apps/trendlab-gui/src-tauri

# Run tests
cd apps/trendlab-gui/ui && npm test
cd apps/trendlab-gui/src-tauri && cargo test
```

---

## Contact

For questions about resurrection:
- TUI implementation: `crates/trendlab-tui/`
- Core logic: `crates/trendlab-core/`
- Engine state: `crates/trendlab-engine/`
- Original roadmap: `docs/roadmap-tauri-gui.md`
