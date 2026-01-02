# YOLO Run Analysis - 2026-01-01

**Run Type**: Full YOLO mode (all strategies × all symbols)
**Symbols**: 98 (S&P 100 universe)
**Date Range**: TBD

---

## Strategy Performance (by Median Sharpe)

| Strategy | Median Sharpe | Avg Max DD |
|----------|---------------|------------|
| Supertrend | 0.289 | 6.5% |
| Parabolic SAR | 0.287 | 6.5% |
| 52-Week High | 0.284 | 6.6% |
| Larry Williams | 0.266 | 6.6% |
| MA Crossover | 0.231 | 5.8% |
| STARC | 0.204 | 6.2% |
| TSMOM | 0.191 | 6.8% |
| Aroon | 0.105 | 6.8% |
| Ensemble | 0.104 | 6.2% |
| Donchian | 0.085 | 6.9% |

---

## Best Sectors

| Sector | Median Sharpe |
|--------|---------------|
| Real Estate | 0.279 |
| Industrials | 0.265 |
| Consumer Cyclical | 0.255 |
| Energy | 0.092 (lagging) |

---

## Best Strategy + Sector Combos

| Combination | Median Sharpe |
|-------------|---------------|
| Supertrend + Real Estate | 0.363 |
| 52-Week High + Real Estate | 0.347 |
| Supertrend + Industrials | 0.334 |

---

## Most Robust Configs (100% Win Ratio Across 98 Symbols)

| Config | Robustness Score |
|--------|------------------|
| Supertrend ATR=8, Mult=3.0 | 0.260 |
| Supertrend ATR=6, Mult=3.0 | 0.260 |
| Parabolic SAR (0.02/0.02/0.2) | 0.259 |
| 52-Week High (95/80/50) | 0.257 |

---

## Key Insights

- **Supertrend** and **Parabolic SAR** with multiplier 3.0 show the most consistent performance across all 98 symbols
- 100% positive Sharpe ratios for top configs
- Current threshold: 0.286-0.289 median Sharpe
- Target: Need to exceed 0.30 to trigger walk-forward validation

---

## Next Steps

- [ ] Fine-tune Supertrend parameters around ATR 6-8, Mult 3.0
- [ ] Investigate Real Estate sector outperformance
- [ ] Test reduced universes (sector-specific) for higher Sharpe
- [ ] Run walk-forward validation once 0.30 threshold crossed
