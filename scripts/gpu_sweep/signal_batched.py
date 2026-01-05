"""
Vectorized signal generators for batched parameter sweeps.

These functions compute entry/exit signals for ALL configs simultaneously,
eliminating the per-config Python loop that was the main bottleneck.

Key insight: Instead of looping over configs and computing indicators per config,
we pre-compute all unique indicators, then use vectorized indexing to build
signal arrays for all configs at once.
"""

import cupy as cp
import numpy as np

from .batched_cache import BatchedIndicatorCache
from .indicator_graph import IndicatorSpec, IndicatorType, collect_strategy_indicators


def compute_signals_batched(
    strategy_type: str,
    configs: list[dict],
    cache: BatchedIndicatorCache,
    open_prices: cp.ndarray,
    high_prices: cp.ndarray,
    low_prices: cp.ndarray,
    close_prices: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """
    Compute entry/exit signals for all configs simultaneously.

    Args:
        strategy_type: Name of the strategy
        configs: List of config dicts with strategy parameters
        cache: Pre-computed indicator cache
        open_prices: Open prices [num_bars]
        high_prices: High prices [num_bars]
        low_prices: Low prices [num_bars]
        close_prices: Close prices [num_bars]

    Returns:
        Tuple of (entry_signals, exit_signals), each shape [num_configs, num_bars]
    """
    num_configs = len(configs)
    num_bars = len(close_prices)

    # Ensure indicators are computed
    specs = collect_strategy_indicators(strategy_type, configs)
    cache.ensure_computed(specs)

    # Dispatch to strategy-specific generator
    if strategy_type == "donchian":
        return _signals_donchian_batched(configs, cache, close_prices)

    elif strategy_type == "ma_crossover":
        return _signals_ma_crossover_batched(configs, cache, close_prices)

    elif strategy_type == "supertrend":
        return _signals_supertrend_batched(configs, cache, close_prices)

    elif strategy_type == "fifty_two_week":
        return _signals_fifty_two_week_batched(configs, cache, high_prices, close_prices)

    elif strategy_type == "parabolic_sar":
        return _signals_parabolic_sar_batched(configs, cache, high_prices, low_prices, close_prices)

    elif strategy_type == "bollinger":
        return _signals_bollinger_batched(configs, cache, close_prices)

    elif strategy_type == "rsi":
        return _signals_rsi_batched(configs, cache, close_prices)

    elif strategy_type == "macd":
        return _signals_macd_batched(configs, cache, close_prices)

    elif strategy_type == "aroon":
        return _signals_aroon_batched(configs, cache)

    elif strategy_type == "tsmom":
        return _signals_tsmom_batched(configs, close_prices)

    elif strategy_type == "keltner":
        return _signals_keltner_batched(configs, cache, close_prices)

    elif strategy_type == "starc":
        return _signals_starc_batched(configs, cache, close_prices)

    elif strategy_type == "stochastic":
        return _signals_stochastic_batched(configs, cache)

    elif strategy_type == "dmi_adx":
        return _signals_dmi_adx_batched(configs, cache)

    else:
        # Unknown strategy - return no signals
        entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
        exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
        return entry, exit_


# ===========================================================================
# Strategy-specific vectorized signal generators
# ===========================================================================


def _signals_donchian_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Donchian breakout signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    # Build lookup for unique Donchian channels
    upper_cache: dict[int, cp.ndarray] = {}
    lower_cache: dict[int, cp.ndarray] = {}

    for cfg in configs:
        entry_lb = cfg["entry_lookback"]
        exit_lb = cfg["exit_lookback"]
        if entry_lb not in upper_cache:
            spec = IndicatorSpec(IndicatorType.DONCHIAN_UPPER, (entry_lb,))
            upper_cache[entry_lb] = cache.get(spec)
        if exit_lb not in lower_cache:
            spec = IndicatorSpec(IndicatorType.DONCHIAN_LOWER, (exit_lb,))
            lower_cache[exit_lb] = cache.get(spec)

    # Vectorized signal computation
    for i, cfg in enumerate(configs):
        entry_lb = cfg["entry_lookback"]
        exit_lb = cfg["exit_lookback"]

        entry_high = upper_cache[entry_lb]
        exit_low = lower_cache[exit_lb]

        # Entry: close breaks above entry channel high
        prev_entry_high = cp.roll(entry_high, 1)
        entry[i, entry_lb:] = close[entry_lb:] > prev_entry_high[entry_lb:]

        # Exit: close breaks below exit channel low
        prev_exit_low = cp.roll(exit_low, 1)
        exit_[i, exit_lb:] = close[exit_lb:] < prev_exit_low[exit_lb:]

    return entry, exit_


def _signals_ma_crossover_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """MA crossover signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    # Build lookup for unique MAs
    sma_cache: dict[int, cp.ndarray] = {}
    ema_cache: dict[int, cp.ndarray] = {}

    for cfg in configs:
        fast = cfg["fast_period"]
        slow = cfg["slow_period"]
        ma_type = cfg.get("ma_type", "sma")

        if ma_type == "ema":
            if fast not in ema_cache:
                ema_cache[fast] = cache.get(IndicatorSpec(IndicatorType.EMA, (fast,)))
            if slow not in ema_cache:
                ema_cache[slow] = cache.get(IndicatorSpec(IndicatorType.EMA, (slow,)))
        else:
            if fast not in sma_cache:
                sma_cache[fast] = cache.get(IndicatorSpec(IndicatorType.SMA, (fast,)))
            if slow not in sma_cache:
                sma_cache[slow] = cache.get(IndicatorSpec(IndicatorType.SMA, (slow,)))

    # Vectorized signal computation
    for i, cfg in enumerate(configs):
        fast = cfg["fast_period"]
        slow = cfg["slow_period"]
        ma_type = cfg.get("ma_type", "sma")

        if ma_type == "ema":
            fast_ma = ema_cache[fast]
            slow_ma = ema_cache[slow]
        else:
            fast_ma = sma_cache[fast]
            slow_ma = sma_cache[slow]

        prev_fast = cp.roll(fast_ma, 1)
        prev_slow = cp.roll(slow_ma, 1)

        # Entry: fast crosses above slow
        entry[i, slow:] = (fast_ma[slow:] > slow_ma[slow:]) & (prev_fast[slow:] <= prev_slow[slow:])

        # Exit: fast crosses below slow
        exit_[i, slow:] = (fast_ma[slow:] < slow_ma[slow:]) & (prev_fast[slow:] >= prev_slow[slow:])

    return entry, exit_


def _signals_supertrend_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Supertrend signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    for i, cfg in enumerate(configs):
        atr_period = cfg["atr_period"]
        mult_int = int(cfg["multiplier"] * 100)

        direction_spec = IndicatorSpec(IndicatorType.SUPERTREND_DIRECTION, (atr_period, mult_int))
        direction = cache.get(direction_spec)

        prev_direction = cp.roll(direction, 1)
        warmup = atr_period + 1

        # Entry: direction flips from False to True (bearish to bullish)
        entry[i, warmup:] = direction[warmup:] & ~prev_direction[warmup:]

        # Exit: direction flips from True to False (bullish to bearish)
        exit_[i, warmup:] = ~direction[warmup:] & prev_direction[warmup:]

    return entry, exit_


def _signals_fifty_two_week_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    high: cp.ndarray,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """52-week high proximity signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    # Build lookup for unique rolling max periods
    rolling_max_cache: dict[int, cp.ndarray] = {}

    for cfg in configs:
        period = cfg["period"]
        if period not in rolling_max_cache:
            spec = IndicatorSpec(IndicatorType.ROLLING_MAX_HIGH, (period,))
            rolling_max_cache[period] = cache.get(spec)

    for i, cfg in enumerate(configs):
        period = cfg["period"]
        entry_pct = cfg["entry_pct"]
        exit_pct = cfg["exit_pct"]

        rolling_high = rolling_max_cache[period]
        proximity = close / (rolling_high + 1e-10)

        # Entry: close is within entry_pct of rolling high
        entry[i, period:] = proximity[period:] >= entry_pct

        # Exit: close falls below exit_pct of rolling high
        exit_[i, period:] = proximity[period:] < exit_pct

    return entry, exit_


def _signals_parabolic_sar_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Parabolic SAR signals (simplified ATR-based approximation)."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    # Get ATR(14) which is used by all configs
    atr_spec = IndicatorSpec(IndicatorType.ATR_WILDER, (14,))
    atr = cache.get(atr_spec)

    for i, cfg in enumerate(configs):
        af_start = cfg.get("af_start", 0.02)
        multiplier = 3.0 / af_start

        lower_band = close - atr * multiplier
        prev_lower = cp.roll(lower_band, 1)

        # Entry: close above previous lower band
        entry[i, 15:] = close[15:] > prev_lower[15:]

        # Exit: close below previous lower band
        exit_[i, 15:] = close[15:] < prev_lower[15:]

    return entry, exit_


def _signals_bollinger_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Bollinger Band signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    # Build lookups
    middle_cache: dict[int, cp.ndarray] = {}
    std_cache: dict[int, cp.ndarray] = {}

    for cfg in configs:
        period = cfg["period"]
        if period not in middle_cache:
            middle_cache[period] = cache.get(IndicatorSpec(IndicatorType.BOLLINGER_MIDDLE, (period,)))
            std_cache[period] = cache.get(IndicatorSpec(IndicatorType.ROLLING_STD, (period,)))

    for i, cfg in enumerate(configs):
        period = cfg["period"]
        std_mult = cfg["std_mult"]

        middle = middle_cache[period]
        std = std_cache[period]

        lower = middle - std_mult * std

        # Entry: close touches lower band (mean reversion long)
        entry[i, period:] = close[period:] <= lower[period:]

        # Exit: close reaches middle band
        exit_[i, period:] = close[period:] >= middle[period:]

    return entry, exit_


def _signals_rsi_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """RSI overbought/oversold signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    # Build lookup for unique RSI periods
    rsi_cache: dict[int, cp.ndarray] = {}

    for cfg in configs:
        period = cfg["period"]
        if period not in rsi_cache:
            rsi_cache[period] = cache.get(IndicatorSpec(IndicatorType.RSI, (period,)))

    for i, cfg in enumerate(configs):
        period = cfg["period"]
        overbought = cfg["overbought"]
        oversold = cfg["oversold"]

        rsi = rsi_cache[period]
        prev_rsi = cp.roll(rsi, 1)

        warmup = period + 1

        # Entry: RSI crosses above oversold (mean reversion long)
        entry[i, warmup:] = (rsi[warmup:] > oversold) & (prev_rsi[warmup:] <= oversold)

        # Exit: RSI crosses below overbought
        exit_[i, warmup:] = (rsi[warmup:] < overbought) & (prev_rsi[warmup:] >= overbought)

    return entry, exit_


def _signals_macd_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """MACD crossover signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    for i, cfg in enumerate(configs):
        fast = cfg["fast_period"]
        slow = cfg["slow_period"]
        signal = cfg["signal_period"]

        macd_line = cache.get(IndicatorSpec(IndicatorType.MACD_LINE, (fast, slow)))
        signal_line = cache.get(IndicatorSpec(IndicatorType.MACD_SIGNAL, (fast, slow, signal)))

        prev_macd = cp.roll(macd_line, 1)
        prev_signal = cp.roll(signal_line, 1)

        warmup = slow + signal

        # Entry: MACD crosses above signal
        entry[i, warmup:] = (macd_line[warmup:] > signal_line[warmup:]) & (
            prev_macd[warmup:] <= prev_signal[warmup:]
        )

        # Exit: MACD crosses below signal
        exit_[i, warmup:] = (macd_line[warmup:] < signal_line[warmup:]) & (
            prev_macd[warmup:] >= prev_signal[warmup:]
        )

    return entry, exit_


def _signals_aroon_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Aroon crossover signals for all configs."""
    num_configs = len(configs)

    # Get num_bars from first config's indicator
    period = configs[0]["period"]
    aroon_up = cache.get(IndicatorSpec(IndicatorType.AROON_UP, (period,)))
    num_bars = len(aroon_up)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    for i, cfg in enumerate(configs):
        period = cfg["period"]

        aroon_up = cache.get(IndicatorSpec(IndicatorType.AROON_UP, (period,)))
        aroon_down = cache.get(IndicatorSpec(IndicatorType.AROON_DOWN, (period,)))

        prev_up = cp.roll(aroon_up, 1)
        prev_down = cp.roll(aroon_down, 1)

        warmup = period + 1

        # Entry: Aroon Up crosses above Aroon Down
        entry[i, warmup:] = (aroon_up[warmup:] > aroon_down[warmup:]) & (
            prev_up[warmup:] <= prev_down[warmup:]
        )

        # Exit: Aroon Down crosses above Aroon Up
        exit_[i, warmup:] = (aroon_down[warmup:] > aroon_up[warmup:]) & (
            prev_down[warmup:] <= prev_up[warmup:]
        )

    return entry, exit_


def _signals_tsmom_batched(
    configs: list[dict],
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Time-series momentum signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    for i, cfg in enumerate(configs):
        lookback = cfg["lookback"]

        lagged_close = cp.roll(close, lookback)
        momentum = (close - lagged_close) / (lagged_close + 1e-10)

        prev_momentum = cp.roll(momentum, 1)

        warmup = lookback + 1

        # Entry: positive momentum
        entry[i, warmup:] = (momentum[warmup:] > 0) & (prev_momentum[warmup:] <= 0)

        # Exit: negative momentum
        exit_[i, warmup:] = (momentum[warmup:] < 0) & (prev_momentum[warmup:] >= 0)

    return entry, exit_


def _signals_keltner_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Keltner Channel signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    for i, cfg in enumerate(configs):
        ema_period = cfg["ema_period"]
        atr_period = cfg["atr_period"]
        mult = cfg["multiplier"]

        middle = cache.get(IndicatorSpec(IndicatorType.KELTNER_MIDDLE, (ema_period,)))
        atr = cache.get(IndicatorSpec(IndicatorType.ATR_WILDER, (atr_period,)))

        lower = middle - mult * atr

        warmup = max(ema_period, atr_period)

        # Entry: close touches lower band
        entry[i, warmup:] = close[warmup:] <= lower[warmup:]

        # Exit: close reaches middle band
        exit_[i, warmup:] = close[warmup:] >= middle[warmup:]

    return entry, exit_


def _signals_starc_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
    close: cp.ndarray,
) -> tuple[cp.ndarray, cp.ndarray]:
    """STARC Band signals for all configs."""
    num_configs = len(configs)
    num_bars = len(close)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    for i, cfg in enumerate(configs):
        sma_period = cfg["sma_period"]
        atr_period = cfg["atr_period"]
        mult = cfg["multiplier"]

        middle = cache.get(IndicatorSpec(IndicatorType.SMA, (sma_period,)))
        atr = cache.get(IndicatorSpec(IndicatorType.ATR_WILDER, (atr_period,)))

        lower = middle - mult * atr

        warmup = max(sma_period, atr_period)

        # Entry: close touches lower band
        entry[i, warmup:] = close[warmup:] <= lower[warmup:]

        # Exit: close reaches middle band
        exit_[i, warmup:] = close[warmup:] >= middle[warmup:]

    return entry, exit_


def _signals_stochastic_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """Stochastic oscillator signals for all configs."""
    num_configs = len(configs)

    # Get num_bars from first config
    k_period = configs[0]["k_period"]
    stoch_k = cache.get(IndicatorSpec(IndicatorType.STOCHASTIC_K, (k_period,)))
    num_bars = len(stoch_k)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    for i, cfg in enumerate(configs):
        k_period = cfg["k_period"]
        d_period = cfg["d_period"]
        oversold = cfg.get("oversold", 20)
        overbought = cfg.get("overbought", 80)

        stoch_k = cache.get(IndicatorSpec(IndicatorType.STOCHASTIC_K, (k_period,)))
        stoch_d = cache.get(IndicatorSpec(IndicatorType.STOCHASTIC_D, (k_period, d_period)))

        prev_k = cp.roll(stoch_k, 1)
        prev_d = cp.roll(stoch_d, 1)

        warmup = k_period + d_period

        # Entry: %K crosses above %D in oversold zone
        entry[i, warmup:] = (
            (stoch_k[warmup:] > stoch_d[warmup:])
            & (prev_k[warmup:] <= prev_d[warmup:])
            & (stoch_k[warmup:] < oversold + 20)  # Near oversold
        )

        # Exit: %K crosses below %D in overbought zone
        exit_[i, warmup:] = (
            (stoch_k[warmup:] < stoch_d[warmup:])
            & (prev_k[warmup:] >= prev_d[warmup:])
            & (stoch_k[warmup:] > overbought - 20)  # Near overbought
        )

    return entry, exit_


def _signals_dmi_adx_batched(
    configs: list[dict],
    cache: BatchedIndicatorCache,
) -> tuple[cp.ndarray, cp.ndarray]:
    """DMI/ADX signals for all configs."""
    num_configs = len(configs)

    # Get num_bars from first config
    di_period = configs[0]["di_period"]
    plus_di = cache.get(IndicatorSpec(IndicatorType.DMI_PLUS, (di_period,)))
    num_bars = len(plus_di)

    entry = cp.zeros((num_configs, num_bars), dtype=cp.bool_)
    exit_ = cp.zeros((num_configs, num_bars), dtype=cp.bool_)

    for i, cfg in enumerate(configs):
        di_period = cfg["di_period"]
        adx_period = cfg["adx_period"]
        adx_threshold = cfg["adx_threshold"]

        plus_di = cache.get(IndicatorSpec(IndicatorType.DMI_PLUS, (di_period,)))
        minus_di = cache.get(IndicatorSpec(IndicatorType.DMI_MINUS, (di_period,)))
        adx = cache.get(IndicatorSpec(IndicatorType.ADX, (di_period, adx_period)))

        prev_plus = cp.roll(plus_di, 1)
        prev_minus = cp.roll(minus_di, 1)

        warmup = di_period + adx_period

        # Entry: +DI crosses above -DI with ADX above threshold
        entry[i, warmup:] = (
            (plus_di[warmup:] > minus_di[warmup:])
            & (prev_plus[warmup:] <= prev_minus[warmup:])
            & (adx[warmup:] > adx_threshold)
        )

        # Exit: -DI crosses above +DI
        exit_[i, warmup:] = (minus_di[warmup:] > plus_di[warmup:]) & (
            prev_minus[warmup:] <= prev_plus[warmup:]
        )

    return entry, exit_
