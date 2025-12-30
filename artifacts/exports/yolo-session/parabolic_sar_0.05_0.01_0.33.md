# Parabolic SAR 0.05/0.01/0.33 Strategy Artifact

## Overview
- **Strategy**: Parabolic SAR (Stop and Reverse)
- **Config**: AF Start=0.05, AF Step=0.01, AF Max=0.33
- **Rank**: #2 in YOLO sweep session
- **Date Range**: 2010-01-09 to 2025-12-29 (16.0 years)

## Performance Metrics
| Metric | Value |
|--------|-------|
| Avg Sharpe | 0.413 |
| OOS Sharpe | 0.44 |
| Min Sharpe | -0.262 |
| Hit Rate | 96.4% |
| FDR p-value | 0.000** |
| Walk-Forward Grade | B |
| Symbols Tested | 222 |

## Strategy Logic

### Indicator
Parabolic SAR trails price and flips when price crosses the SAR value:
- **AF Start**: Initial acceleration factor (0.05)
- **AF Step**: Increment per new extreme (0.01)
- **AF Max**: Maximum acceleration factor (0.33)

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
- AF Start: 0.05
- AF Step: 0.01
- AF Max: 0.33

**Entry Rule**: Enter long when close crosses above the Parabolic SAR value (SAR flips below price)

**Exit Rule**: Exit long when close crosses below the Parabolic SAR value (SAR flips above price)

**Fill Model**: Execute trades on the next bar's open after signal

**Important**:
1. Use ta.sar(0.05, 0.01, 0.33) for the indicator
2. Use ta.crossover(close, sar) for entry detection
3. Use ta.crossunder(close, sar) for exit detection
4. Use strategy.entry() and strategy.close() for position management
5. Set default_qty_type to strategy.percent_of_equity with 100% allocation

---

## File Location
`artifacts/exports/yolo-session/parabolic_sar_0.05_0.01_0.33.json`
