# Supertrend 18/4.3 Strategy Artifact

## Overview
- **Strategy**: Supertrend
- **Config**: ATR Period=18, Multiplier=4.3
- **Rank**: #1 in YOLO sweep session
- **Date Range**: 2010-01-09 to 2025-12-29 (16.0 years)

## Performance Metrics
| Metric | Value |
|--------|-------|
| Avg Sharpe | 0.415 |
| OOS Sharpe | 0.44 |
| Min Sharpe | -0.270 |
| Max Sharpe | 1.135 |
| Hit Rate | 97.3% |
| Walk-Forward Grade | B |
| Symbols Tested | 222 |

## Best/Worst Performers
**Best Symbols**: RDDT (1.13), PLTR (0.91), CEG (0.84), PPA (0.78), VOO (0.78)
**Worst Symbols**: KHC (-0.27), DOW (-0.19), OIH (-0.07), DVN (-0.02), OXY (-0.02)

**Best Sectors**: ETF Industrials (0.69), ETF Broad Market (0.67), ETF Technology (0.60)

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
- ATR Period: 18
- Multiplier: 4.3

**Entry Rule**: Enter long when Supertrend direction changes from downtrend to uptrend (direction changes from 1 to -1)

**Exit Rule**: Exit long when Supertrend direction changes from uptrend to downtrend (direction changes from -1 to 1)

**Fill Model**: Execute trades on the next bar's open after signal

**Important**:
1. Use ta.supertrend(4.3, 18) for the indicator
2. The direction value is -1 for uptrend, 1 for downtrend
3. Use strategy.entry() and strategy.close() for position management
4. Set default_qty_type to strategy.percent_of_equity with 100% allocation

---

## File Location
`artifacts/exports/yolo-session/supertrend_18_4.3.json`
