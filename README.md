# TrendLab

TrendLab is a **research-grade trend-following backtesting lab** (not a live-trading engine).
It exists to help you answer, repeatedly and defensibly:

> Which trend-following approach works, under what conditions, and how do I know I’m not fooling myself?

The project optimizes for:

- **Correctness and reproducibility** (invariants + deterministic outputs)
- **Fast experimentation** (sweep-first workflows across strategy families)
- **Verifiable results** via **StrategyArtifact → Pine parity** (TradingView as an external reference implementation)

## Ethos (what we care about)

- **Correctness over cleverness**: no “probably right” backtests.
- **BDD-first**: a `.feature` file is the contract; implementation follows.
- **Explicit assumptions**: fill model, costs, data adjustments, missing bars policy are spelled out and tested.
- **Parity is correctness, not polish**: Pine parity is part of validation, not a “nice to have.”

## How it works (high level)

TrendLab’s “happy path” is:

1. **Ingest & cache market data** (planned: Yahoo daily OHLCV → Parquet cache)
2. **Compute indicators and signals** (strict time alignment; no lookahead)
3. **Simulate fills and accounting** (default: signal-on-close → fill-next-open; explicit fees/slippage)
4. **Run sweeps** across parameter grids and universes (planned)
5. **Rank + report** with robustness diagnostics (planned)
6. **Export StrategyArtifact** JSON (planned)
7. **Verify in TradingView** via Pine parity vectors / exported strategy results (planned)

## Architecture (workspace crates)

- **`trendlab-core`**: pure domain logic (bars, indicators, strategies, backtest kernel, metrics). **No IO.**
- **`trendlab-cli`**: orchestration + IO (data refresh, sweep runs, report export, artifact export).
- **`trendlab-bdd`**: cucumber runner, step definitions, and fixtures that lock invariants early.

For more detail, see `docs/PROJECT_OVERVIEW.md` and `docs/architecture.md`.

## Current status (what’s real today)

Milestone 0 (“Foundation”) is complete:

- Cucumber BDD suite is wired and runs under `cargo test`
- Deterministic fixtures live in `fixtures/synth/`
- A minimal vertical slice exists in core:
  - SMA indicator (no-lookahead by construction)
  - Minimal backtest kernel: next-open fill + fees/slippage + accounting identity
- One-command quality gate exists (fmt + clippy + tests)

Milestone 1 (“Data Layer”) is next: provider ingestion + Parquet cache + data-quality reports + BDD scenarios.

## Quick start

```bash
# Build
cargo build

# Run tests (includes BDD)
cargo test

# CLI help
cargo run -p trendlab-cli -- --help

# TUI
cargo run -p trendlab-tui --bin trendlab-tui
```

## One-command quality gate (“press start”)

**Windows:**

```powershell
.\scripts\verify.ps1
```

**macOS/Linux:**

```bash
bash scripts/verify.sh
```

## Development workflow (the loop)

1. Add/extend a `.feature` file in `crates/trendlab-bdd/tests/features/`
2. Add/extend a tiny deterministic fixture in `fixtures/synth/` (20–200 bars)
3. Run `scripts/verify.*` and watch it fail (red)
4. Implement the minimum change in `trendlab-core` / `trendlab-cli`
5. Re-run `scripts/verify.*` until green

## Planned CLI commands (roadmap)

These are the intended UX, but most are not implemented yet:

```bash
# Fetch daily bars from Yahoo Finance
trendlab data refresh-yahoo --tickers SPY,QQQ,IWM --start 2020-01-01 --end 2024-12-31

# Run a sweep (example)
trendlab sweep --strategy donchian --universe SPY,QQQ --start 2020-01-01 --end 2023-12-31

# Export a StrategyArtifact for Pine parity
trendlab artifact export --run-id 20241226_001 --config-id best_sharpe
```

## Project structure

```text
TrendLab/
├── crates/
│   ├── trendlab-core/    # Domain types, indicators, sim kernel, metrics (no IO)
│   ├── trendlab-cli/     # CLI + orchestration + IO
│   └── trendlab-bdd/     # BDD runner + step defs + feature files
├── docs/                 # Assumptions, schemas, architecture, style
├── fixtures/             # Deterministic test datasets (tiny)
├── data/                 # Market data cache (gitignored)
├── schemas/              # JSON schemas for artifacts/configs
├── configs/              # Sweep grids and templates
├── reports/              # Generated reports and run outputs
└── artifacts/            # Exported StrategyArtifact JSON
```

## Documentation

- **Assumptions**: `docs/assumptions.md` (fill conventions, timezone, missing bars)
- **Canonical schema**: `docs/schema.md` (bars, partitions, units)
- **BDD style**: `docs/bdd-style.md`
- **Architecture**: `docs/architecture.md`
- **Roadmap**: `.claude/plans/development-plan.md`

## License

MIT
