# TrendLab Research Reports

This directory contains analysis reports from backtesting experiments.

## Reports

| Date | Report | Summary |
|------|--------|---------|
| 2025-12-29 | [YOLO Sweep Analysis](2025-12-29-yolo-sweep-analysis.md) | Initial 16-strategy sweep across 222 symbols. Supertrend, ParabolicSAR, and 52-Week High emerge as most robust. |

## Key Findings (Running)

### Best Strategies So Far
1. **Supertrend (17/2.5)** — Most robust, works on 94% of symbols
2. **ParabolicSAR** — Nearly identical performance to Supertrend
3. **52-Week High Breakout** — Simple momentum, very consistent

### Strategies to Avoid
- DMI/ADX — Too noisy, high turnover
- Keltner Channels — Inconsistent signals
- Darvas Box — Overtrading kills returns

### Patterns Observed
- Lower trade frequency correlates with better Sharpe
- ETFs outperform individual stocks (cleaner signals)
- Simple strategies beat complex ones in trending markets

## Open Questions

- [ ] How do winners perform in bear markets / drawdowns?
- [ ] Are nearby parameter values equally robust? (overfitting check)
- [ ] What's the optimal portfolio allocation across strategies?
- [ ] Can regime detection improve timing?

## Methodology Notes

- All backtests use daily bars, next-bar-open fills
- Sharpe calculated using daily returns, annualized
- "Robustness" = avg_sharpe / (1 + std_sharpe across symbols)
- YOLO mode tests all strategies × all symbols × best configs
