# Parabolic SAR 0.01/0.08/0.37 Strategy Artifact

## Overview
- **Strategy**: Parabolic SAR (Stop and Reverse)
- **Config**: AF Start=0.01, AF Step=0.08, AF Max=0.37
- **Rank**: #2 in YOLO sweep session
- **Date Range**: 2010-01-09 to 2025-12-29 (16.0 years)

## Performance Metrics
| Metric | Value |
|--------|-------|
| Avg Sharpe | 0.413 |
| OOS Sharpe | 0.44 |
| Min Sharpe | -0.262 |
| Max Sharpe | 1.124 |
| Hit Rate | 96.8% |
| Walk-Forward Grade | B |
| Symbols Tested | 222 |

## Best/Worst Performers
**Best Symbols**: RDDT (1.12), PLTR (0.91), CEG (0.85), VOO (0.78), PPA (0.78)
**Worst Symbols**: KHC (-0.26), DOW (-0.18), OIH (-0.07), DVN (-0.03), OXY (-0.02)

**Best Sectors**: ETF Industrials (0.69), ETF Broad Market (0.68), ETF Technology (0.60)

## Strategy Logic

### Indicator
Parabolic SAR trails price and flips when price crosses the SAR value:
- **AF Start**: Initial acceleration factor (0.01)
- **AF Step**: Increment per new extreme (0.08)
- **AF Max**: Maximum acceleration factor (0.37)

### Entry Rule
Enter long when SAR flips below price (uptrend begins) - close crosses above SAR.

### Exit Rule
Exit long when SAR flips above price (downtrend begins) - close crosses below SAR.

## Pine Script Generation Prompt

Use this prompt with an LLM to generate equivalent Pine Script v6:

---

Generate a Pine Script v6 strategy that replicates the following:

**Strategy**: Parabolic SAR
**Parameters**:
- AF Start: 0.01
- AF Step: 0.08
- AF Max: 0.37

**Entry Rule**: Enter long when close crosses above the Parabolic SAR value (SAR flips below price)

**Exit Rule**: Exit long when close crosses below the Parabolic SAR value (SAR flips above price)

**Fill Model**: Execute trades on the next bar's open after signal

**Important**:
1. Use ta.sar(0.01, 0.08, 0.37) for the indicator
2. Use ta.crossover(close, sar) for entry detection
3. Use ta.crossunder(close, sar) for exit detection
4. Use strategy.entry() and strategy.close() for position management
5. Set default_qty_type to strategy.percent_of_equity with 100% allocation

---

## File Location
`artifacts/exports/yolo-session/parabolic_sar_0.01_0.08_0.37.json`
