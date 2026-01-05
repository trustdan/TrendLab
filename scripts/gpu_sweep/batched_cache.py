"""
Batched indicator cache for GPU-accelerated parameter sweeps.

This module provides a GPU-resident cache that computes each unique indicator
only once, even when many strategy configs share the same parameters.

Key optimization: Instead of computing EMA(20) for each of 27 configs that
need it, we compute it once and reuse the result.

Phase 3 enhancement: Uses CUDA RawKernels to eliminate Python bar-by-bar
loops for stateful indicators (EMA, ATR, RSI, rolling operations).
"""

import cupy as cp

from .indicator_graph import (
    IndicatorSpec,
    IndicatorType,
    expand_dependencies,
    topological_sort,
)

# Import CUDA kernels for fast computation
from .kernels.ema import ema_kernel, wilder_smooth_kernel
from .kernels.rolling import (
    rolling_max_kernel,
    rolling_min_kernel,
    donchian_upper_kernel,
    donchian_lower_kernel,
    rolling_std_kernel,
)
from .kernels.rsi import rsi_kernel
from .kernels.supertrend import supertrend_kernel


class BatchedIndicatorCache:
    """
    GPU-resident cache for deduped indicator computation.

    Usage:
        cache = BatchedIndicatorCache(high, low, close)
        specs = collect_strategy_indicators('ma_crossover', configs)
        cache.ensure_computed(specs)
        ema_20 = cache.get(IndicatorSpec(IndicatorType.EMA, (20,)))
    """

    def __init__(
        self,
        high: cp.ndarray,
        low: cp.ndarray,
        close: cp.ndarray,
        open_prices: cp.ndarray | None = None,
        volume: cp.ndarray | None = None,
    ):
        """
        Initialize cache with OHLCV data.

        Args:
            high: High prices, shape [num_bars] or [num_symbols, num_bars]
            low: Low prices, same shape
            close: Close prices, same shape
            open_prices: Open prices (optional), same shape
            volume: Volume (optional), same shape
        """
        # Ensure 2D shape [num_symbols, num_bars]
        if high.ndim == 1:
            self.high = high.reshape(1, -1)
            self.low = low.reshape(1, -1)
            self.close = close.reshape(1, -1)
            self.open = open_prices.reshape(1, -1) if open_prices is not None else None
            self.volume = volume.reshape(1, -1) if volume is not None else None
            self._squeeze = True
        else:
            self.high = high
            self.low = low
            self.close = close
            self.open = open_prices
            self.volume = volume
            self._squeeze = False

        self.num_symbols, self.num_bars = self.close.shape
        self._cache: dict[IndicatorSpec, cp.ndarray] = {}

    def ensure_computed(self, specs: set[IndicatorSpec]) -> None:
        """
        Compute all needed indicators in dependency order.

        Args:
            specs: Set of indicator specs to compute
        """
        # Expand dependencies and sort topologically
        all_specs = expand_dependencies(specs)
        ordered = topological_sort(all_specs)

        # Compute in order (dependencies first)
        for spec in ordered:
            if spec not in self._cache:
                self._cache[spec] = self._compute(spec)

    def get(self, spec: IndicatorSpec) -> cp.ndarray:
        """
        Get cached indicator (must call ensure_computed first).

        Args:
            spec: Indicator spec to retrieve

        Returns:
            GPU array with indicator values, shape [num_symbols, num_bars]
            or [num_bars] if cache was initialized with 1D data

        Raises:
            KeyError: If indicator was not computed
        """
        if spec not in self._cache:
            raise KeyError(f"Indicator not computed: {spec}. Call ensure_computed first.")

        result = self._cache[spec]
        if self._squeeze and result.ndim == 2:
            return result.squeeze(0)
        return result

    def get_or_none(self, spec: IndicatorSpec) -> cp.ndarray | None:
        """Get cached indicator or None if not computed."""
        return self._cache.get(spec)

    def _compute(self, spec: IndicatorSpec) -> cp.ndarray:
        """
        Compute a single indicator.

        This dispatches to the appropriate computation function based on type.
        Dependencies are guaranteed to be already computed.
        """
        t = spec.type
        params = spec.params

        # Import indicator functions
        from . import indicators as ind

        # === Moving Averages ===
        if t == IndicatorType.SMA:
            window = params[0]
            return ind.sma_gpu(self.close, window)

        elif t == IndicatorType.EMA:
            window = params[0]
            # Use CUDA kernel for EMA (eliminates Python bar-by-bar loop)
            return ema_kernel(self.close, window)

        # === Rolling Extremes ===
        # Use CUDA kernels for rolling operations (eliminates Python loops)
        elif t == IndicatorType.ROLLING_MAX:
            window = params[0]
            return rolling_max_kernel(self.close, window)

        elif t == IndicatorType.ROLLING_MIN:
            window = params[0]
            return rolling_min_kernel(self.close, window)

        elif t == IndicatorType.ROLLING_MAX_HIGH:
            window = params[0]
            return rolling_max_kernel(self.high, window)

        elif t == IndicatorType.ROLLING_MIN_LOW:
            window = params[0]
            return rolling_min_kernel(self.low, window)

        # === Donchian ===
        # Use CUDA kernels for Donchian (prior bars only)
        elif t == IndicatorType.DONCHIAN_UPPER:
            lookback = params[0]
            return donchian_upper_kernel(self.high, lookback)

        elif t == IndicatorType.DONCHIAN_LOWER:
            lookback = params[0]
            return donchian_lower_kernel(self.low, lookback)

        # === Volatility ===
        elif t == IndicatorType.TRUE_RANGE:
            return ind.true_range_gpu(self.high, self.low, self.close)

        elif t == IndicatorType.ATR_SMA:
            window = params[0]
            return ind.atr_gpu(self.high, self.low, self.close, window)

        elif t == IndicatorType.ATR_WILDER:
            window = params[0]
            # Get True Range (either cached or compute)
            tr_spec = IndicatorSpec(IndicatorType.TRUE_RANGE, ())
            if tr_spec in self._cache:
                tr = self._cache[tr_spec]
            else:
                tr = ind.true_range_gpu(self.high, self.low, self.close)
                self._cache[tr_spec] = tr
            # Apply Wilder smoothing using CUDA kernel
            return wilder_smooth_kernel(tr, window)

        # === Supertrend ===
        elif t == IndicatorType.SUPERTREND:
            atr_period = params[0]
            mult_int = params[1]
            multiplier = mult_int / 100.0
            # Use CUDA kernel for Supertrend (eliminates Python bar-by-bar loop)
            st, direction, upper, lower = supertrend_kernel(
                self.high, self.low, self.close, atr_period, multiplier
            )
            # Cache the components too for potential reuse
            self._cache[IndicatorSpec(IndicatorType.SUPERTREND_DIRECTION, params)] = direction
            self._cache[IndicatorSpec(IndicatorType.SUPERTREND_UPPER, params)] = upper
            self._cache[IndicatorSpec(IndicatorType.SUPERTREND_LOWER, params)] = lower
            return st

        elif t == IndicatorType.SUPERTREND_DIRECTION:
            # Computed as side effect of SUPERTREND
            base_spec = IndicatorSpec(IndicatorType.SUPERTREND, params)
            if base_spec not in self._cache:
                self._compute(base_spec)
            return self._cache[spec]

        elif t == IndicatorType.SUPERTREND_UPPER:
            base_spec = IndicatorSpec(IndicatorType.SUPERTREND, params)
            if base_spec not in self._cache:
                self._compute(base_spec)
            return self._cache[spec]

        elif t == IndicatorType.SUPERTREND_LOWER:
            base_spec = IndicatorSpec(IndicatorType.SUPERTREND, params)
            if base_spec not in self._cache:
                self._compute(base_spec)
            return self._cache[spec]

        # === Bollinger Bands ===
        elif t == IndicatorType.ROLLING_STD:
            period = params[0]
            # Use CUDA kernel for rolling std
            return rolling_std_kernel(self.close, period)

        elif t == IndicatorType.BOLLINGER_MIDDLE:
            period = params[0]
            # Just SMA - check if already cached
            sma_spec = IndicatorSpec(IndicatorType.SMA, (period,))
            cached = self._cache.get(sma_spec)
            if cached is not None:
                return cached
            return ind.sma_gpu(self.close, period)

        elif t == IndicatorType.BOLLINGER_UPPER:
            period = params[0]
            std_mult = params[1] / 100.0 if len(params) > 1 else 2.0
            middle = self.get(IndicatorSpec(IndicatorType.BOLLINGER_MIDDLE, (period,)))
            std = self.get(IndicatorSpec(IndicatorType.ROLLING_STD, (period,)))
            return middle + std_mult * std

        elif t == IndicatorType.BOLLINGER_LOWER:
            period = params[0]
            std_mult = params[1] / 100.0 if len(params) > 1 else 2.0
            middle = self.get(IndicatorSpec(IndicatorType.BOLLINGER_MIDDLE, (period,)))
            std = self.get(IndicatorSpec(IndicatorType.ROLLING_STD, (period,)))
            return middle - std_mult * std

        # === RSI ===
        elif t == IndicatorType.RSI:
            period = params[0]
            # Use CUDA kernel for RSI (eliminates Python bar-by-bar loop)
            return rsi_kernel(self.close, period)

        # === MACD ===
        elif t == IndicatorType.MACD_LINE:
            fast, slow = params[0], params[1]
            fast_ema = self.get(IndicatorSpec(IndicatorType.EMA, (fast,)))
            slow_ema = self.get(IndicatorSpec(IndicatorType.EMA, (slow,)))
            return fast_ema - slow_ema

        elif t == IndicatorType.MACD_SIGNAL:
            fast, slow, signal = params
            macd_line = self.get(IndicatorSpec(IndicatorType.MACD_LINE, (fast, slow)))
            # Use CUDA kernel for EMA of MACD line
            return ema_kernel(macd_line, signal)

        elif t == IndicatorType.MACD_HISTOGRAM:
            fast, slow, signal = params
            macd_line = self.get(IndicatorSpec(IndicatorType.MACD_LINE, (fast, slow)))
            signal_line = self.get(IndicatorSpec(IndicatorType.MACD_SIGNAL, (fast, slow, signal)))
            return macd_line - signal_line

        # === Aroon ===
        elif t == IndicatorType.AROON_UP:
            period = params[0]
            aroon_up, _, _ = ind.aroon_gpu(self.high, self.low, period)
            return aroon_up

        elif t == IndicatorType.AROON_DOWN:
            period = params[0]
            _, aroon_down, _ = ind.aroon_gpu(self.high, self.low, period)
            return aroon_down

        elif t == IndicatorType.AROON_OSCILLATOR:
            period = params[0]
            aroon_up = self.get(IndicatorSpec(IndicatorType.AROON_UP, (period,)))
            aroon_down = self.get(IndicatorSpec(IndicatorType.AROON_DOWN, (period,)))
            return aroon_up - aroon_down

        # === Keltner ===
        elif t == IndicatorType.KELTNER_MIDDLE:
            ema_period = params[0]
            return self.get(IndicatorSpec(IndicatorType.EMA, (ema_period,)))

        elif t == IndicatorType.KELTNER_UPPER:
            ema_period, atr_period = params[0], params[1]
            mult = params[2] / 100.0 if len(params) > 2 else 2.0
            middle = self.get(IndicatorSpec(IndicatorType.KELTNER_MIDDLE, (ema_period,)))
            atr = self.get(IndicatorSpec(IndicatorType.ATR_WILDER, (atr_period,)))
            return middle + mult * atr

        elif t == IndicatorType.KELTNER_LOWER:
            ema_period, atr_period = params[0], params[1]
            mult = params[2] / 100.0 if len(params) > 2 else 2.0
            middle = self.get(IndicatorSpec(IndicatorType.KELTNER_MIDDLE, (ema_period,)))
            atr = self.get(IndicatorSpec(IndicatorType.ATR_WILDER, (atr_period,)))
            return middle - mult * atr

        # === Stochastic ===
        elif t == IndicatorType.STOCHASTIC_K:
            k_period = params[0]
            return self._compute_stochastic_k(k_period)

        elif t == IndicatorType.STOCHASTIC_D:
            k_period, d_period = params
            stoch_k = self.get(IndicatorSpec(IndicatorType.STOCHASTIC_K, (k_period,)))
            return ind.sma_gpu(stoch_k, d_period)

        # === DMI/ADX ===
        elif t == IndicatorType.DMI_PLUS:
            di_period = params[0]
            plus_di, minus_di, _ = self._compute_dmi_adx(di_period, di_period)
            # Cache minus_di too
            self._cache[IndicatorSpec(IndicatorType.DMI_MINUS, (di_period,))] = minus_di
            return plus_di

        elif t == IndicatorType.DMI_MINUS:
            di_period = params[0]
            # Check if already computed as side effect
            if spec in self._cache:
                return self._cache[spec]
            _, minus_di, _ = self._compute_dmi_adx(di_period, di_period)
            return minus_di

        elif t == IndicatorType.ADX:
            di_period, adx_period = params
            _, _, adx = self._compute_dmi_adx(di_period, adx_period)
            return adx

        else:
            raise NotImplementedError(f"Indicator type not implemented: {t}")

    def _compute_stochastic_k(self, k_period: int) -> cp.ndarray:
        """Compute Stochastic %K."""
        result = cp.full((self.num_symbols, self.num_bars), cp.nan, dtype=cp.float32)

        for i in range(k_period - 1, self.num_bars):
            window_high = self.high[:, i - k_period + 1 : i + 1]
            window_low = self.low[:, i - k_period + 1 : i + 1]

            highest = cp.max(window_high, axis=1)
            lowest = cp.min(window_low, axis=1)

            denom = highest - lowest
            denom = cp.where(denom == 0, 1e-10, denom)

            result[:, i] = 100 * (self.close[:, i] - lowest) / denom

        return result

    def _compute_dmi_adx(
        self, di_period: int, adx_period: int
    ) -> tuple[cp.ndarray, cp.ndarray, cp.ndarray]:
        """Compute DMI (+DI, -DI) and ADX."""
        from . import indicators as ind

        # True Range
        tr = ind.true_range_gpu(self.high, self.low, self.close)
        if tr.ndim == 1:
            tr = tr.reshape(1, -1)

        # Directional Movement
        high_diff = self.high[:, 1:] - self.high[:, :-1]
        low_diff = self.low[:, :-1] - self.low[:, 1:]

        plus_dm = cp.where((high_diff > low_diff) & (high_diff > 0), high_diff, 0)
        minus_dm = cp.where((low_diff > high_diff) & (low_diff > 0), low_diff, 0)

        # Pad to original length
        plus_dm = cp.concatenate([cp.zeros((self.num_symbols, 1)), plus_dm], axis=1)
        minus_dm = cp.concatenate([cp.zeros((self.num_symbols, 1)), minus_dm], axis=1)

        # Smoothed TR and DM using Wilder smoothing
        smoothed_tr = cp.full_like(tr, cp.nan)
        smoothed_plus_dm = cp.full_like(tr, cp.nan)
        smoothed_minus_dm = cp.full_like(tr, cp.nan)

        # Seed with sum of first di_period values
        if self.num_bars >= di_period:
            smoothed_tr[:, di_period - 1] = cp.sum(tr[:, :di_period], axis=1)
            smoothed_plus_dm[:, di_period - 1] = cp.sum(plus_dm[:, :di_period], axis=1)
            smoothed_minus_dm[:, di_period - 1] = cp.sum(minus_dm[:, :di_period], axis=1)

            # Wilder smoothing
            for i in range(di_period, self.num_bars):
                smoothed_tr[:, i] = (
                    smoothed_tr[:, i - 1] - smoothed_tr[:, i - 1] / di_period + tr[:, i]
                )
                smoothed_plus_dm[:, i] = (
                    smoothed_plus_dm[:, i - 1]
                    - smoothed_plus_dm[:, i - 1] / di_period
                    + plus_dm[:, i]
                )
                smoothed_minus_dm[:, i] = (
                    smoothed_minus_dm[:, i - 1]
                    - smoothed_minus_dm[:, i - 1] / di_period
                    + minus_dm[:, i]
                )

        # DI values
        plus_di = 100 * smoothed_plus_dm / (smoothed_tr + 1e-10)
        minus_di = 100 * smoothed_minus_dm / (smoothed_tr + 1e-10)

        # DX
        di_sum = plus_di + minus_di
        di_diff = cp.abs(plus_di - minus_di)
        dx = 100 * di_diff / (di_sum + 1e-10)

        # ADX (smoothed DX)
        adx = cp.full_like(dx, cp.nan)
        if self.num_bars >= di_period + adx_period:
            start_idx = di_period + adx_period - 1
            # Seed with SMA of first adx_period DX values
            adx[:, start_idx] = cp.mean(
                dx[:, di_period - 1 : di_period - 1 + adx_period], axis=1
            )

            # Wilder smoothing
            for i in range(start_idx + 1, self.num_bars):
                adx[:, i] = (adx[:, i - 1] * (adx_period - 1) + dx[:, i]) / adx_period

        return plus_di, minus_di, adx

    def clear(self) -> None:
        """Clear the cache."""
        self._cache.clear()

    def cache_size(self) -> int:
        """Return number of cached indicators."""
        return len(self._cache)

    def memory_usage_bytes(self) -> int:
        """Estimate GPU memory usage of cached indicators."""
        total = 0
        for arr in self._cache.values():
            total += arr.nbytes
        return total
