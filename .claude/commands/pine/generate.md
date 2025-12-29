---
description: Generate Pine Script v6 from strategy config (finds/creates artifact automatically)
argument-hint: [strategy] [config] or from TUI screenshot
allowed-tools: Bash(ls:*), Bash(cat:*), Read, Glob, Grep, Write, Edit
---

Generate a TradingView Pine Script v6 for the specified strategy configuration.

## Inputs

- strategy: $1 (e.g., "52wk_high", "donchian", "supertrend")
- config: $2 (e.g., "80_70_59", "20_10")
- OR: Parse from TUI screenshot/user description

## Reference

- @.claude/agents/pine-artifact-writer.md
- @schemas/strategy-artifact.schema.json
- @pine-script-docs/concepts/strategies.md
- @crates/trendlab-core/src/strategy_v2.rs (for strategy logic)

## Your tasks

1. **Identify Strategy**: Parse strategy type and config from user input or screenshot
   - Strategy types: `52wk_high`, `donchian`, `ma_crossover`, `supertrend`, `tsmom`, etc.
   - Config format varies by strategy (e.g., `80_70_59` for 52wk_high = period/entry%/exit%)

2. **Find Artifact**: Search `artifacts/exports/` for matching `<strategy>_<config>.json`
   ```bash
   ls artifacts/exports/*/<strategy>*<config>*.json
   ```

3. **Create if Missing**: If no artifact found, create one:
   - Look up strategy parameters in `crates/trendlab-core/src/strategy_v2.rs`
   - Create artifact JSON in `artifacts/exports/manual/<strategy>_<config>.json`
   - Include: indicators with pine_expr, rules with pine_condition, parameters

4. **Generate Pine Script**: Build complete Pine Script v6 with:
   - `//@version=6` header
   - `strategy()` declaration with proper settings (overlay, margin, commission, slippage)
   - Input parameters matching artifact (with `input.int()`, `input.float()`)
   - Indicator calculations from artifact's `pine_expr`
   - Entry/exit logic from artifact's `pine_condition`
   - `strategy.entry()` and `strategy.close()` calls
   - Plotting for visual feedback (indicators, thresholds, entry/exit markers)
   - Background color when in position

5. **Save Output**: Write to `pine-scripts/strategies/<strategy>/<config>.pine`
   - Create directory if needed
   - Use `.pine` extension for TradingView compatibility

6. **Update Index**: Add entry to `pine-scripts/README.md`
   - Add row to Scripts table with strategy, config, date, file link

7. **Report**: Display file path and summary to user

## Strategy Config Formats

| Strategy | Config Format | Example |
|----------|---------------|---------|
| 52wk_high | period_entry%_exit% | 80_70_59 |
| donchian | entry_exit | 20_10 |
| ma_crossover | fast_slow_type | 50_200_sma |
| supertrend | atr_mult | 10_2.0 |
| tsmom | lookback | 252 |
| dmi_adx | di_adx_threshold | 14_14_25 |
| bollinger_squeeze | period_std_squeeze | 20_2.0_0.05 |

## Pine Script Template

```pine
//@version=6
strategy("[Strategy Name] ([Config])", overlay=true, margin_long=100, margin_short=100,
         default_qty_type=strategy.percent_of_equity, default_qty_value=100,
         commission_type=strategy.commission.percent, commission_value=0.1,
         slippage=1)

// === INPUTS ===
// [Input declarations from artifact parameters]

// === INDICATORS ===
// [Indicator calculations from artifact pine_expr]

// === SIGNALS ===
// [Entry/exit conditions from artifact pine_condition]

// === STRATEGY EXECUTION ===
if entryCondition and strategy.position_size == 0
    strategy.entry("Long", strategy.long)

if exitCondition and strategy.position_size > 0
    strategy.close("Long")

// === PLOTTING ===
// [Visual feedback - indicators, thresholds, markers]
```

## Constraints

- Always use Pine Script v6 syntax
- Long-only strategies by default (matches TrendLab backtest model)
- Commission: 0.1% (10 bps) per side
- Slippage: 1 tick
- Use `strategy.position_size == 0` for flat position check
- Use `strategy.position_size > 0` for long position check
