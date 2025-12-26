# BDD Style Guide for TrendLab

This document defines conventions for writing Gherkin feature files and cucumber-rs step definitions.

---

## File Organization

```
crates/trendlab-bdd/
├── Cargo.toml
├── tests/
│   ├── bdd.rs              # Cucumber runner
│   └── features/
│       ├── backtest/       # Backtest engine scenarios
│       ├── indicators/     # Indicator calculation scenarios
│       ├── strategies/     # Strategy-specific scenarios
│       └── data/           # Data ingestion scenarios
```

---

## Scenario Naming

Use descriptive, behavior-focused names:

```gherkin
# Good
Scenario: Entry signal triggers on MA crossover
Scenario: No lookahead in indicator calculation
Scenario: Fees are deducted from trade PnL

# Bad
Scenario: Test MA crossover
Scenario: Test1
Scenario: Check fees work
```

---

## Tag Conventions

| Tag | Meaning |
|-----|---------|
| `@wip` | Work in progress — skip in CI |
| `@slow` | Slow test — may be skipped in quick runs |
| `@invariant` | Tests a core invariant (must never fail) |
| `@strategy:<id>` | Tests specific strategy (e.g., `@strategy:ma_cross`) |
| `@regression` | Regression test for a specific bug |

**Example:**
```gherkin
@invariant
Scenario: Signals cannot use future data
  Given a strategy that peeks at tomorrow's close
  Then the backtest should fail with "lookahead detected"
```

---

## Step Definition Patterns

### Given steps (setup)
```gherkin
Given a bar series from fixture "spy_2020_2023.csv"
Given a 20-day SMA indicator
Given initial capital of $100,000
Given fees of 10 basis points
```

### When steps (action)
```gherkin
When the backtest runs from 2020-01-01 to 2023-12-31
When the indicator is calculated
When a buy signal occurs at 2022-03-15
```

### Then steps (assertion)
```gherkin
Then the SMA at 2022-01-03 should be 450.23
Then the trade PnL should be -$50.00 after fees
Then no signal should reference bar T+1 data
```

---

## Invariant Scenarios

Every feature file should include invariant tests where applicable:

```gherkin
@invariant
Scenario: No lookahead in signal generation
  Given a strategy with entry rule "close > sma(20)"
  When the backtest runs
  Then every signal at time T only uses data from T or earlier

@invariant
Scenario: Equity curve accounting identity
  Given any completed backtest
  Then final_equity = initial_capital + sum(all_trade_pnl) - sum(all_fees)

@invariant
Scenario: Deterministic results
  Given the same inputs and configuration
  When the backtest runs twice
  Then both runs produce identical results
```

---

## Fixture References

Always use fixtures from `fixtures/`:

```gherkin
# Good — uses version-controlled fixture
Given a bar series from fixture "spy_100_bars.csv"

# Bad — depends on external data
Given a bar series for SPY from Yahoo Finance
```

---

## Step Definition Implementation

In Rust, use clear struct names for World:

```rust
use cucumber::{given, then, when, World};

#[derive(Debug, Default, World)]
pub struct BacktestWorld {
    bars: Option<Vec<Bar>>,
    strategy: Option<Box<dyn Strategy>>,
    result: Option<BacktestResult>,
}

#[given(regex = r"^a bar series from fixture (.+)$")]
async fn load_fixture(world: &mut BacktestWorld, fixture: String) {
    let path = format!("../../fixtures/{}", fixture);
    world.bars = Some(load_bars_from_csv(&path).unwrap());
}
```

---

## Common Pitfalls

1. **Vague assertions** — "Then it should work" is not testable
2. **External dependencies** — Don't fetch from Yahoo in tests
3. **Non-deterministic data** — Use fixed fixtures, not random data
4. **Missing invariants** — Every feature needs at least one invariant test
5. **Skipped edge cases** — Test empty data, single bar, first/last bar

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2024-12-26 | Initial BDD style guide |
