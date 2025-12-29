# TrendLab (Rust + Polars) — CLAUDE.md

## Mission
Build a research-grade trend-following backtesting lab in Rust, optimized for:
- large parameter sweeps across many symbols
- robust statistical evaluation (OOS, regime splits, turnover/cost sensitivity)
- deterministic, testable behavior (BDD first)
- outputting "strategy artifacts" that can be used to generate parity Pine Scripts later

This is NOT a live-trading engine. Favor correctness, reproducibility, and speed of experimentation.

---

## Repository Structure

```
TrendLab/
├── .claude/
│   ├── agents/           # Subagent definitions (10 agents)
│   ├── commands/         # Slash command skills (9 skills)
│   └── rules.md          # Operational rules for Claude
├── apps/
│   └── trendlab-gui/     # Desktop GUI (Tauri v2 + React)
│       ├── src-tauri/    # Rust backend
│       │   ├── src/
│       │   │   ├── commands/  # Tauri command handlers
│       │   │   ├── state.rs   # AppState with RwLock wrappers
│       │   │   ├── events.rs  # Event emission helpers
│       │   │   ├── error.rs   # GUI error types
│       │   │   └── jobs.rs    # Job lifecycle management
│       │   └── Cargo.toml
│       └── ui/           # React frontend
│           ├── src/
│           │   ├── components/    # React components
│           │   │   ├── panels/    # Panel components (Data, Strategy, Sweep, Results, Chart)
│           │   │   ├── charts/    # TradingView Lightweight Charts wrappers
│           │   │   └── *.tsx      # Navigation, StatusBar, StartupModal
│           │   ├── hooks/         # Custom hooks (useTauriCommand, useKeyboardNavigation)
│           │   ├── store/         # Zustand store with slices
│           │   └── types/         # TypeScript type definitions
│           └── package.json
├── crates/
│   ├── trendlab-core/    # Domain types, strategies, metrics
│   ├── trendlab-cli/     # CLI interface
│   ├── trendlab-tui/     # Terminal UI (ratatui)
│   └── trendlab-bdd/     # Cucumber-rs runner + step definitions
│       └── tests/
│           ├── bdd.rs    # Runner (harness=false)
│           └── features/ # Gherkin .feature files
├── docs/
│   ├── assumptions.md    # Fill conventions, adj/unadj, timezone, missing bars
│   ├── schema.md         # Canonical bar schema + units
│   ├── bdd-style.md      # Scenario conventions, tags, step naming
│   └── pine/             # Pine parity notes (subset of pine-script-docs)
├── fixtures/             # Tiny deterministic datasets (20-200 bars) for tests
├── data/                 # GITIGNORED — real market data
│   ├── raw/              # Cached raw provider responses
│   └── parquet/          # Normalized partitioned Parquet
├── schemas/              # JSON schemas (StrategyArtifact, configs)
├── configs/              # Default sweep grids, report templates
├── reports/              # Sweep results, data quality reports
├── artifacts/            # StrategyArtifact JSON exports
└── pine-script-docs/     # TradingView Pine Script v6 reference (full mirror)
```

---

## Subagents

Delegate immediately when a task matches an agent's domain. Agents are in `.claude/agents/`.

| Agent | Use When |
|-------|----------|
| **rust-architect** | Crate layout, traits, APIs, domain types, correctness invariants |
| **polars-expert** | LazyFrame pipelines, Parquet I/O, joins, window ops, performance |
| **trend-following-expert** | Strategy design (MA cross, breakouts, momentum), parameter grids, robustness tests |
| **data-provider-expert** | Yahoo Finance ingestion, caching, normalization, data quality |
| **bdd-test-author** | Gherkin feature files + cucumber-rs step definitions |
| **metrics-analyst** | CAGR, Sharpe, drawdown, turnover, ranking logic, cost sensitivity |
| **pine-artifact-writer** | StrategyArtifact schema, Pine-friendly DSL, parity test vectors |
| **tauri-expert** | Tauri desktop apps, Rust backend commands, IPC, window management |
| **financial-charts-expert** | Candlestick charts, indicators, TradingView Lightweight Charts |
| **web-frontend-expert** | TypeScript/web frontends for Tauri, component architecture, state management |

---

## Slash Commands (Skills)

Invoke with `/skill-name` or via the Skill tool.

### TrendLab Core
| Command | Purpose |
|---------|---------|
| `/trendlab:bootstrap` | Create Rust workspace skeleton (crates, dirs, CLI placeholders) |
| `/trendlab:new-strategy [id]` | Create strategy spec + BDD scenarios + code skeleton |
| `/trendlab:run-sweep [strategy] [universe] [start] [end]` | Run parameter sweep, write Parquet + summary |

### Data
| Command | Purpose |
|---------|---------|
| `/data:refresh-yahoo [tickers] [start] [end]` | Fetch/refresh Yahoo daily bars to Parquet cache |

### Metrics
| Command | Purpose |
|---------|---------|
| `/metrics:add-metric [name]` | Add new metric end-to-end (calc + schema + tests + docs) |

### Pine Script Export
| Command | Purpose |
|---------|---------|
| `/pine:generate [strategy] [config]` | Generate Pine Script v6 and save to pine-scripts/ |
| `/pine:export-artifact [run_id] [config_id]` | Export StrategyArtifact JSON for Pine generation |
| `/pine:parity-check [artifact.json]` | Create parity checklist + test vectors for Pine validation |

#### Pine Script Generation Pipeline

When you ask for a Pine Script (e.g., "generate pine script for 52-week high 80/70/59"):

1. **Identify**: Parse strategy type and config from your request or TUI screenshot
2. **Artifact Lookup**: Search `artifacts/exports/` for matching artifact JSON
3. **Auto-Create**: If no artifact exists, create one from strategy parameters
4. **Generate**: Build Pine Script v6 using artifact's indicators and rules
5. **Save**: Write to `pine-scripts/strategies/<strategy>/<config>.pine`
6. **Index**: Update `pine-scripts/README.md` with new entry

**Invoke**: `/pine:generate [strategy] [config]` or ask naturally

**Output Location**: `pine-scripts/strategies/<strategy_id>/<params>.pine`

**Example**:
- Input: "generate pine script for the top 52-week high config"
- Output: `pine-scripts/strategies/52wk_high/80_70_59.pine`

### Development
| Command | Purpose |
|---------|---------|
| `/dev:tdd-bugfix [description]` | Reproduce → failing test → fix → verify workflow |
| `/dev:release-check [notes]` | Run full quality gate (fmt, clippy, tests) + summary |

---

## Key Documentation

Assumptions and contracts are documented in `docs/`:
- `docs/assumptions.md` — Fill conventions, adjusted vs unadjusted, timezone, missing bars
- `docs/schema.md` — Canonical bar schema with field types and units
- `docs/bdd-style.md` — How to write scenarios, tag conventions, step naming

---

## Documentation Sync Requirements

**IMPORTANT**: When adding new features, update BOTH documentation sources:

1. **README.md** — User-facing documentation for GitHub/web
   - Feature descriptions and usage examples
   - Keyboard shortcuts tables
   - Milestone status updates
   - Project Map section

2. **TUI Help Panel** (`crates/trendlab-tui/src/help.rs`) — In-app help for TUI users
   - Keyboard shortcuts by panel
   - Context-sensitive help content
   - Quick reference format (scannable, compact)

The Help panel is Tab 6 in the TUI, accessible via `?` or `6`. It provides:

- Global navigation shortcuts
- Panel-specific shortcuts (Data, Strategy, Sweep, Results, Chart)
- Feature explanations (Risk Profiles, YOLO Mode, Statistical Analysis)
- Vim-style navigation (j/k, gg, G, Ctrl+d/u)

When implementing a new feature with keyboard shortcuts or user-facing behavior:

1. Add the shortcut/feature to README.md
2. Add corresponding entry to the Help panel content
3. Ensure both sources use consistent terminology

---

## Roadmaps

Active development roadmaps:
- `docs/roadmap-v2-strategies.md` — V2 strategy implementation plan (Polars-native backtest engine)
- `docs/roadmap-tauri-gui.md` — Tauri GUI implementation plan (React + TradingView charts)

---

## Finding Current Plans

When starting a session or trying to understand where the project is at, check for recent plans in:

- **User plans folder**: `C:\Users\Dan\.claude\plans\` — Contains recent implementation plans and progress notes
- Look for the most recently modified `.md` files to understand current work in progress
- Plans include task breakdowns, decisions made, and next steps

---

## Repo Conventions

### Rust Style
- Rust stable, `cargo fmt` always
- `cargo clippy --all-targets --all-features -D warnings` must pass
- Prefer explicit types at API boundaries; avoid "clever" lifetimes
- Favor pure functions and immutable data; keep side effects in thin IO layers

### Data Policy
- **fixtures/** — Tiny deterministic datasets (20-200 bars), version-controlled
- **data/** — Real market data, GITIGNORED, never committed
- Raw cache: `data/raw/{provider}/...`
- Normalized: `data/parquet/{timeframe}/symbol=.../year=.../*.parquet`
- Never re-fetch if cached unless `--force` flag given

### Polars Rules
- ALWAYS use `scan_parquet` (lazy) rather than eager reads
- This enables predicate/projection pushdown and better performance
- Keep transformations deterministic and reproducible

### Backtest Model (Phase 1)
- Signal computed on bar close
- Fill convention default: next bar open (configurable)
- Fees: bps per trade (configurable)
- Slippage: optional simple bps (configurable)
- Position sizing: fixed notional or vol-target (later)
- Long-only first; add long/short after invariants proven

### Strategy Artifact (for Pine Parity)
Every "winning config" emits a `StrategyArtifact` (JSON):
- strategy_id, version
- timeframe, fill_model, fees/slippage
- indicator definitions (window lengths, ATR settings, etc.)
- entry/exit boolean rules in Pine-friendly DSL
- parameters + chosen defaults
- parity test vectors: small date window with indicator values + expected entry/exit

Schema lives in `schemas/strategy-artifact.schema.json`.

---

## Test-First Workflow (BDD)

BDD lives in `crates/trendlab-bdd/` with cucumber-rs:
- `tests/bdd.rs` — Runner with `harness = false`
- `tests/features/*.feature` — Gherkin scenarios

Rules:
- Write/extend `.feature` scenarios BEFORE implementing behavior
- Do not weaken tests to make code "pass"
- Add invariants (no lookahead, accounting identities, determinism)
- Use fixtures from `fixtures/` for deterministic, fast tests

---

## Common Commands

```bash
cargo test                                           # Run all tests (including BDD)
cargo fmt                                            # Format code
cargo clippy --all-targets --all-features -D warnings # Lint
cargo run -p trendlab-cli -- --help                  # CLI help
cargo tauri dev -c apps/trendlab-gui/src-tauri       # Run GUI in dev mode
```

---

## GUI Keyboard Shortcuts

The GUI mirrors TUI keyboard navigation for muscle-memory consistency.

### Global Navigation
| Key | Action |
|-----|--------|
| `1-6` | Direct panel access (Data, Strategy, Sweep, Results, Chart, Help) |
| `?` | Open Help panel (same as `6`) |
| `Tab` | Next panel |
| `Shift+Tab` | Previous panel |
| `Esc` | Cancel current operation / close modal |

### Vim-Style List Navigation
| Key | Action |
|-----|--------|
| `j` / `Down` | Move down in list |
| `k` / `Up` | Move up in list |
| `h` / `Left` | Collapse / navigate left |
| `l` / `Right` | Expand / navigate right |
| `Enter` | Confirm / expand / collapse |

### Selection
| Key | Action |
|-----|--------|
| `Space` | Toggle item selection |
| `a` | Select all in current context |
| `n` | Deselect all (select none) |

### Panel-Specific Actions
| Key | Panel | Action |
|-----|-------|--------|
| `f` | Data | Fetch data for selected tickers |
| `s` | Data | Enter search mode |
| `s` | Results | Cycle sort column |
| `v` | Results | Cycle view mode |
| `e` | Strategy | Toggle ensemble mode |
| `d` | Chart | Toggle drawdown overlay |
| `m` | Chart | Cycle chart mode |
| `v` | Chart | Toggle volume subplot |
| `c` | Chart | Toggle crosshair |

---

## Reference Documentation

Pine Script v6 docs are in `pine-script-docs/`:
- `language.md` — Full language reference
- `concepts/strategies.md` — Strategy() specifics
- `concepts/repainting.md` — Avoiding repainting issues
- `faq/strategies.md` — Strategy FAQ
- `error-messages.md` — Error reference

---

## Safety / Secrets

- Never commit API keys or credentials
- `data/` is gitignored — never commit market data
- Add sensitive paths to `.claude/settings.json` deny list
- Use environment variables for any API tokens
