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

TrendLab's "happy path" is:

1. **Ingest & cache market data** ✅ Yahoo daily OHLCV → Parquet cache
2. **Compute indicators and signals** ✅ Strict time alignment; no lookahead
3. **Simulate fills and accounting** ✅ Signal-on-close → fill-next-open; explicit fees/slippage
4. **Run sweeps** ✅ Parallel parameter sweeps across universes via Polars + Rayon
5. **Rank + report** ✅ Full metrics suite with HTML/CSV export
6. **Export StrategyArtifact** ✅ JSON schema for Pine generation
7. **Verify in TradingView** via Pine parity vectors (in progress)

## Architecture (workspace crates)

- **`trendlab-core`**: pure domain logic (bars, indicators, strategies, backtest kernel, metrics). **No IO.**
- **`trendlab-cli`**: orchestration + IO (data refresh, sweep runs, report export, artifact export).
- **`trendlab-bdd`**: cucumber runner, step definitions, and fixtures that lock invariants early.

For more detail, see `docs/PROJECT_OVERVIEW.md` and `docs/architecture.md`.

## Current status (what's real today)

**Milestone 0 ("Foundation")** ✅ Complete:

- Cucumber BDD suite wired and running under `cargo test`
- Deterministic fixtures in `fixtures/synth/`
- Minimal vertical slice: SMA indicator, backtest kernel with next-open fill + fees/slippage

**Milestone 1 ("Data Layer")** ✅ Complete:

- Yahoo Finance data provider with automatic caching
- Parquet storage for normalized bar data
- Data quality validation and missing bar handling

**Milestone 2 ("Polars Integration")** ✅ Complete:

- Dual-backend architecture: Polars-native (vectorized) and sequential
- 3 strategy families fully ported to Polars: Donchian/Turtle, MA Crossover, TSMOM
- Unified sweep runner with parallel execution via Rayon
- Full TUI integration with Polars-native sweeps

See [Backtest Architecture](#backtest-architecture) below for details on the dual-backend design.

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

## TUI Features

The TUI provides an interactive terminal interface for backtesting exploration.

### Panels

| Panel | Hotkey | Description |
|-------|--------|-------------|
| Data | `1` | Browse universe sectors, select tickers, fetch market data |
| Strategy | `2` | Choose strategy type and configure parameters |
| Sweep | `3` | Run parameter sweeps across selected tickers |
| Results | `4` | View sweep results ranked by metrics |
| Chart | `5` | Visualize equity curves with legends |

### Full-Auto Mode

On startup, choose between **Manual** and **Full-Auto** modes:

- **Manual**: Use panels to pick tickers, configure strategy, then run sweeps
- **Full-Auto**: Automatically selects all universe tickers, runs sweeps, and displays results

### Sweep Depth Presets

Full-Auto mode includes research-backed parameter sweep depths:

| Depth | Configs | Description |
|-------|---------|-------------|
| **Quick** | ~11 | Core classics only (Turtle 20/55, Golden Cross 50/200) |
| **Standard** | ~40 | Balanced coverage - good for initial exploration |
| **Comprehensive** | ~88 | Extended ranges + fine granularity for thorough analysis |

Parameter ranges are based on:
- **Donchian/Turtle**: Classic Turtle Trading rules (20/10 for S1, 55/20 for S2)
- **MA Crossover**: Golden Cross variants (50/200 SMA, with EMA options)
- **TSMOM**: Academic momentum research (63, 126, 252 day lookbacks)

### Startup Modal Controls

| Key | Action |
|-----|--------|
| `Left`/`Right` | Switch between Manual and Full-Auto modes |
| `Up`/`Down` | Select strategy (in Full-Auto mode) |
| `[`/`]` | Cycle sweep depth (Quick/Standard/Comprehensive) |
| `Enter` | Start with selected options |
| `Esc` | Dismiss modal (enters Manual mode) |

### Chart Views

The Chart panel supports multiple view modes (press `m` to cycle):

- **Single**: Individual equity curve
- **Multi-Ticker**: Overlay best configs per ticker
- **Portfolio**: Aggregated portfolio equity
- **Strategy Comparison**: Compare strategies with color-coded legend
- **Per-Ticker Best**: Each ticker's best strategy result

## Backtest Architecture

TrendLab uses a **dual-backend architecture** for backtesting:

### Polars Backend (Default)

The Polars backend uses vectorized DataFrame operations for high performance:

- **Vectorized indicators**: Rolling windows, exponential moving averages computed column-wise
- **Parallel sweeps**: Parameter combinations processed in parallel via Rayon
- **Memory efficient**: Lazy evaluation with predicate pushdown
- **StrategyV2 trait**: Polars-native strategy implementations

```rust
// StrategyV2 trait for Polars-native strategies
pub trait StrategyV2: Send + Sync {
    fn name(&self) -> &str;
    fn compute_signals(&self, df: &DataFrame) -> Result<DataFrame>;
}
```

Implemented strategies:

| Strategy | Description | Key Parameters |
|----------|-------------|----------------|
| `DonchianBreakoutV2` | Channel breakout (Turtle-style) | entry_lookback, exit_lookback |
| `MACrossoverV2` | Moving average crossover | fast_period, slow_period, ma_type |
| `TsmomV2` | Time-series momentum | lookback_days |

### Sequential Backend (Fallback)

The sequential backend processes bars one-at-a-time for debugging and validation:

- **Bar-by-bar simulation**: Explicit order of operations
- **Easier debugging**: Step through each bar's logic
- **Reference implementation**: Used to validate Polars results

Use `--sequential` flag with CLI commands to use this backend.

### CLI Usage

```bash
# Run sweep with Polars (default, faster)
cargo run -p trendlab-cli -- sweep --strategy donchian --universe SPY,QQQ

# Run sweep with sequential backend (for debugging)
cargo run -p trendlab-cli -- sweep --strategy donchian --universe SPY,QQQ --sequential
```

### Performance Characteristics

| Backend | Sweep Speed | Memory | Use Case |
|---------|-------------|--------|----------|
| Polars | ~10-50x faster | Lower (lazy eval) | Production sweeps |
| Sequential | Baseline | Higher | Debugging, validation |

## Strategies

TrendLab implements three classic trend-following strategy families:

### Donchian Breakout / Turtle System

Channel breakout strategy based on the Turtle Trading rules:

- **Entry**: Long when price breaks above N-day high
- **Exit**: Exit when price breaks below M-day low
- **Presets**: Turtle S1 (20/10), Turtle S2 (55/20)

### MA Crossover

Moving average crossover with configurable MA types:

- **Entry**: Long when fast MA crosses above slow MA
- **Exit**: Exit when fast MA crosses below slow MA
- **MA Types**: SMA (Simple), EMA (Exponential)
- **Presets**: Golden Cross (50/200 SMA)

### TSMOM (Time-Series Momentum)

Academic momentum strategy based on Moskowitz et al. research:

- **Signal**: Long if return over lookback period is positive
- **Lookbacks**: 63 (quarterly), 126 (semi-annual), 252 (annual)

## Indicators

All indicators are pure functions with no lookahead:

| Indicator | Description | Module |
|-----------|-------------|--------|
| Donchian Channel | Highest high / lowest low over N bars | `indicators.rs` |
| SMA | Simple moving average | `indicators.rs` |
| EMA | Exponential moving average | `indicators.rs` |
| ATR | Average true range (standard) | `indicators.rs` |
| ATR Wilder | Wilder's smoothed ATR | `indicators.rs` |

## Metrics

Full performance metrics computed for every backtest:

| Metric | Description |
|--------|-------------|
| Total Return | End equity / initial equity - 1 |
| CAGR | Compound annual growth rate |
| Sharpe | Annualized risk-adjusted return (252 days) |
| Sortino | Downside-only Sharpe variant |
| Max Drawdown | Largest peak-to-trough decline |
| Calmar | CAGR / Max Drawdown |
| Win Rate | Winning trades / total trades |
| Profit Factor | Gross profit / gross loss |
| Turnover | Annual trading volume as multiple of capital |

## Position Sizing

TrendLab supports multiple position sizing approaches:

| Sizer | Description |
|-------|-------------|
| Fixed | Constant number of units per trade |
| Volatility | Units = risk budget / (ATR × price) — Turtle-style |

**Pyramiding** is also supported: add to winning positions up to a configurable maximum.

## Universe Configuration

Tickers are organized by sector in `configs/universe.toml`:

- **11 equity sectors**: Technology, Healthcare, Financials, etc.
- **12 ETF categories**: Broad market, sector ETFs, commodities
- **150+ tickers** ready to use

Load universe in code:

```rust
let universe = Universe::load("configs/universe.toml")?;
for sector in &universe.sectors {
    println!("{}: {} tickers", sector.name, sector.len());
}
```

## BDD Test Coverage

15 Gherkin feature files covering:

| Feature | Tests |
|---------|-------|
| `strategy_donchian.feature` | Donchian breakout entry/exit logic |
| `strategy_turtle_s1.feature` | Turtle System 1 rules |
| `strategy_turtle_s2.feature` | Turtle System 2 rules |
| `strategy_ma_crossover.feature` | MA crossover logic |
| `strategy_tsmom.feature` | Time-series momentum |
| `indicators.feature` | SMA, EMA, Donchian, ATR |
| `invariants.feature` | No-lookahead, accounting identity |
| `costs.feature` | Fees and slippage handling |
| `volatility_sizing.feature` | ATR-based sizing |
| `pyramiding.feature` | Position pyramiding |
| `sweep.feature` | Parameter sweep infrastructure |
| `data_quality.feature` | Data validation |
| `artifact_parity.feature` | Pine export format |

## One-command quality gate ("press start")

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

## CLI Commands

All major commands are implemented and working:

```bash
# Data management
trendlab data refresh-yahoo --tickers SPY,QQQ,IWM --start 2020-01-01 --end 2024-12-31
trendlab data refresh-yahoo --tickers SPY --start 2020-01-01 --end 2024-12-31 --force
trendlab data status --ticker SPY

# Run a single backtest
trendlab run --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31

# Run parameter sweep (Polars by default)
trendlab sweep --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31
trendlab sweep --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31 --sequential
trendlab sweep --strategy donchian --ticker SPY --start 2020-01-01 --end 2023-12-31 \
    --grid "entry:10,20,30,40;exit:5,10,15" --top-n 10

# Reporting
trendlab report list
trendlab report summary --run-id sweep_001 --top-n 10
trendlab report html --run-id sweep_001 --open
trendlab report export --run-id sweep_001 --output results.csv

# Strategy artifacts (for Pine parity)
trendlab artifact export --run-id sweep_001 --config-id best_sharpe
trendlab artifact validate --path artifact.json
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
