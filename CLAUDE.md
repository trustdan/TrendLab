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
├── crates/
│   ├── trendlab-core/    # Domain types, strategies, metrics
│   ├── trendlab-cli/     # CLI interface
│   ├── trendlab-tui/     # Terminal UI (ratatui)
│   ├── trendlab-gui/     # Desktop GUI (Tauri + React)
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
| `/pine:export-artifact [run_id] [config_id]` | Export StrategyArtifact JSON for Pine generation |
| `/pine:parity-check [artifact.json]` | Create parity checklist + test vectors for Pine validation |

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

## Roadmaps

Active development roadmaps:
- `docs/roadmap-v2-strategies.md` — V2 strategy implementation plan (Polars-native backtest engine)
- `docs/roadmap-tauri-gui.md` — Tauri GUI implementation plan (React + TradingView charts)

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
```

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
