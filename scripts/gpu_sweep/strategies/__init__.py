"""
Strategy implementations for GPU Mega-Sweep.

Each strategy mirrors the corresponding Rust StrategyV2 implementation
in crates/trendlab-core/src/strategy_v2.rs.

Strategies are organized by category:
- Breakout: donchian, darvas_box, opening_range
- Moving Average: ma_crossover, tsmom
- Volatility: supertrend, keltner, starc, bollinger
- Other Trend: fifty_two_week, parabolic_sar, larry_williams, heikin_ashi
- Directional: dmi_adx, aroon
- Oscillators: rsi, macd, stochastic, williams_r, cci, roc
"""

from .base import Strategy, IndicatorKey, IndicatorReq, STRATEGY_REGISTRY

__all__ = ["Strategy", "IndicatorKey", "IndicatorReq", "STRATEGY_REGISTRY"]
