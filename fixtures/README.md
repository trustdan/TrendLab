# TrendLab Fixtures

This folder contains small, deterministic datasets used for testing.

## Guidelines

- **Size:** 20-200 bars maximum per fixture
- **Deterministic:** Fixed data, never fetched dynamically
- **Version controlled:** All fixtures are committed to git

## Naming Convention

```
{symbol}_{bars}_{description}.csv
```

Examples:
- `spy_100_2023.csv` — 100 bars of SPY from 2023
- `aapl_50_split.csv` — 50 bars including a stock split
- `synthetic_ma_cross.csv` — Synthetic data designed to trigger MA crossover

## CSV Format

```csv
ts,open,high,low,close,volume,symbol,timeframe
2024-01-01T00:00:00Z,380.50,382.10,379.80,381.20,50000000,SPY,1d
```

All prices are split-adjusted and dividend-adjusted.

## Creating Fixtures

1. Export a small slice from real data (then anonymize if needed)
2. Or create synthetic data with known properties
3. Document the fixture's purpose in this README
4. Ensure the fixture is used in at least one BDD scenario

## Available Fixtures

| File | Description | Bars | Use Case |
|------|-------------|------|----------|
| `synth/determinism_30.csv` | Monotonic synthetic series | 30 | Determinism + accounting invariants |
| `synth/lookahead_30.csv` | Monotonic synthetic series | 30 | No-lookahead indicator stability |
| `synth/fill_next_open.csv` | Small series with known opens | 6 | Fill-model invariant (next open) |
| `synth/costs_roundtrip.csv` | Small series with known opens | 5 | Fees + slippage cost model checks |
