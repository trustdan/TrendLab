"""
Base strategy abstraction for GPU Mega-Sweep.

Mirrors Rust StrategyV2 trait from crates/trendlab-core/src/strategy_v2.rs.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum, auto
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import cupy as cp


class IndicatorKey(Enum):
    """
    Indicator types for caching and deduplication.

    Mirrors Rust IndicatorKey from crates/trendlab-core/src/indicator_cache.rs.
    """

    # Moving Averages
    SMA = auto()
    EMA = auto()

    # Channels
    DONCHIAN = auto()
    BOLLINGER = auto()
    KELTNER = auto()
    STARC = auto()

    # Volatility
    TRUE_RANGE = auto()
    ATR = auto()
    ATR_WILDER = auto()

    # Trend
    SUPERTREND = auto()
    PARABOLIC_SAR = auto()

    # Directional
    DMI = auto()
    ADX = auto()
    AROON = auto()

    # Oscillators
    RSI = auto()
    MACD = auto()
    STOCHASTIC = auto()
    WILLIAMS_R = auto()
    CCI = auto()
    ROC = auto()

    # Other
    ROLLING_MAX = auto()
    ROLLING_MIN = auto()
    HEIKIN_ASHI = auto()
    TSMOM = auto()
    FIFTY_TWO_WEEK_HIGH = auto()


@dataclass(frozen=True)
class IndicatorReq:
    """
    A request for a specific indicator with parameters.

    Frozen dataclass for hashability (used in caching).

    Examples:
        IndicatorReq(IndicatorKey.SMA, (10,))  # SMA with window=10
        IndicatorReq(IndicatorKey.ATR_WILDER, (14,))  # Wilder ATR with window=14
        IndicatorReq(IndicatorKey.SUPERTREND, (10, 300))  # ATR period=10, mult=3.0 (x100)
    """

    key: IndicatorKey
    params: tuple  # Hashable params, e.g., (window,) or (period, multiplier_x100)

    def __str__(self) -> str:
        params_str = "_".join(str(p) for p in self.params)
        return f"{self.key.name.lower()}_{params_str}"


class Strategy(ABC):
    """
    Abstract base class for GPU-accelerated strategies.

    Mirrors Rust StrategyV2 trait. Each strategy must specify:
    1. Which indicators it needs (for caching/deduplication)
    2. How to compute entry/exit signals from indicators
    3. The warmup period before valid signals
    """

    @property
    @abstractmethod
    def id(self) -> str:
        """Unique identifier for this strategy (e.g., 'ma_crossover')."""

    @property
    @abstractmethod
    def params(self) -> dict:
        """Strategy parameters as a dict (for serialization)."""

    @abstractmethod
    def indicator_reqs(self) -> list[IndicatorReq]:
        """
        Return list of indicators needed for this strategy.

        Used for:
        1. Pre-computing indicators before signal generation
        2. Deduplicating indicators across multiple configs
        3. Building the indicator cache
        """

    @abstractmethod
    def compute_signals(
        self,
        indicators: dict[IndicatorReq, "cp.ndarray"],
        close: "cp.ndarray",
    ) -> tuple["cp.ndarray", "cp.ndarray"]:
        """
        Compute entry and exit signals from pre-computed indicators.

        Args:
            indicators: Dict mapping IndicatorReq to computed values.
                       Shape of each value: [num_bars] for single symbol
                       or [num_symbols, num_bars] for batched.
            close: Close prices, same shape as indicator values.

        Returns:
            Tuple of (entry_signals, exit_signals) as boolean arrays.
            Same shape as input arrays.

        Note:
            Signals are "raw" - they indicate when conditions are met,
            not accounting for current position state. The position
            scan handles state transitions.
        """

    @abstractmethod
    def warmup_period(self) -> int:
        """
        Minimum number of bars before valid signals.

        Signals before this point should be masked as False.
        This is typically max(indicator_lookbacks).
        """


# Global strategy registry for dynamic loading
STRATEGY_REGISTRY: dict[str, type[Strategy]] = {}


def register_strategy(cls: type[Strategy]) -> type[Strategy]:
    """Decorator to register a strategy class."""
    # Create a temporary instance to get the ID
    # This requires strategies to have default params or we inspect the class
    STRATEGY_REGISTRY[cls.__name__.lower().replace("strategy", "")] = cls
    return cls
