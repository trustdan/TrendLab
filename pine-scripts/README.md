# Generated Pine Scripts

Pine Scripts generated from TrendLab strategy artifacts for use in TradingView.

## Scripts

| Strategy | Config | Generated | Performance | File |
|----------|--------|-----------|-------------|------|
| Supertrend | ATR=18, Mult=4.3 | 2025-12-29 | Sharpe 0.415, OOS 0.44, Hit 97.3% | [supertrend/18_4.3.pine](strategies/supertrend/18_4.3.pine) |
| Parabolic SAR | 0.01/0.08/0.37 | 2025-12-29 | Sharpe 0.413, OOS 0.44, Hit 96.8% | [parabolic_sar/0.01_0.08_0.37.pine](strategies/parabolic_sar/0.01_0.08_0.37.pine) |
| 52-Week High | 115/70%/52% | 2025-12-29 | Sharpe 0.403, OOS 0.36, Hit 95.9% | [52wk_high/115_70_52.pine](strategies/52wk_high/115_70_52.pine) |
| Supertrend | ATR=17, Mult=3.2 | 2025-12-29 | Sharpe 0.413, OOS 0.44, Hit 95.9% | [supertrend/17_3.2.pine](strategies/supertrend/17_3.2.pine) |
| Parabolic SAR | 0.05/0.01/0.33 | 2025-12-29 | Sharpe 0.413, OOS 0.44, Hit 96.4% | [parabolic_sar/0.05_0.01_0.33.pine](strategies/parabolic_sar/0.05_0.01_0.33.pine) |
| 52-Week High | 80/70%/59% | 2025-12-29 | Sharpe 0.583, Hit 94.1% | [52wk_high/80_70_59.pine](strategies/52wk_high/80_70_59.pine) |

## Usage

1. Open TradingView and go to Pine Editor
2. Copy the contents of a `.pine` file
3. Paste into Pine Editor and click "Add to chart"
4. Apply to **daily** chart for parity with TrendLab backtests
5. Compare results in Strategy Tester tab

## Strategy Types

| Strategy ID | Description | Config Format |
|-------------|-------------|---------------|
| `52wk_high` | 52-Week High breakout | `period_entry%_exit%` |
| `donchian` | Donchian channel breakout | `entry_exit` |
| `ma_crossover` | Moving average crossover | `fast_slow_type` |
| `supertrend` | Supertrend indicator | `atr_mult` |
| `tsmom` | Time-series momentum | `lookback` |
| `dmi_adx` | DMI/ADX trend strength | `di_adx_threshold` |
| `bollinger_squeeze` | Bollinger Band squeeze | `period_std_squeeze` |

## Parity Notes

These scripts are designed to match TrendLab backtest behavior:

- **Fill Model**: Orders execute on next bar open (TradingView default)
- **Commission**: 0.1% (10 bps) per side
- **Slippage**: 1 tick
- **Position Sizing**: 100% of equity per trade
- **Direction**: Long-only by default

For exact parity validation, use the parity test vectors in the corresponding artifact JSON.

## Directory Structure

```
pine-scripts/
├── README.md           # This file
└── strategies/
    ├── 52wk_high/
    │   └── 80_70_59.pine
    ├── donchian/
    │   └── 20_10.pine
    └── ...
```
