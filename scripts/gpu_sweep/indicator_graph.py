"""
Indicator dependency graph for deduplication and topological ordering.

This module defines IndicatorSpec (a hashable identifier for cached indicators)
and provides topological sorting to compute indicators in dependency order.

Key insight: Many strategy configs share the same indicator parameters.
For example, 27 Keltner configs might all need EMA(10), EMA(20), EMA(30).
By deduplicating, we compute 3 EMAs instead of 27.
"""

from dataclasses import dataclass
from enum import Enum, auto
from typing import FrozenSet


class IndicatorType(Enum):
    """Types of indicators that can be computed and cached."""

    # Moving averages
    SMA = auto()
    EMA = auto()

    # Rolling extremes
    ROLLING_MAX = auto()
    ROLLING_MIN = auto()
    ROLLING_MAX_HIGH = auto()  # Rolling max specifically on high prices
    ROLLING_MIN_LOW = auto()  # Rolling min specifically on low prices

    # Donchian channels (derived from rolling extremes)
    DONCHIAN_UPPER = auto()
    DONCHIAN_LOWER = auto()

    # Volatility
    TRUE_RANGE = auto()
    ATR_SMA = auto()
    ATR_WILDER = auto()

    # Supertrend components
    SUPERTREND = auto()
    SUPERTREND_UPPER = auto()
    SUPERTREND_LOWER = auto()
    SUPERTREND_DIRECTION = auto()

    # Bollinger Bands components
    BOLLINGER_MIDDLE = auto()
    BOLLINGER_UPPER = auto()
    BOLLINGER_LOWER = auto()
    BOLLINGER_BANDWIDTH = auto()
    ROLLING_STD = auto()

    # RSI
    RSI = auto()

    # MACD components
    MACD_LINE = auto()
    MACD_SIGNAL = auto()
    MACD_HISTOGRAM = auto()

    # Aroon components
    AROON_UP = auto()
    AROON_DOWN = auto()
    AROON_OSCILLATOR = auto()

    # Keltner/STARC components
    KELTNER_MIDDLE = auto()
    KELTNER_UPPER = auto()
    KELTNER_LOWER = auto()

    # Stochastic components
    STOCHASTIC_K = auto()
    STOCHASTIC_D = auto()

    # DMI/ADX components
    DMI_PLUS = auto()
    DMI_MINUS = auto()
    ADX = auto()


@dataclass(frozen=True)
class IndicatorSpec:
    """
    Unique identifier for a cached indicator.

    This is hashable and can be used as a dictionary key.
    The params tuple contains the numeric parameters for the indicator.

    Examples:
        IndicatorSpec(IndicatorType.SMA, (20,))        # SMA with period 20
        IndicatorSpec(IndicatorType.EMA, (50,))        # EMA with period 50
        IndicatorSpec(IndicatorType.ATR_WILDER, (14,)) # Wilder ATR with period 14
        IndicatorSpec(IndicatorType.SUPERTREND, (10, 300))  # period=10, mult*100=300
    """

    type: IndicatorType
    params: tuple  # Numeric params, hashable

    def __repr__(self) -> str:
        params_str = ", ".join(str(p) for p in self.params)
        return f"{self.type.name}({params_str})"

    def dependencies(self) -> list["IndicatorSpec"]:
        """
        Return list of IndicatorSpecs this indicator depends on.

        Dependencies are computed based on indicator type and parameters.
        For example, ATR_WILDER depends on TRUE_RANGE.
        """
        deps: list[IndicatorSpec] = []

        if self.type == IndicatorType.ATR_SMA:
            # ATR_SMA(period) depends on TRUE_RANGE() and SMA(period) of TR
            deps.append(IndicatorSpec(IndicatorType.TRUE_RANGE, ()))

        elif self.type == IndicatorType.ATR_WILDER:
            # ATR_WILDER(period) depends on TRUE_RANGE()
            deps.append(IndicatorSpec(IndicatorType.TRUE_RANGE, ()))

        elif self.type == IndicatorType.SUPERTREND:
            # SUPERTREND(atr_period, mult) depends on ATR_WILDER(atr_period)
            atr_period = self.params[0]
            deps.append(IndicatorSpec(IndicatorType.ATR_WILDER, (atr_period,)))

        elif self.type == IndicatorType.DONCHIAN_UPPER:
            # DONCHIAN_UPPER(lookback) depends on ROLLING_MAX_HIGH(lookback)
            lookback = self.params[0]
            deps.append(IndicatorSpec(IndicatorType.ROLLING_MAX_HIGH, (lookback,)))

        elif self.type == IndicatorType.DONCHIAN_LOWER:
            # DONCHIAN_LOWER(lookback) depends on ROLLING_MIN_LOW(lookback)
            lookback = self.params[0]
            deps.append(IndicatorSpec(IndicatorType.ROLLING_MIN_LOW, (lookback,)))

        elif self.type == IndicatorType.BOLLINGER_MIDDLE:
            # BOLLINGER_MIDDLE(period) is just SMA(period)
            period = self.params[0]
            deps.append(IndicatorSpec(IndicatorType.SMA, (period,)))

        elif self.type in (IndicatorType.BOLLINGER_UPPER, IndicatorType.BOLLINGER_LOWER):
            # Depends on BOLLINGER_MIDDLE and ROLLING_STD
            period = self.params[0]
            deps.append(IndicatorSpec(IndicatorType.BOLLINGER_MIDDLE, (period,)))
            deps.append(IndicatorSpec(IndicatorType.ROLLING_STD, (period,)))

        elif self.type == IndicatorType.MACD_LINE:
            # MACD_LINE(fast, slow) depends on EMA(fast) and EMA(slow)
            fast, slow = self.params[0], self.params[1]
            deps.append(IndicatorSpec(IndicatorType.EMA, (fast,)))
            deps.append(IndicatorSpec(IndicatorType.EMA, (slow,)))

        elif self.type == IndicatorType.MACD_SIGNAL:
            # MACD_SIGNAL(fast, slow, signal) depends on MACD_LINE
            fast, slow, signal = self.params
            deps.append(IndicatorSpec(IndicatorType.MACD_LINE, (fast, slow)))

        elif self.type == IndicatorType.MACD_HISTOGRAM:
            # MACD_HISTOGRAM depends on MACD_LINE and MACD_SIGNAL
            fast, slow, signal = self.params
            deps.append(IndicatorSpec(IndicatorType.MACD_LINE, (fast, slow)))
            deps.append(IndicatorSpec(IndicatorType.MACD_SIGNAL, (fast, slow, signal)))

        elif self.type == IndicatorType.KELTNER_MIDDLE:
            # KELTNER_MIDDLE(ema_period) is EMA(ema_period)
            ema_period = self.params[0]
            deps.append(IndicatorSpec(IndicatorType.EMA, (ema_period,)))

        elif self.type in (IndicatorType.KELTNER_UPPER, IndicatorType.KELTNER_LOWER):
            # KELTNER bands depend on KELTNER_MIDDLE and ATR_WILDER
            ema_period, atr_period = self.params[0], self.params[1]
            deps.append(IndicatorSpec(IndicatorType.KELTNER_MIDDLE, (ema_period,)))
            deps.append(IndicatorSpec(IndicatorType.ATR_WILDER, (atr_period,)))

        elif self.type == IndicatorType.STOCHASTIC_D:
            # STOCHASTIC_D depends on STOCHASTIC_K
            k_period, d_period = self.params
            deps.append(IndicatorSpec(IndicatorType.STOCHASTIC_K, (k_period,)))

        elif self.type == IndicatorType.AROON_OSCILLATOR:
            # AROON_OSCILLATOR depends on AROON_UP and AROON_DOWN
            period = self.params[0]
            deps.append(IndicatorSpec(IndicatorType.AROON_UP, (period,)))
            deps.append(IndicatorSpec(IndicatorType.AROON_DOWN, (period,)))

        elif self.type == IndicatorType.ADX:
            # ADX depends on DMI_PLUS and DMI_MINUS
            di_period, adx_period = self.params
            deps.append(IndicatorSpec(IndicatorType.DMI_PLUS, (di_period,)))
            deps.append(IndicatorSpec(IndicatorType.DMI_MINUS, (di_period,)))

        return deps


def expand_dependencies(specs: set[IndicatorSpec]) -> set[IndicatorSpec]:
    """
    Expand a set of indicator specs to include all transitive dependencies.

    Args:
        specs: Initial set of indicator specs needed

    Returns:
        Expanded set including all dependencies
    """
    result = set(specs)
    queue = list(specs)

    while queue:
        spec = queue.pop()
        for dep in spec.dependencies():
            if dep not in result:
                result.add(dep)
                queue.append(dep)

    return result


def topological_sort(specs: set[IndicatorSpec]) -> list[IndicatorSpec]:
    """
    Sort indicator specs in dependency order (dependencies first).

    Uses Kahn's algorithm for topological sorting.

    Args:
        specs: Set of indicator specs to sort

    Returns:
        List of specs in order such that dependencies come before dependents

    Raises:
        ValueError: If there's a circular dependency (shouldn't happen)
    """
    # Expand to include all dependencies
    all_specs = expand_dependencies(specs)

    # Build adjacency and in-degree maps
    in_degree: dict[IndicatorSpec, int] = {spec: 0 for spec in all_specs}
    dependents: dict[IndicatorSpec, list[IndicatorSpec]] = {spec: [] for spec in all_specs}

    for spec in all_specs:
        for dep in spec.dependencies():
            if dep in all_specs:
                dependents[dep].append(spec)
                in_degree[spec] += 1

    # Start with specs that have no dependencies
    queue = [spec for spec in all_specs if in_degree[spec] == 0]
    result: list[IndicatorSpec] = []

    while queue:
        # Pop from queue (order doesn't matter for correctness)
        spec = queue.pop(0)
        result.append(spec)

        # Reduce in-degree for dependents
        for dependent in dependents[spec]:
            in_degree[dependent] -= 1
            if in_degree[dependent] == 0:
                queue.append(dependent)

    # Check for cycles
    if len(result) != len(all_specs):
        missing = all_specs - set(result)
        raise ValueError(f"Circular dependency detected involving: {missing}")

    return result


def collect_strategy_indicators(
    strategy_type: str,
    configs: list[dict],
) -> set[IndicatorSpec]:
    """
    Collect all unique indicator specs needed for a strategy's configs.

    This examines the strategy type and all config parameters to determine
    which indicators need to be computed.

    Args:
        strategy_type: Name of the strategy (e.g., 'ma_crossover', 'supertrend')
        configs: List of config dicts with strategy parameters

    Returns:
        Set of unique IndicatorSpecs needed for all configs
    """
    specs: set[IndicatorSpec] = set()

    if strategy_type == "donchian":
        for cfg in configs:
            entry_lb = cfg["entry_lookback"]
            exit_lb = cfg["exit_lookback"]
            specs.add(IndicatorSpec(IndicatorType.DONCHIAN_UPPER, (entry_lb,)))
            specs.add(IndicatorSpec(IndicatorType.DONCHIAN_LOWER, (exit_lb,)))

    elif strategy_type == "ma_crossover":
        for cfg in configs:
            fast = cfg["fast_period"]
            slow = cfg["slow_period"]
            ma_type = cfg.get("ma_type", "sma")
            if ma_type == "ema":
                specs.add(IndicatorSpec(IndicatorType.EMA, (fast,)))
                specs.add(IndicatorSpec(IndicatorType.EMA, (slow,)))
            else:
                specs.add(IndicatorSpec(IndicatorType.SMA, (fast,)))
                specs.add(IndicatorSpec(IndicatorType.SMA, (slow,)))

    elif strategy_type == "supertrend":
        for cfg in configs:
            atr_period = cfg["atr_period"]
            # Store multiplier as int (x100) to avoid float hashing issues
            mult_int = int(cfg["multiplier"] * 100)
            specs.add(IndicatorSpec(IndicatorType.SUPERTREND, (atr_period, mult_int)))

    elif strategy_type == "fifty_two_week":
        for cfg in configs:
            period = cfg["period"]
            specs.add(IndicatorSpec(IndicatorType.ROLLING_MAX_HIGH, (period,)))

    elif strategy_type == "parabolic_sar":
        # Parabolic SAR uses ATR internally
        specs.add(IndicatorSpec(IndicatorType.ATR_WILDER, (14,)))

    elif strategy_type == "bollinger":
        for cfg in configs:
            period = cfg["period"]
            specs.add(IndicatorSpec(IndicatorType.BOLLINGER_MIDDLE, (period,)))
            specs.add(IndicatorSpec(IndicatorType.ROLLING_STD, (period,)))

    elif strategy_type == "rsi":
        for cfg in configs:
            period = cfg["period"]
            specs.add(IndicatorSpec(IndicatorType.RSI, (period,)))

    elif strategy_type == "macd":
        for cfg in configs:
            fast = cfg["fast_period"]
            slow = cfg["slow_period"]
            signal = cfg["signal_period"]
            specs.add(IndicatorSpec(IndicatorType.MACD_LINE, (fast, slow)))
            specs.add(IndicatorSpec(IndicatorType.MACD_SIGNAL, (fast, slow, signal)))

    elif strategy_type == "aroon":
        for cfg in configs:
            period = cfg["period"]
            specs.add(IndicatorSpec(IndicatorType.AROON_UP, (period,)))
            specs.add(IndicatorSpec(IndicatorType.AROON_DOWN, (period,)))

    elif strategy_type == "tsmom":
        # TSMOM doesn't need precomputed indicators (just lagged close)
        pass

    elif strategy_type == "keltner":
        for cfg in configs:
            ema_period = cfg["ema_period"]
            atr_period = cfg["atr_period"]
            specs.add(IndicatorSpec(IndicatorType.KELTNER_MIDDLE, (ema_period,)))
            specs.add(IndicatorSpec(IndicatorType.ATR_WILDER, (atr_period,)))

    elif strategy_type == "starc":
        for cfg in configs:
            sma_period = cfg["sma_period"]
            atr_period = cfg["atr_period"]
            specs.add(IndicatorSpec(IndicatorType.SMA, (sma_period,)))
            specs.add(IndicatorSpec(IndicatorType.ATR_WILDER, (atr_period,)))

    elif strategy_type == "stochastic":
        for cfg in configs:
            k_period = cfg["k_period"]
            d_period = cfg["d_period"]
            specs.add(IndicatorSpec(IndicatorType.STOCHASTIC_K, (k_period,)))
            specs.add(IndicatorSpec(IndicatorType.STOCHASTIC_D, (k_period, d_period)))

    elif strategy_type == "dmi_adx":
        for cfg in configs:
            di_period = cfg["di_period"]
            adx_period = cfg["adx_period"]
            specs.add(IndicatorSpec(IndicatorType.DMI_PLUS, (di_period,)))
            specs.add(IndicatorSpec(IndicatorType.DMI_MINUS, (di_period,)))
            specs.add(IndicatorSpec(IndicatorType.ADX, (di_period, adx_period)))

    return specs
