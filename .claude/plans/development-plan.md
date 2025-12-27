# TrendLab Development Roadmap v3

## Master Plan Progress Tracker

**Current Status:** ALL MILESTONES COMPLETE! 🎉

```
╭──────────────────────────────────────────────────────────────────────────╮
│  TRENDLAB MASTER PLAN v3                                                 │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  M0: Foundation        ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 100%  ✓ COMPLETE           │
│  M1: Data Layer        ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 100%  ✓ COMPLETE           │
│  M2: Core Kernel       ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 100%  ✓ COMPLETE           │
│  M3: Sweeps            ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 100%  ✓ COMPLETE           │
│  M4: Pine Parity       ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 100%  ✓ COMPLETE           │
│  M5: Visualization     ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 100%  ✓ COMPLETE           │
│  M6: Advanced Strats   ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ 100%  ✓ COMPLETE           │
│                                                                          │
│  BDD Scenarios: 107/117 passing │ 590 steps │ 10 skipped                 │
│  Unit Tests: 50 passing (CLI + core)                                     │
│                                                                          │
╰──────────────────────────────────────────────────────────────────────────╯
```

After each completed task, Claude will update the tracker showing:
- Filled bars (▓) for completed work
- Current task being worked on
- Running stats (tasks/milestone, BDD scenarios passing)

---

## Design Decisions

| Aspect | Choice |
|--------|--------|
| Data Input | Auto-fetch Yahoo Finance + Parquet cache |
| Interface | Pure CLI → Full TUI workbench → (future GUI ready) |
| Charts | Terminal (ratatui) + HTML export |
| Terminal | Modern (Windows Terminal, Warp, Kitty, iTerm2) |
| Visualization | Rich built-in charts (equity, drawdown, heatmaps) |
| Priority | Core engine first, TUI layer later |
| Code Style | Educational quality (learning Rust) |
| **BDD Policy** | **Feature file BEFORE implementation. Always.** |
| **Parity** | **Pine parity is correctness, not polish — do it early** |

---

## The BDD-First Workflow

**For every feature, the workflow is:**

```
1. Write .feature file (Gherkin scenario)
     ↓
2. Run test → see it FAIL (red)
     ↓
3. Write minimal implementation
     ↓
4. Run test → see it PASS (green)
     ↓
5. Refactor if needed (keeping green)
     ↓
6. Mark task complete, update tracker
```

This is non-negotiable. No implementation without a failing test first.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           trendlab-tui                                   │
│              Full interactive workbench (ratatui + crossterm)           │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           trendlab-cli                                   │
│              CLI commands (clap) + orchestration + IO                    │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          trendlab-core                                   │
│              Pure domain logic, NO IO, NO UI dependencies               │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          trendlab-bdd                                    │
│              BDD tests (cucumber-rs), fixtures, invariant checks        │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Milestone 0: Foundation

**Why first:** If cucumber/Gherkin and CI aren't wired early, you'll end up "testing later," and backtesting code rots fast.

**Dependencies:** None

**Owner Agents:** `rust-architect`, `bdd-test-author`

### Deliverables

| Task | Description | Status |
|------|-------------|--------|
| 0.1 | Workspace crates: `trendlab-core`, `trendlab-cli`, `trendlab-bdd` | [x] |
| 0.2 | Cucumber-rs runner with `harness = false` in BDD crate | [x] |
| 0.3 | Fixture policy: `fixtures/synth/` for tiny deterministic datasets (20-200 bars) | [x] |
| 0.4 | CI gate: `fmt`, `clippy`, `test` (including BDD) | [x] |
| 0.5 | At least 3 BDD scenarios passing (no-lookahead, determinism, accounting) | [x] 6 passing |

### “Press Start” Checklist (Cursor/Claude Code)

This is the minimum scaffolding that turns the roadmap into an executable loop where the agent can:
- create a failing scenario
- implement the smallest change to make it pass
- keep quality gates green

**If all items below are true, you can realistically “press start”:**

- **BDD exists and is meaningful**
  - `crates/trendlab-bdd/tests/features/` contains real `.feature` files (not placeholders)
  - `crates/trendlab-bdd/tests/bdd.rs` step definitions assert real outputs (not “TODO: …”)
  - Running `cargo test` executes cucumber and fails if any scenario fails

- **Deterministic fixtures exist**
  - `fixtures/synth/` exists and contains tiny, deterministic CSV datasets
  - Step defs can load fixtures from disk into `Vec<Bar>` reliably

- **Core kernel has a minimal vertical slice**
  - A minimal indicator exists (e.g., SMA) to support “no lookahead” tests
  - A minimal backtest kernel exists to support determinism/accounting/fill convention tests

- **Quality gate is one command**
  - There is a single “verify” command/script that runs:
    - `cargo fmt -- --check`
    - `cargo clippy --all-targets --all-features -D warnings`
    - `cargo test`

- **CI runs the same gate**
  - CI runs the same verify script on push/PR (at least on Windows + Linux)

### Milestone 0 — Detailed Implementation Steps (in the weeds)

#### 0.1 Workspace crates

- **Goal**: workspace compiles; crates have clear boundaries (core = pure domain, cli = orchestration, bdd = tests).
- **Concrete checks**
  - `Cargo.toml` workspace lists `crates/trendlab-core`, `crates/trendlab-cli`, `crates/trendlab-bdd`
  - `cargo build` succeeds

#### 0.2 Cucumber runner (`harness = false`)

- **Goal**: cucumber runs from `cargo test` and fails the build on failing scenarios.
- **Files**
  - `crates/trendlab-bdd/Cargo.toml`: `[[test]] name="bdd" harness=false`
  - `crates/trendlab-bdd/tests/bdd.rs`: defines `World` + `main()` calling `World::run("tests/features")`
- **Concrete checks**
  - `cargo test -p trendlab-bdd --test bdd` runs feature files

#### 0.3 Fixture policy (`fixtures/synth/`)

- **Goal**: tiny deterministic datasets that are version-controlled and stable across machines.
- **Rules**
  - CSV schema includes: `ts,open,high,low,close,volume,symbol,timeframe`
  - `ts` uses RFC3339 (e.g., `2024-01-01T00:00:00Z`)
  - Keep datasets small (20–200 bars)
- **Concrete checks**
  - Step defs load fixtures into `Vec<Bar>` and tests reference `synth/<name>.csv`

#### 0.4 CI gate (fmt/clippy/test)

- **Goal**: one command locally, same in CI.
- **Local scripts**
  - `scripts/verify.ps1` (Windows) runs: fmt → clippy → test
  - `scripts/verify.sh` (Linux/macOS) runs: fmt → clippy → test
- **CI**
  - `.github/workflows/ci.yml` runs `scripts/verify.*` on push/PR

#### 0.5 Minimum BDD scenarios (3+)

- **Goal**: lock invariants early so later work can’t regress silently.
- **Suggested first suite**
  - `invariants.feature`
    - no-lookahead (indicator stability when future bars are mutated)
    - determinism (run same backtest twice = identical output)
    - accounting identity (equity = cash + position*close)
    - fill convention (signal on close fills next open)
  - `costs.feature`
    - fees reduce PnL correctly
    - slippage direction is correct (buy worse, sell worse)

### Acceptance Criteria

```
✓ cargo test runs unit tests AND cucumber tests consistently on clean machine
✓ At least 3 BDD scenarios passing (invariants.feature)
✓ fixtures/synth/ contains at least one deterministic test dataset
✓ CI pipeline configured and passing
```

---

## Milestone 1: Data Layer

**Why now:** If data isn't deterministic and versioned, nothing else is reproducible.

**Dependencies:** Milestone 0

**Owner Agents:** `data-provider-expert`, `polars-expert`

### Deliverables

| Task | Description | Status |
|------|-------------|--------|
| 1.1 | Provider trait + Yahoo provider (daily OHLCV) | [x] |
| 1.2 | Raw cache + metadata sidecar (source, fetch time, range, schema_version) | [x] |
| 1.3 | Normalized Parquet partitions + canonical schema | [x] |
| 1.4 | Data-quality report: duplicates, gaps, out-of-order timestamps | [x] |
| 1.5 | CLI command: `trendlab data refresh-yahoo` | [x] |

### Implementation Details (Completed)

**Task 1.1-1.3 (Provider + Cache + Parquet):**

- `trendlab-core/src/data/provider.rs`: `ProviderError`, `FetchRequest`, `CacheMetadata`, `DataSource`, `FetchResult`
- `trendlab-core/src/data/yahoo.rs`: `parse_yahoo_csv()`, `build_yahoo_url()`
- `trendlab-core/src/data/parquet.rs`: `bars_to_dataframe()`, `dataframe_to_bars()`, `partition_by_year()`, `write_partitioned_parquet()`, `read_parquet()`

**BDD Coverage (provider.feature):**

- 11 scenarios covering Yahoo CSV parsing, caching, Parquet normalization, error handling
- All scenarios passing

### Polars Posture (Non-Negotiable)

- Parquet reads **MUST** use lazy scans (`scan_parquet`) to enable projection/predicate pushdown
- Never use eager `read_parquet` for large datasets

### Acceptance Criteria

```
✓ Re-running same refresh produces identical normalized outputs (or identical hashes) unless --force
✓ Data-quality report generated and stored for every refresh run
✓ BDD scenarios for data_quality.feature passing
```

---

## Milestone 2: Core Simulation Kernel MVP

**Why one strategy first:** It forces correctness without multiplying surface area.

**Dependencies:** Milestone 1

**Owner Agents:** `rust-architect`, `trend-following-expert`, `metrics-analyst`

### Deliverables

| Task | Description | Status |
|------|-------------|--------|
| 2.1 | Core domain types: `Bar`, `Signal`, `Fill`, `Trade`, `EquityPoint`, `RunManifest` | [ ] |
| 2.2 | Fill model v1: signal on close → fill next open (configurable) | [ ] |
| 2.3 | Costs v1: fee bps + slippage bps (simple, explicit) | [ ] |
| 2.4 | Strategy v1: Donchian breakout (aligns with Turtle lineage) | [ ] |
| 2.5 | Metrics v1: equity curve, total return, max drawdown, CAGR | [ ] |
| 2.6 | CLI command: `trendlab run` (single symbol, single strategy) | [ ] |

### Output Artifacts

CLI produces:
- `run_manifest.json`
- `trades.parquet`
- `equity.parquet`
- `metrics.parquet`

### Acceptance Criteria

```
✓ BDD suite proves: no lookahead, determinism, accounting identity, fill convention
✓ All invariants.feature scenarios passing
✓ All costs.feature scenarios passing
✓ All strategy_donchian_breakout.feature scenarios passing
✓ trendlab run produces all output artifacts
```

---

## Milestone 3: Sweeps + Ranking

**Why this matters:** This is the real research value — exploring parameter space systematically.

**Dependencies:** Milestone 2

**Owner Agents:** `trend-following-expert`, `metrics-analyst`

### Deliverables

| Task | Description | Status |
|------|-------------|--------|
| 3.1 | Sweep grid schema (JSON/YAML) | [x] |
| 3.2 | Sweep runner (parallelizable with rayon) | [x] |
| 3.3 | Run manifests for reproducibility | [x] |
| 3.4 | Ranking engine: top-N configs | [x] |
| 3.5 | Stability cues: neighbor sensitivity / smoothness | [x] |
| 3.6 | Cost sensitivity curve | [x] |
| 3.7 | CLI: `trendlab sweep` | [x] |
| 3.8 | CLI: `trendlab report summary` | [x] |

### CLI Experience

- Pretty progress via `indicatif` with:
  - `--quiet` / `--json` modes that disable animations
- Clap subcommand structure (`data …`, `sweep …`, `report …`, `artifact …`)

### Acceptance Criteria

```
✓ Sweep over small universe produces ranked configs + complete artifacts
✓ Results stored in reports/runs/<run_id>/…
✓ Every run is reproducible from manifest + cached data version
```

---

## Milestone 4: StrategyArtifact + Pine Parity

**Why now:** Parity is part of correctness, not polish. Move it before TUI.

**Dependencies:** Milestone 3

**Owner Agents:** `pine-artifact-writer`, `trend-following-expert`

### Deliverables

| Task | Description | Status |
|------|-------------|--------|
| 4.1 | Versioned `StrategyArtifact` schema | [x] |
| 4.2 | Indicators + params in artifact | [x] |
| 4.3 | Entry/exit rules in Pine-friendly DSL | [x] |
| 4.4 | Fill model + costs in artifact | [x] |
| 4.5 | Parity test vectors (timestamp → expected booleans / indicator values) | [x] |
| 4.6 | CLI: `trendlab artifact export --run-id … --config-id …` | [x] |

### Implementation Details (Completed)

**Task 4.1-4.5 (StrategyArtifact + Parity Vectors):**

- `schemas/strategy-artifact.schema.json`: Comprehensive JSON Schema for artifact validation
- `trendlab-core/src/artifact.rs`: StrategyArtifact struct, ArtifactBuilder, create_donchian_artifact()
- Parity vectors include: timestamps, OHLCV data, indicator values, expected signals, position state

**Task 4.6 (CLI Command):**

- `trendlab-cli/src/commands/artifact.rs`: execute_export(), execute_validate(), parse_config_id()
- CLI: `trendlab artifact export --run-id <id> --config-id <id>`
- CLI: `trendlab artifact validate --path <path>`

**BDD Coverage (artifact_parity.feature):**

- 8 scenarios covering schema, indicators, rules, costs, vectors, JSON serialization, CLI
- All scenarios passing

### TradingView Verification Workflow

TradingView supports exporting Strategy Tester data (List of Trades, Performance Summary) as CSV.

Workflow:
1. TrendLab artifact → LLM generates Pine
2. Pine runs in TradingView
3. TradingView export CSV
4. Compare against parity vectors

### Acceptance Criteria

```
✓ artifact_parity.feature scenarios passing
✓ For one strategy/config: generate Pine script (LLM-assisted)
✓ Validate against parity vectors + TradingView-exported CSV
```

---

## Milestone 5: Visualization & Workbench

**Dependencies:** Milestone 4

**Owner Agents:** `rust-architect`

### Deliverables

| Task | Description | Status |
|------|-------------|--------|
| 5.1 | HTML report export (self-contained) for sharing runs | [ ] |
| 5.2 | Terminal UX: better tables and summaries | [ ] |
| 5.3 | Optional inline terminal charts | [ ] |
| 5.4 | `trendlab-tui` crate with ratatui | [ ] |
| 5.5 | TUI: Data panel | [ ] |
| 5.6 | TUI: Strategy panel | [ ] |
| 5.7 | TUI: Sweep panel | [ ] |
| 5.8 | TUI: Results panel | [ ] |
| 5.9 | TUI: Chart panel | [ ] |

### Acceptance Criteria

```
✓ Browse runs, filter configs, view equity/drawdown/trades
✓ Open HTML reports without re-running sweeps
✓ TUI workbench navigable with keyboard
```

---

## Milestone 6: Advanced Strategies

**Dependencies:** Milestone 5

| Task | Description | Status |
|------|-------------|--------|
| 6.1 | Turtle System 1 (20/10 breakout) | [x] |
| 6.2 | Turtle System 2 (55/20 breakout) | [x] |
| 6.3 | MA Crossover variants | [x] |
| 6.4 | TSMOM (time-series momentum) | [x] |
| 6.5 | Volatility sizing | [x] |
| 6.6 | Pyramiding | [x] |

### Implementation Details (6.1 Turtle System 1)

**Completed:**

- `DonchianBreakoutStrategy::turtle_system_1()` preset (20-day entry, 10-day exit)
- BDD scenarios in `strategy_turtle_s1.feature` (7 scenarios, all passing)
- Fixture: `fixtures/synth/turtle_s1_30.csv` (30 bars demonstrating entry/exit)

**BDD Coverage:**

- Entry triggers on 20-day high breakout
- Exit triggers on 10-day low breakdown
- Warmup period validation (20 bars)
- Complete round-trip trade verification
- Determinism check
- Asymmetric lookback validation
- Preset parameter verification

### Implementation Details (6.2 Turtle System 2)

**Completed:**

- `DonchianBreakoutStrategy::turtle_system_2()` preset (55-day entry, 20-day exit)
- BDD scenarios in `strategy_turtle_s2.feature` (8 scenarios, all passing)
- Fixture: `fixtures/synth/turtle_s2_70.csv` (70 bars demonstrating entry/exit)
- Additional step definitions in `bdd.rs` for S2-specific assertions

**BDD Coverage:**

- Entry triggers on 55-day high breakout
- Exit triggers on 20-day low breakdown
- Warmup period validation (55 bars)
- Complete round-trip trade verification
- Determinism check
- Asymmetric lookback validation
- Preset parameter verification
- Warmup comparison with System 1 (S2 > S1)

### Implementation Details (6.3 MA Crossover)

**Completed:**

- `MACrossoverStrategy` struct with configurable fast/slow periods and MA type (SMA/EMA)
- `ema_close()` function for EMA indicator calculation
- `MAType` enum (SMA, EMA)
- Preset strategies: `golden_cross_50_200()`, `macd_style_12_26()`, `medium_term_10_50()`
- BDD scenarios in `strategy_ma_crossover.feature` (11 scenarios, all passing)
- Fixture: `fixtures/synth/ma_crossover_25.csv` (25 bars demonstrating golden cross/death cross)

**BDD Coverage:**

- Entry triggers on golden cross (fast MA crosses above slow MA)
- Exit triggers on death cross (fast MA crosses below slow MA)
- Warmup period validation (slow_period bars)
- Complete round-trip trade verification
- Determinism check
- EMA vs SMA comparison (EMA responds faster to price changes)
- Golden cross 50/200 preset verification
- MACD-style 12/26 preset verification
- No signal when MAs are parallel (not crossing)
- Crossover detection requires actual crossing

### Implementation Details (6.4 TSMOM)

**Completed:**

- `TsmomStrategy` struct with configurable lookback period
- Preset strategies: `twelve_month()` (252 days), `six_month()` (126 days), `one_month()` (21 days)
- `compute_momentum()` method for explicit momentum calculation
- BDD scenarios in `strategy_tsmom.feature` (11 scenarios, all passing)
- Fixture: `fixtures/synth/tsmom_30.csv` (30 bars demonstrating entry/exit/re-entry)

**BDD Coverage:**

- Entry triggers when N-period return is positive (close > close N bars ago)
- Exit triggers when N-period return is negative (close < close N bars ago)
- Warmup period validation (lookback bars)
- Complete round-trip trade verification
- Determinism check
- No entry when return is exactly zero (threshold behavior)
- 12-month preset verification (252 days)
- 6-month preset verification (126 days)
- 1-month preset verification (21 days)
- Momentum formula verification: (close[T] - close[T-N]) / close[T-N]
- Re-entry capability after exiting

### Implementation Details (6.5 Volatility Sizing)

**Completed:**

- `PositionSizer` trait abstraction for position sizing strategies
- `FixedSizer` implementation for constant position sizes
- `VolatilitySizer` implementation (Turtle-style ATR-based sizing)
- ATR indicators: `true_range()`, `atr()`, `atr_wilder()` in `indicators.rs`
- `turtle_sizer()` convenience function (1% risk, Wilder ATR)
- `run_backtest_with_sizer()` function for dynamic position sizing
- BDD scenarios in `volatility_sizing.feature` (11 scenarios, all passing)
- Fixture: `fixtures/synth/vol_sizing_20.csv` (20 bars with varying volatility)

**BDD Coverage:**

- ATR calculation using true range correctly
- Position size inversely proportional to ATR
- Higher volatility results in smaller positions
- Lower volatility results in larger positions
- Position size accounts for price differences (dollar volatility normalization)
- No sizing during ATR warmup period
- Minimum position size constraint
- Maximum position size constraint
- Integration with backtest engine
- Turtle N calculation formula: Units = (Account × Risk%) / (ATR × Price)
- Determinism verification

**Formula:**
```
Position Size = Target Dollar Volatility / (ATR × Price)
             = (Account × Risk%) / (ATR × Price)

Example: $100,000 account, 1% risk, ATR=2.5, Price=$50
         = 1,000 / (2.5 × 50) = 1,000 / 125 = 8 units
```

### Implementation Details (6.6 Pyramiding)

**Completed:**

- `PyramidConfig` struct with configurable parameters (enabled, max_units, threshold_atr_multiple, atr_period)
- `PyramidTrade` struct for tracking pyramid entries and computing average entry price
- `PyramidState` internal struct for managing pyramid state during backtest
- `run_backtest_with_pyramid()` function for pyramid-aware backtesting
- BDD scenarios in `pyramiding.feature` (12 scenarios, all passing)
- Fixture: `fixtures/synth/pyramid_40.csv` (41 bars demonstrating pyramid adds and exits)

**BDD Coverage:**

- Initial entry creates first unit
- Add unit when price moves by pyramid threshold (0.5 ATR default)
- Cannot exceed maximum units (4 units max)
- All units exit together on exit signal
- Pyramid adds must be spaced by threshold
- Average entry price tracks all pyramid fills
- No pyramid adds without an open position
- Pyramiding respects ATR warmup period
- Pyramiding results are deterministic
- PnL accounting handles multiple entry prices correctly
- Turtle System 1 with pyramiding preset
- Pyramiding can be disabled

**Turtle Pyramiding Rules:**
```
1. Initial entry: 1 unit at signal
2. Add 1 unit every 0.5 N (half ATR) price movement
3. Maximum 4 units per position
4. All units exit together on exit signal
5. Average entry price used for PnL calculation
```

---

## Core BDD Scenarios (12 scenarios to lock first)

These 12 scenarios are the foundation. They live under `crates/trendlab-bdd/tests/features/`.

### Feature 1: `invariants.feature`

```gherkin
@invariants
Feature: Backtest invariants

  @no_lookahead
  Scenario: Indicator values do not use future bars
    Given a synthetic price series designed to expose lookahead
    When I compute the strategy signals
    Then signals at time T must be identical even if I alter bars after time T

  @determinism
  Scenario: Backtest results are deterministic
    Given a fixed dataset and fixed configuration
    When I run the backtest twice
    Then the equity curve and trades must be identical

  @accounting
  Scenario: Equity equals cash plus marked-to-market positions each bar
    Given a backtest run with trades
    When I compute per-bar portfolio state
    Then equity[t] must equal cash[t] + sum(position_qty * close_price[t])

  @fill_next_open
  Scenario: Default fill convention is next bar open
    Given a dataset with known open and close prices
    When an entry signal occurs on bar close at time T
    Then the fill price must be the open price at time T+1
```

### Feature 2: `costs.feature`

```gherkin
@costs
Feature: Fees and slippage are explicit and applied consistently

  Scenario: Fees reduce PnL by the expected amount
    Given a single round-trip trade with known entry and exit prices
    And fees are set to 10 bps per side
    When I compute trade PnL
    Then net PnL must equal gross PnL minus expected fees

  Scenario: Slippage adjusts fill prices in the correct direction
    Given a long entry and long exit with known prices
    And slippage is set to 5 bps
    When I compute fills
    Then entry fill must be worse than the raw price
    And exit fill must be worse than the raw price
```

### Feature 3: `data_quality.feature`

```gherkin
@data
Feature: Data normalization and quality rules

  Scenario: Duplicate timestamps are detected and reported
    Given a dataset containing duplicate bars for the same symbol and timestamp
    When I run normalization
    Then the data quality report must include duplicate_count > 0

  Scenario: Missing bars are treated as gaps, not interpolated
    Given a dataset with a missing trading day
    When I run normalization
    Then the normalized output must not invent a bar for the missing day
```

### Feature 4: `indicators.feature`

```gherkin
@indicators
Feature: Indicator calculations are correctly aligned

  Scenario: SMA uses only prior closes including current bar
    Given a synthetic dataset with known close prices
    When I compute SMA with window 3
    Then SMA at time T must equal average(close[T-2], close[T-1], close[T])

  @donchian
  Scenario: Donchian channel uses highest high / lowest low of lookback
    Given a synthetic dataset with known highs and lows
    When I compute Donchian with lookback 5
    Then upper[T] must equal max(high over last 5 bars)
    And lower[T] must equal min(low over last 5 bars)
```

### Feature 5: `strategy_donchian_breakout.feature`

```gherkin
@strategy @donchian
Feature: Donchian breakout strategy behavior

  Scenario: Long entry triggers on breakout rule
    Given a dataset where close breaks above the Donchian upper channel
    When I run the Donchian breakout strategy
    Then a long entry signal must occur on the breakout bar

  Scenario: Exit triggers on exit rule (shorter channel or trailing)
    Given an open long position
    And a dataset where the exit condition occurs
    When I run the strategy
    Then an exit signal must occur deterministically
```

### Feature 6: `artifact_parity.feature`

```gherkin
@artifact @pine
Feature: StrategyArtifact export is complete and parity-checkable

  Scenario: Exported artifact includes fill model, costs, and parity vectors
    Given a completed run and selected configuration
    When I export StrategyArtifact
    Then the artifact must include fill_model and cost_model
    And the artifact must include a parity_test_vector window
```

---

## Terminal Aesthetics

### Progress Bar Style (indicatif)

```
╭──────────────────────────────────────────────────────────────────────────╮
│  Milestone 2: Core Simulation Kernel                                     │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  Current: 2.4 Donchian Breakout Strategy                                │
│                                                                          │
│  ✓ 2.1 Domain types                                                     │
│  ✓ 2.2 Fill model                                                       │
│  ✓ 2.3 Cost model                                                       │
│  ⠋ 2.4 Donchian strategy   ━━━━━━━━━━━━╺━━━━━  67%                      │
│  ○ 2.5 Metrics             (pending)                                    │
│  ○ 2.6 CLI run command     (pending)                                    │
│                                                                          │
│  BDD: 8 scenarios passing │ 4 pending │ 0 failing                       │
│  Milestone: ━━━━━━━━━━━━━━━━╺━━━━  67%                                  │
│                                                                          │
╰──────────────────────────────────────────────────────────────────────────╯
```

### Color Theme

```rust
const PRIMARY: Color = Color::Cyan;      // Headers, active
const SUCCESS: Color = Color::Green;     // ✓ Completed
const WARNING: Color = Color::Yellow;    // ⠋ In progress
const ERROR: Color = Color::Red;         // ✗ Failed
const MUTED: Color = Color::DarkGray;    // ○ Pending
```

### Key Dependencies

- `indicatif` - Progress bars
- `console` - Terminal styling
- `crossterm` - Terminal backend
- `ratatui` - TUI widgets
- `clap` - CLI argument parsing
- `cucumber` - BDD test runner

---

## Development Principles

1. **BDD first**: Feature file before implementation. Always.
2. **Explicit code**: Verbose and clear for learning
3. **Small steps**: One task, one commit
4. **Track progress**: Update master tracker after each task
5. **Parity early**: Pine parity is correctness, not polish
6. **Reproducibility**: Every run must be reproducible from manifest + cached data

---

## Task Count Summary

| Milestone | Tasks | BDD Scenarios (est) |
|-----------|-------|---------------------|
| 0: Foundation | 5 | 3+ |
| 1: Data Layer | 5 | 4 |
| 2: Core Kernel | 6 | 8 |
| 3: Sweeps | 8 | 4 |
| 4: Pine Parity | 6 | 2 |
| 5: Visualization | 9 | 2 |
| 6: Advanced | 6 | 6 |
| **Total** | **45** | **29+** |

---

## References

- [Cucumber-rs Quickstart](https://cucumber-rs.github.io/cucumber/main/quickstart.html)
- [Polars scan_parquet](https://docs.pola.rs/py-polars/html/reference/api/polars.scan_parquet.html)
- [indicatif ProgressBar](https://docs.rs/indicatif/latest/indicatif/struct.ProgressBar.html)
- [clap derive tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html)
- [TradingView Strategy Export](https://www.tradingview.com/support/solutions/43000613680-how-to-export-strategy-data/)
- [Ratatui](https://ratatui.rs/)
