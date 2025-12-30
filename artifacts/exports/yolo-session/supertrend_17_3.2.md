# Supertrend 17/3.2 Strategy Artifact

## Overview
- **Strategy**: Supertrend
- **Config**: ATR Period=17, Multiplier=3.2
- **Rank**: #1 in YOLO sweep session
- **Date Range**: 2010-01-09 to 2025-12-29 (16.0 years)

## Performance Metrics
| Metric | Value |
|--------|-------|
| Avg Sharpe | 0.413 |
| OOS Sharpe | 0.44 |
| Min Sharpe | -0.271 |
| Hit Rate | 95.9% |
| FDR p-value | 0.000** |
| Walk-Forward Grade | B |
| Symbols Tested | 222 |

## Strategy Logic

### Indicator
The Supertrend indicator calculates upper and lower bands based on ATR:
- Basic price = (High + Low) / 2
- Upper Band = Basic + (Multiplier * ATR)
- Lower Band = Basic - (Multiplier * ATR)

The bands trail price and flip when price crosses them.

### Entry Rule
Enter long when Supertrend flips to uptrend (close crosses above the supertrend line).

### Exit Rule
Exit long when Supertrend flips to downtrend (close crosses below the supertrend line).

## Pine Script Generation Prompt

Use this prompt with an LLM to generate equivalent Pine Script v6:

---

Generate a Pine Script v6 strategy that replicates the following:

**Strategy**: Supertrend
**Parameters**:
- ATR Period: 17
- Multiplier: 3.2

**Entry Rule**: Enter long when Supertrend direction changes from downtrend to uptrend (direction changes from 1 to -1)

**Exit Rule**: Exit long when Supertrend direction changes from uptrend to downtrend (direction changes from -1 to 1)

**Fill Model**: Execute trades on the next bar's open after signal

**Important**:
1. Use ta.supertrend(3.2, 17) for the indicator
2. The direction value is -1 for uptrend, 1 for downtrend
3. Use strategy.entry() and strategy.close() for position management
4. Set default_qty_type to strategy.percent_of_equity with 100% allocation

---

## File Location
`artifacts/exports/yolo-session/supertrend_17_3.2.json`
