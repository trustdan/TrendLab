# TrendLab Architecture

## Overview

TrendLab is a research-grade backtesting lab for trend-following strategies. It prioritizes correctness, reproducibility, and speed of experimentation over live-trading concerns.

## Module Boundaries

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              trendlab-cli                                │
│                         (CLI interface, orchestration)                   │
└─────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                             trendlab-core                                │
├─────────────┬─────────────┬─────────────┬─────────────┬─────────────────┤
│    Data     │   Signals   │    Fills    │  Accounting │     Metrics     │
│  (bar.rs)   │(strategy.rs)│ (fills.rs)  │ (equity.rs) │  (metrics.rs)   │
└─────────────┴─────────────┴─────────────┴─────────────┴─────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                            Strategy Artifact                             │
│                    (JSON export for Pine generation)                     │
└─────────────────────────────────────────────────────────────────────────┘
```

## Data Flow

```
1. Data Layer (bar.rs)
   - Load bars from Parquet via Polars scan_parquet
   - Validate schema, handle missing bars, apply adjustments
   - Output: LazyFrame of OHLCV bars

2. Signal Layer (strategy.rs)
   - Compute indicators (MA, ATR, breakout levels)
   - Generate entry/exit signals based on indicator state
   - NO lookahead: signals computed on bar[i] use only data[0..=i]
   - Output: LazyFrame with signal columns

3. Fill Layer (fills.rs)
   - Convert signals to fills using fill model (next-bar open)
   - Apply slippage model if configured
   - Output: Fill events with timestamps and prices

4. Accounting Layer (equity.rs)
   - Track positions, cash, and equity curve
   - Apply transaction costs (bps per trade)
   - Output: Position history, equity series

5. Metrics Layer (metrics.rs)
   - Compute performance metrics (CAGR, Sharpe, drawdown)
   - Aggregate across parameter sweep
   - Output: Metrics table per config

6. Artifact Layer (artifact.rs)
   - Package winning config into StrategyArtifact JSON
   - Include indicator definitions, rules, test vectors
   - Output: JSON file for Pine generation
```

## Hard Invariants

These invariants MUST hold at all times. Tests should verify them.

### No Lookahead

```
Signal[i] = f(Bar[0], Bar[1], ..., Bar[i])
```

A signal on bar `i` can only use information available up to and including bar `i`. This is enforced by:
- Using Polars window functions with proper boundaries
- BDD scenarios that check signal timing against known data
- Never using `.shift(-n)` (forward shift) in signal logic

### Determinism

```
Given identical inputs (data, config, random seed),
outputs (signals, fills, metrics) MUST be identical.
```

Enforced by:
- No floating-point non-determinism (use ordered operations)
- Explicit random seeds where randomness is needed
- BDD scenarios with golden outputs

### Accounting Identity

```
Cash + PositionValue = Equity (at all times)
Equity[t] = Equity[t-1] + PnL[t] - Costs[t]
```

Enforced by:
- Explicit accounting in equity tracker
- BDD scenarios that verify cash + position = equity

### Fill Model Consistency

```
If FillModel = NextBarOpen:
  Signal[i] triggers at Open[i+1]
  Fill price = Open[i+1] (+ slippage if configured)
```

The fill model is configurable but must be applied consistently.

## Crate Responsibilities

### trendlab-core

- Domain types: `Bar`, `Signal`, `Fill`, `Position`, `Metric`
- Strategy trait and implementations
- Metrics calculations
- Error types
- NO I/O (pure computation)

### trendlab-cli

- CLI argument parsing (clap)
- Orchestration of data fetch, sweep, report
- File I/O (Parquet, JSON, CSV)
- Configuration loading

### trendlab-bdd

- Cucumber-rs runner
- Step definitions
- Feature files for all invariants
- Fixture loading

## Key Design Decisions

See [ADR log](adr/) for detailed reasoning on:
- ADR-0001: Fill model choice (next-bar open default)
- ADR-0002: Adjustment policy (adjusted close for signals)
- ADR-0003: Missing bar handling (forward-fill or skip)
- ADR-0004: Timezone conventions (exchange local or UTC)

## Performance Considerations

- Use `scan_parquet` (lazy) for all data loading
- Predicate pushdown for date range filtering
- Projection pushdown to load only needed columns
- Avoid `.collect()` until final output
- Parameter sweeps are embarrassingly parallel (rayon)

## Extension Points

### Adding a New Strategy

1. Define strategy struct implementing `Strategy` trait
2. Add BDD scenarios in `features/strategies/{name}.feature`
3. Register in strategy factory
4. Add to sweep config schema

### Adding a New Metric

1. Define calculation in `metrics.rs`
2. Add to `Metrics` struct
3. Add BDD scenario for known values
4. Update schema documentation

### Adding a New Data Provider

1. Implement provider module in `trendlab-cli`
2. Normalize to canonical bar schema
3. Add caching layer
4. Add data quality checks
