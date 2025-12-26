# TrendLab

Research-grade trend-following backtesting lab built in Rust with Polars. TrendLab is designed for running large parameter sweeps across many symbols, with robust statistical evaluation and deterministic, testable behavior. It outputs strategy artifacts that can be used to generate parity-checked Pine Scripts for TradingView.

## Quick Start

```bash
# Build the project
cargo build --release

# Run tests
cargo test

# See CLI help
cargo run -p trendlab-cli -- --help
```

## Commands

### Fetch Market Data (Planned)

```bash
# Fetch daily bars from Yahoo Finance
trendlab data refresh-yahoo --tickers SPY,QQQ,IWM --start 2020-01-01 --end 2024-12-31

# Check data status
trendlab data status --ticker SPY
```

### Run Parameter Sweep (Planned)

```bash
# Run a sweep for MA crossover strategy
trendlab sweep --strategy ma_cross --universe SPY,QQQ --start 2020-01-01 --end 2023-12-31
```

### Generate Reports (Planned)

```bash
# Generate summary report
trendlab report summary --run-id 20241226_001

# Export metrics to CSV
trendlab report export --run-id 20241226_001 --output results.csv
```

### Export Strategy Artifacts (Planned)

```bash
# Export artifact for Pine Script generation
trendlab artifact export --run-id 20241226_001 --config-id best_sharpe
```

## Project Structure

```
TrendLab/
├── crates/
│   ├── trendlab-core/    # Domain types, strategies, metrics
│   ├── trendlab-cli/     # CLI interface
│   └── trendlab-bdd/     # BDD tests with cucumber-rs
├── docs/                 # Assumptions, schemas, BDD style guide
├── fixtures/             # Deterministic test datasets
├── data/                 # Market data (gitignored)
├── schemas/              # JSON schemas for artifacts
├── configs/              # Sweep grids and templates
├── reports/              # Generated reports
└── artifacts/            # Exported strategy artifacts
```

## Development

```bash
# Format code
cargo fmt

# Run linter
cargo clippy --all-targets --all-features -D warnings

# Run all tests including BDD
cargo test
```

## Documentation

- [Assumptions](docs/assumptions.md) - Fill conventions, timezone, missing bars policy
- [Schema](docs/schema.md) - Bar schema, Parquet layout, metrics definitions
- [BDD Style Guide](docs/bdd-style.md) - How to write Gherkin scenarios

## License

MIT
