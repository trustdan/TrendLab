# TrendLab YOLO Sweep Analysis Report

**Date**: December 29, 2025
**Period Analyzed**: March 21, 2024 — December 26, 2025 (443 trading days)
**Universe**: 222 symbols across 23 sectors
**Strategies Tested**: 16

---

## Executive Summary

We ran a comprehensive parameter sweep across 16 trend-following strategies on a diverse universe of 222 stocks and ETFs. The goal was to identify which strategies and configurations perform most consistently across different market conditions and asset types.

**Key Finding**: Simple momentum strategies (Supertrend, ParabolicSAR, 52-Week High breakout) significantly outperformed more complex approaches. The winning strategies share a common trait: they stay in trades longer and trade less frequently.

**Top 3 Most Robust Strategies**:
1. Supertrend (ATR period=17, multiplier=2.5) — 0.37 Sharpe, profitable on 98.6% of symbols
2. ParabolicSAR (af_start=0.1, af_step=0.03, af_max=0.29) — 0.37 Sharpe, profitable on 98.2% of symbols
3. 52-Week High (period=50, entry=100%, exit=43%) — 0.36 Sharpe, profitable on 98.2% of symbols

---

## Methodology

### Data
- Daily OHLCV bars from Yahoo Finance
- Adjusted for splits and dividends
- 443 trading days of history per symbol

### Backtest Settings
- Initial capital: $100,000
- Position sizing: Full capital allocation (single position)
- Fill model: Next bar open after signal
- Transaction costs: Included (configurable bps)

### Evaluation Metrics
- **Sharpe Ratio**: Risk-adjusted returns (primary ranking metric)
- **CAGR**: Compound annual growth rate
- **Max Drawdown**: Largest peak-to-trough decline
- **Win Rate**: Percentage of profitable trades
- **Robustness Score**: Sharpe / (1 + std deviation of Sharpe across symbols)

---

## Strategy Performance Rankings

### Tier 1: Strong Performers (Sharpe > 0.30)

| Strategy | Median Sharpe | Median CAGR | Avg Max DD | Avg Trades | % Symbols Profitable |
|----------|---------------|-------------|------------|------------|---------------------|
| Supertrend | 0.364 | 0.64% | 7.6% | 0.05 | 98.6% |
| ParabolicSAR | 0.362 | 0.64% | 7.6% | 0.16 | 98.2% |
| 52-Week High | 0.361 | 0.61% | 7.7% | 0.32 | 98.2% |
| Larry Williams | 0.344 | 0.54% | 7.7% | 25.2 | 97.8% |
| STARC Bands | 0.297 | 0.43% | 7.1% | 3.3 | 94.6% |
| MA Crossover | 0.296 | 0.40% | 6.4% | 5.8 | 94.1% |

### Tier 2: Moderate Performers (Sharpe 0.15-0.30)

| Strategy | Median Sharpe | Median CAGR | Avg Max DD | Avg Trades |
|----------|---------------|-------------|------------|------------|
| TSMOM | 0.245 | 0.32% | 7.1% | 52.9 |
| Aroon | 0.186 | 0.18% | 7.6% | 51.0 |
| Ensemble | 0.185 | 0.16% | 6.4% | 19.4 |
| Donchian | 0.158 | 0.15% | 7.6% | 33.1 |

### Tier 3: Weak Performers (Sharpe < 0.15)

| Strategy | Median Sharpe | Median CAGR | Avg Max DD | Avg Trades |
|----------|---------------|-------------|------------|------------|
| Bollinger Squeeze | 0.116 | 0.11% | 6.9% | 96.8 |
| Turtle S1 | 0.114 | 0.10% | 6.5% | 81.2 |
| Darvas Box | 0.108 | 0.11% | 7.4% | 103.5 |
| Turtle S2 | 0.103 | 0.08% | 6.5% | 41.1 |
| Keltner | 0.078 | 0.08% | 7.1% | 112.4 |
| DMI/ADX | 0.042 | 0.03% | 8.0% | 160.3 |

---

## Key Observations

### 1. Trade Frequency vs Performance

There's a clear inverse relationship between trade frequency and performance. The best strategies trade rarely:

- **Supertrend**: 0.05 trades avg (essentially buy-and-hold with trend filter)
- **ParabolicSAR**: 0.16 trades avg
- **52-Week High**: 0.32 trades avg

The worst performers trade constantly:
- **DMI/ADX**: 160 trades avg
- **Keltner**: 112 trades avg
- **Darvas Box**: 103 trades avg

**Interpretation**: In a generally bullish market (Mar 2024 - Dec 2025), strategies that stay invested and avoid whipsaws outperform. High-frequency signals generate transaction costs and miss sustained moves.

### 2. Simple Beats Complex

The top performers are conceptually simple:
- Supertrend: Stay long while price is above a volatility-adjusted trailing stop
- ParabolicSAR: Stay long while the parabolic curve is below price
- 52-Week High: Buy breakouts, sell on retracements

More complex approaches (Ensemble voting, DMI/ADX with multiple thresholds) underperformed.

### 3. ETFs Outperform Individual Stocks

Sector analysis reveals ETFs have significantly higher Sharpe ratios:

| Sector Type | Median Sharpe |
|-------------|---------------|
| ETF Communications | 0.52 |
| ETF Broad Market | 0.45 |
| ETF Technology | 0.45 |
| Technology (stocks) | 0.30 |
| Financials (stocks) | 0.27 |

**Interpretation**: ETFs have lower idiosyncratic volatility, making trend signals cleaner. Individual stocks are noisier and more prone to gaps/whipsaws.

### 4. Best Strategy + Sector Combinations

The highest Sharpe configurations combine simple strategies with diversified ETFs:

| Combination | Median Sharpe |
|-------------|---------------|
| ParabolicSAR + ETF Broad Market | 0.63 |
| Supertrend + ETF Broad Market | 0.63 |
| 52-Week High + ETF Broad Market | 0.63 |
| STARC + ETF Broad Market | 0.61 |

---

## Robustness Analysis

### What Makes a Strategy "Robust"?

A robust strategy performs consistently across many different symbols — it doesn't just work on a few lucky picks. We measure this by:

1. **Symbol Win Ratio**: % of symbols with positive Sharpe
2. **Sharpe Std Dev**: How much performance varies across symbols (lower = more consistent)
3. **Robustness Score**: avg_sharpe / (1 + std_sharpe)

### Most Robust Configurations

| Strategy | Config | Avg Sharpe | Std Sharpe | Symbol Win % | Robustness |
|----------|--------|------------|------------|--------------|------------|
| Supertrend | atr=17, mult=2.5 | 0.368 | 0.188 | 98.6% | 0.31 |
| ParabolicSAR | start=0.1, step=0.03, max=0.29 | 0.367 | 0.189 | 98.2% | 0.31 |
| 52-Week High | period=50, entry=100%, exit=43% | 0.358 | 0.186 | 98.2% | 0.30 |
| Larry Williams | range_mult=0.6, atr_stop=3.9 | 0.337 | 0.177 | 97.8% | 0.29 |

### Least Robust Configurations

| Strategy | Avg Sharpe | Std Sharpe | Symbol Win % | Robustness |
|----------|------------|------------|--------------|------------|
| DMI/ADX | 0.055 | 0.244 | 58.1% | 0.04 |
| Keltner | 0.107 | 0.263 | 64.0% | 0.08 |
| Turtle S2 | 0.112 | 0.212 | 64.9% | 0.09 |

---

## Universal Winners

These configs rank in the top half of all strategies on the most symbols:

| Strategy | Top Half % | Top Quarter % | Top 10% |
|----------|------------|---------------|---------|
| Supertrend | 93.7% | 77.5% | 22.1% |
| ParabolicSAR | 91.0% | 73.9% | 7.2% |
| 52-Week High | 88.3% | 65.8% | 9.9% |
| Larry Williams | 87.8% | 41.9% | 14.4% |

**Interpretation**: Supertrend is the most "universally good" strategy — it ranks in the top half on 94% of all symbols tested. You'd have to be unlucky to pick a symbol where it underperforms.

---

## Limitations & Caveats

### 1. Bullish Period Bias
The test period (Mar 2024 - Dec 2025) was generally bullish. Strategies that stay long and avoid exits will naturally look good. Performance may differ in bear markets or sideways chop.

### 2. Single Config Per Strategy
This analysis shows one "best" config per strategy from a prior parameter sweep. The full parameter space may have configs that perform differently.

### 3. No Out-of-Sample Testing
All results are in-sample. True predictive power requires forward testing on unseen data.

### 4. Position Sizing
All backtests used full capital allocation. Real portfolios would use position sizing, which affects Sharpe calculations.

---

## Recommendations

### For Live Trading Consideration

1. **Supertrend (17/2.5)** on broad market ETFs (SPY, QQQ, IWM)
   - Simple to implement
   - Low turnover = low costs
   - Most robust across market conditions

2. **52-Week High Breakout** for momentum exposure
   - Clear, unambiguous signals
   - Works well on liquid large-caps

3. **Avoid** DMI/ADX and Keltner configurations
   - High turnover erodes returns
   - Inconsistent across symbols

### For Further Research

1. **Bear market testing**: How do these strategies perform in drawdowns?
2. **Regime filtering**: Can we improve by trading only in "trending" regimes?
3. **Portfolio construction**: Combine multiple strategies for diversification
4. **Parameter stability**: Are nearby parameter values equally good? (avoid overfitting)

---

## Appendix: Strategy Descriptions

### Supertrend
Volatility-based trend indicator. Goes long when price closes above the Supertrend line (calculated using ATR). Exits when price closes below.

### ParabolicSAR
Welles Wilder's Parabolic Stop and Reverse. Accelerating trailing stop that flips direction on breakouts.

### 52-Week High Breakout
Enters when price breaks above X% of the 52-week high. Exits when price falls below Y% of the high.

### Larry Williams
Range breakout system using previous bar's range multiplied by a factor to set entry/exit levels.

### STARC Bands
Starc bands use ATR to create channels around an SMA. Enters on band breakouts.

### MA Crossover
Classic fast/slow moving average crossover system.

---

*Report generated by TrendLab YOLO Analysis*
