"""
GPU-accelerated indicator computation using cuPy.

All indicators mirror the Rust implementations in:
- crates/trendlab-core/src/indicators.rs (sequential)
- crates/trendlab-core/src/indicators_polars.rs (vectorized)

Indicators are computed for batched data with shape [num_symbols, num_bars].
NaN values are used to indicate warmup periods (where indicator is not valid).
"""

import cupy as cp


# ---------------------------------------------------------------------------
# Moving Averages
# ---------------------------------------------------------------------------


def sma_gpu(close: cp.ndarray, window: int) -> cp.ndarray:
    """
    Compute Simple Moving Average.

    Args:
        close: Close prices, shape [num_bars] or [num_symbols, num_bars]
        window: Lookback window

    Returns:
        SMA values, same shape as input. NaN for first (window-1) bars.
    """
    if close.ndim == 1:
        close = close.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = close.shape
    result = cp.full_like(close, cp.nan)

    # Cumulative sum for efficient rolling mean
    cumsum = cp.cumsum(close, axis=1)

    # SMA = (cumsum[i] - cumsum[i-window]) / window for i >= window
    result[:, window - 1:] = (
        cumsum[:, window - 1:] - cp.concatenate([
            cp.zeros((num_symbols, 1)),
            cumsum[:, :-window]
        ], axis=1)
    ) / window

    if squeeze:
        result = result.squeeze(0)

    return result


def ema_gpu(close: cp.ndarray, window: int) -> cp.ndarray:
    """
    Compute Exponential Moving Average.

    Uses multiplier k = 2 / (window + 1).
    Seeds with SMA of first `window` bars.

    Args:
        close: Close prices, shape [num_bars] or [num_symbols, num_bars]
        window: Lookback window

    Returns:
        EMA values, same shape as input. NaN for first (window-1) bars.
    """
    if close.ndim == 1:
        close = close.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = close.shape
    result = cp.full_like(close, cp.nan)

    if num_bars < window:
        if squeeze:
            result = result.squeeze(0)
        return result

    # Multiplier
    k = 2.0 / (window + 1)

    # Seed with SMA of first window bars
    sma_seed = cp.mean(close[:, :window], axis=1)
    result[:, window - 1] = sma_seed

    # EMA[t] = close[t] * k + EMA[t-1] * (1 - k)
    # We need to iterate for the exponential smoothing
    # Use a loop but process all symbols in parallel
    for i in range(window, num_bars):
        result[:, i] = close[:, i] * k + result[:, i - 1] * (1 - k)

    if squeeze:
        result = result.squeeze(0)

    return result


# ---------------------------------------------------------------------------
# Rolling Extremes (for Donchian, 52-week high, etc.)
# ---------------------------------------------------------------------------


def rolling_max_gpu(data: cp.ndarray, window: int) -> cp.ndarray:
    """
    Compute rolling maximum.

    Args:
        data: Input data, shape [num_bars] or [num_symbols, num_bars]
        window: Lookback window

    Returns:
        Rolling max values, same shape as input. NaN for first (window-1) bars.
    """
    if data.ndim == 1:
        data = data.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = data.shape
    result = cp.full_like(data, cp.nan)

    # Use sliding window via as_strided
    # This creates a view with shape [num_symbols, num_bars - window + 1, window]
    for i in range(window - 1, num_bars):
        result[:, i] = cp.max(data[:, i - window + 1:i + 1], axis=1)

    if squeeze:
        result = result.squeeze(0)

    return result


def rolling_min_gpu(data: cp.ndarray, window: int) -> cp.ndarray:
    """
    Compute rolling minimum.

    Args:
        data: Input data, shape [num_bars] or [num_symbols, num_bars]
        window: Lookback window

    Returns:
        Rolling min values, same shape as input. NaN for first (window-1) bars.
    """
    if data.ndim == 1:
        data = data.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = data.shape
    result = cp.full_like(data, cp.nan)

    for i in range(window - 1, num_bars):
        result[:, i] = cp.min(data[:, i - window + 1:i + 1], axis=1)

    if squeeze:
        result = result.squeeze(0)

    return result


def donchian_gpu(
    high: cp.ndarray,
    low: cp.ndarray,
    lookback: int,
) -> tuple[cp.ndarray, cp.ndarray]:
    """
    Compute Donchian Channel (upper and lower bands).

    Note: Computes on PRIOR bars only (not including current bar).
    This matches the Rust implementation.

    Args:
        high: High prices, shape [num_bars] or [num_symbols, num_bars]
        low: Low prices, same shape as high
        lookback: Lookback window

    Returns:
        Tuple of (upper, lower) bands, same shape as input.
        NaN for first `lookback` bars.
    """
    if high.ndim == 1:
        high = high.reshape(1, -1)
        low = low.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = high.shape
    upper = cp.full_like(high, cp.nan)
    lower = cp.full_like(low, cp.nan)

    # Donchian uses PRIOR bars (not including current)
    # So at bar i, we look at bars [i-lookback, i-1]
    for i in range(lookback, num_bars):
        upper[:, i] = cp.max(high[:, i - lookback:i], axis=1)
        lower[:, i] = cp.min(low[:, i - lookback:i], axis=1)

    if squeeze:
        upper = upper.squeeze(0)
        lower = lower.squeeze(0)

    return upper, lower


# ---------------------------------------------------------------------------
# Volatility
# ---------------------------------------------------------------------------


def true_range_gpu(
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
) -> cp.ndarray:
    """
    Compute True Range.

    TR = max(high - low, |high - prev_close|, |low - prev_close|)
    First bar: TR = high - low

    Args:
        high: High prices, shape [num_bars] or [num_symbols, num_bars]
        low: Low prices, same shape
        close: Close prices, same shape

    Returns:
        True Range values, same shape as input.
    """
    if high.ndim == 1:
        high = high.reshape(1, -1)
        low = low.reshape(1, -1)
        close = close.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = high.shape

    # High - Low
    hl = high - low

    # Previous close
    prev_close = cp.roll(close, 1, axis=1)
    prev_close[:, 0] = close[:, 0]  # First bar has no prev

    # |High - Prev Close|
    hpc = cp.abs(high - prev_close)

    # |Low - Prev Close|
    lpc = cp.abs(low - prev_close)

    # True Range = max of all three
    tr = cp.maximum(cp.maximum(hl, hpc), lpc)

    # First bar: just use H-L
    tr[:, 0] = hl[:, 0]

    if squeeze:
        tr = tr.squeeze(0)

    return tr


def atr_gpu(
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
    window: int,
) -> cp.ndarray:
    """
    Compute Average True Range using simple moving average.

    Args:
        high: High prices, shape [num_bars] or [num_symbols, num_bars]
        low: Low prices, same shape
        close: Close prices, same shape
        window: ATR window

    Returns:
        ATR values, same shape as input. NaN for first (window-1) bars.
    """
    tr = true_range_gpu(high, low, close)
    return sma_gpu(tr, window)


def atr_wilder_gpu(
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
    window: int,
) -> cp.ndarray:
    """
    Compute Average True Range using Wilder smoothing.

    Wilder smoothing: ATR[t] = ATR[t-1] * (window-1)/window + TR[t] / window
    First ATR = SMA of first window TRs.

    Args:
        high: High prices, shape [num_bars] or [num_symbols, num_bars]
        low: Low prices, same shape
        close: Close prices, same shape
        window: ATR window

    Returns:
        ATR values, same shape as input. NaN for first (window-1) bars.
    """
    if high.ndim == 1:
        high = high.reshape(1, -1)
        low = low.reshape(1, -1)
        close = close.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    tr = true_range_gpu(high, low, close)
    if tr.ndim == 1:
        tr = tr.reshape(1, -1)

    num_symbols, num_bars = tr.shape
    result = cp.full_like(tr, cp.nan)

    if num_bars < window:
        if squeeze:
            result = result.squeeze(0)
        return result

    # Seed with SMA of first window TRs
    result[:, window - 1] = cp.mean(tr[:, :window], axis=1)

    # Wilder smoothing
    alpha = 1.0 / window
    for i in range(window, num_bars):
        result[:, i] = result[:, i - 1] * (1 - alpha) + tr[:, i] * alpha

    if squeeze:
        result = result.squeeze(0)

    return result


# ---------------------------------------------------------------------------
# Supertrend
# ---------------------------------------------------------------------------


def supertrend_gpu(
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
    atr_period: int,
    multiplier: float,
) -> tuple[cp.ndarray, cp.ndarray, cp.ndarray, cp.ndarray]:
    """
    Compute Supertrend indicator.

    Args:
        high: High prices, shape [num_bars] or [num_symbols, num_bars]
        low: Low prices, same shape
        close: Close prices, same shape
        atr_period: ATR lookback period
        multiplier: ATR multiplier for bands

    Returns:
        Tuple of:
        - supertrend: Supertrend line values
        - is_uptrend: Boolean array (True = uptrend/bullish)
        - upper_band: Upper band values
        - lower_band: Lower band values
    """
    if high.ndim == 1:
        high = high.reshape(1, -1)
        low = low.reshape(1, -1)
        close = close.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = high.shape

    # Compute ATR
    atr = atr_wilder_gpu(high, low, close, atr_period)
    if atr.ndim == 1:
        atr = atr.reshape(1, -1)

    # Basic bands: HL2 +/- multiplier * ATR
    hl2 = (high + low) / 2
    basic_upper = hl2 + multiplier * atr
    basic_lower = hl2 - multiplier * atr

    # Initialize final bands and trend
    upper_band = cp.full_like(close, cp.nan)
    lower_band = cp.full_like(close, cp.nan)
    supertrend = cp.full_like(close, cp.nan)
    is_uptrend = cp.zeros((num_symbols, num_bars), dtype=cp.bool_)

    # First valid bar
    start = atr_period - 1
    if start < num_bars:
        upper_band[:, start] = basic_upper[:, start]
        lower_band[:, start] = basic_lower[:, start]
        supertrend[:, start] = upper_band[:, start]
        is_uptrend[:, start] = False

    # Iterate through bars
    for i in range(start + 1, num_bars):
        # Update final upper band
        # If basic_upper < prev_upper or prev_close > prev_upper: use basic
        # Else: use prev_upper
        use_basic_upper = (basic_upper[:, i] < upper_band[:, i - 1]) | (
            close[:, i - 1] > upper_band[:, i - 1]
        )
        upper_band[:, i] = cp.where(
            use_basic_upper,
            basic_upper[:, i],
            upper_band[:, i - 1],
        )

        # Update final lower band
        # If basic_lower > prev_lower or prev_close < prev_lower: use basic
        # Else: use prev_lower
        use_basic_lower = (basic_lower[:, i] > lower_band[:, i - 1]) | (
            close[:, i - 1] < lower_band[:, i - 1]
        )
        lower_band[:, i] = cp.where(
            use_basic_lower,
            basic_lower[:, i],
            lower_band[:, i - 1],
        )

        # Determine trend
        # Uptrend if: prev was uptrend and close >= lower_band
        #          or: prev was downtrend and close > upper_band
        was_uptrend = is_uptrend[:, i - 1]
        is_uptrend[:, i] = cp.where(
            was_uptrend,
            close[:, i] >= lower_band[:, i],
            close[:, i] > upper_band[:, i],
        )

        # Supertrend value
        supertrend[:, i] = cp.where(
            is_uptrend[:, i],
            lower_band[:, i],
            upper_band[:, i],
        )

    if squeeze:
        supertrend = supertrend.squeeze(0)
        is_uptrend = is_uptrend.squeeze(0)
        upper_band = upper_band.squeeze(0)
        lower_band = lower_band.squeeze(0)

    return supertrend, is_uptrend, upper_band, lower_band


# ---------------------------------------------------------------------------
# Bollinger Bands
# ---------------------------------------------------------------------------


def bollinger_bands_gpu(
    close: cp.ndarray,
    period: int,
    std_mult: float,
) -> tuple[cp.ndarray, cp.ndarray, cp.ndarray, cp.ndarray]:
    """
    Compute Bollinger Bands.

    Args:
        close: Close prices, shape [num_bars] or [num_symbols, num_bars]
        period: SMA period
        std_mult: Standard deviation multiplier

    Returns:
        Tuple of (middle, upper, lower, bandwidth)
    """
    if close.ndim == 1:
        close = close.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = close.shape

    # Middle band (SMA)
    middle = sma_gpu(close, period)
    if middle.ndim == 1:
        middle = middle.reshape(1, -1)

    # Rolling standard deviation
    std = cp.full_like(close, cp.nan)
    for i in range(period - 1, num_bars):
        window_data = close[:, i - period + 1:i + 1]
        std[:, i] = cp.std(window_data, axis=1)

    # Upper and lower bands
    upper = middle + std_mult * std
    lower = middle - std_mult * std

    # Bandwidth: (upper - lower) / middle
    bandwidth = (upper - lower) / (middle + 1e-10)

    if squeeze:
        middle = middle.squeeze(0)
        upper = upper.squeeze(0)
        lower = lower.squeeze(0)
        bandwidth = bandwidth.squeeze(0)

    return middle, upper, lower, bandwidth


# ---------------------------------------------------------------------------
# RSI
# ---------------------------------------------------------------------------


def rsi_gpu(close: cp.ndarray, period: int) -> cp.ndarray:
    """
    Compute Relative Strength Index.

    Args:
        close: Close prices, shape [num_bars] or [num_symbols, num_bars]
        period: RSI period

    Returns:
        RSI values (0-100), same shape as input. NaN for first `period` bars.
    """
    if close.ndim == 1:
        close = close.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = close.shape

    # Price changes
    delta = cp.diff(close, axis=1)

    # Separate gains and losses
    gains = cp.where(delta > 0, delta, 0)
    losses = cp.where(delta < 0, -delta, 0)

    # Pad to maintain shape
    gains = cp.concatenate([cp.zeros((num_symbols, 1)), gains], axis=1)
    losses = cp.concatenate([cp.zeros((num_symbols, 1)), losses], axis=1)

    # Wilder smoothed averages
    avg_gain = cp.full_like(close, cp.nan)
    avg_loss = cp.full_like(close, cp.nan)

    # Seed with SMA
    if num_bars >= period:
        avg_gain[:, period] = cp.mean(gains[:, 1:period + 1], axis=1)
        avg_loss[:, period] = cp.mean(losses[:, 1:period + 1], axis=1)

        # Wilder smoothing
        for i in range(period + 1, num_bars):
            avg_gain[:, i] = (avg_gain[:, i - 1] * (period - 1) + gains[:, i]) / period
            avg_loss[:, i] = (avg_loss[:, i - 1] * (period - 1) + losses[:, i]) / period

    # RSI = 100 - 100 / (1 + RS)
    # RS = avg_gain / avg_loss
    rs = avg_gain / (avg_loss + 1e-10)
    rsi = 100 - 100 / (1 + rs)

    if squeeze:
        rsi = rsi.squeeze(0)

    return rsi


# ---------------------------------------------------------------------------
# MACD
# ---------------------------------------------------------------------------


def macd_gpu(
    close: cp.ndarray,
    fast_period: int,
    slow_period: int,
    signal_period: int,
) -> tuple[cp.ndarray, cp.ndarray, cp.ndarray]:
    """
    Compute MACD (Moving Average Convergence Divergence).

    Args:
        close: Close prices, shape [num_bars] or [num_symbols, num_bars]
        fast_period: Fast EMA period
        slow_period: Slow EMA period
        signal_period: Signal line EMA period

    Returns:
        Tuple of (macd_line, signal_line, histogram)
    """
    # MACD line = fast EMA - slow EMA
    fast_ema = ema_gpu(close, fast_period)
    slow_ema = ema_gpu(close, slow_period)
    macd_line = fast_ema - slow_ema

    # Signal line = EMA of MACD line
    signal_line = ema_gpu(macd_line, signal_period)

    # Histogram = MACD - Signal
    histogram = macd_line - signal_line

    return macd_line, signal_line, histogram


# ---------------------------------------------------------------------------
# Aroon
# ---------------------------------------------------------------------------


def aroon_gpu(
    high: cp.ndarray,
    low: cp.ndarray,
    period: int,
) -> tuple[cp.ndarray, cp.ndarray, cp.ndarray]:
    """
    Compute Aroon indicator.

    Args:
        high: High prices, shape [num_bars] or [num_symbols, num_bars]
        low: Low prices, same shape
        period: Lookback period

    Returns:
        Tuple of (aroon_up, aroon_down, aroon_oscillator)
    """
    if high.ndim == 1:
        high = high.reshape(1, -1)
        low = low.reshape(1, -1)
        squeeze = True
    else:
        squeeze = False

    num_symbols, num_bars = high.shape

    aroon_up = cp.full_like(high, cp.nan)
    aroon_down = cp.full_like(low, cp.nan)

    for i in range(period, num_bars):
        window_high = high[:, i - period:i + 1]
        window_low = low[:, i - period:i + 1]

        # Bars since highest high
        high_idx = cp.argmax(window_high, axis=1)
        bars_since_high = period - high_idx

        # Bars since lowest low
        low_idx = cp.argmin(window_low, axis=1)
        bars_since_low = period - low_idx

        # Aroon Up = ((period - bars_since_high) / period) * 100
        aroon_up[:, i] = ((period - bars_since_high) / period) * 100

        # Aroon Down = ((period - bars_since_low) / period) * 100
        aroon_down[:, i] = ((period - bars_since_low) / period) * 100

    aroon_oscillator = aroon_up - aroon_down

    if squeeze:
        aroon_up = aroon_up.squeeze(0)
        aroon_down = aroon_down.squeeze(0)
        aroon_oscillator = aroon_oscillator.squeeze(0)

    return aroon_up, aroon_down, aroon_oscillator


# ---------------------------------------------------------------------------
# Indicator Cache
# ---------------------------------------------------------------------------


class IndicatorCache:
    """
    Cache for computed indicators to avoid recomputation.

    Mirrors Rust IndicatorCache from crates/trendlab-core/src/indicator_cache.rs.
    Indicators are keyed by (IndicatorKey, params) tuple.
    """

    def __init__(
        self,
        high: cp.ndarray,
        low: cp.ndarray,
        close: cp.ndarray,
        volume: cp.ndarray | None = None,
    ):
        """
        Initialize cache with OHLCV data.

        Args:
            high: High prices, shape [num_symbols, num_bars]
            low: Low prices, same shape
            close: Close prices, same shape
            volume: Volume (optional), same shape
        """
        self.high = high
        self.low = low
        self.close = close
        self.volume = volume
        self._cache: dict[tuple, cp.ndarray] = {}

    def get_or_compute(
        self,
        key: str,
        params: tuple,
        compute_fn,
    ) -> cp.ndarray:
        """
        Get cached indicator or compute and cache it.

        Args:
            key: Indicator key (e.g., 'sma', 'ema', 'atr_wilder')
            params: Hashable params tuple (e.g., (window,))
            compute_fn: Function to compute indicator if not cached

        Returns:
            Computed or cached indicator values
        """
        cache_key = (key, params)
        if cache_key not in self._cache:
            self._cache[cache_key] = compute_fn()
        return self._cache[cache_key]

    def sma(self, window: int) -> cp.ndarray:
        """Get SMA with caching."""
        return self.get_or_compute(
            "sma", (window,), lambda: sma_gpu(self.close, window)
        )

    def ema(self, window: int) -> cp.ndarray:
        """Get EMA with caching."""
        return self.get_or_compute(
            "ema", (window,), lambda: ema_gpu(self.close, window)
        )

    def atr_wilder(self, window: int) -> cp.ndarray:
        """Get Wilder ATR with caching."""
        return self.get_or_compute(
            "atr_wilder",
            (window,),
            lambda: atr_wilder_gpu(self.high, self.low, self.close, window),
        )

    def donchian(self, lookback: int) -> tuple[cp.ndarray, cp.ndarray]:
        """Get Donchian channel with caching."""
        upper = self.get_or_compute(
            "donchian_upper",
            (lookback,),
            lambda: donchian_gpu(self.high, self.low, lookback)[0],
        )
        lower = self.get_or_compute(
            "donchian_lower",
            (lookback,),
            lambda: donchian_gpu(self.high, self.low, lookback)[1],
        )
        return upper, lower

    def rolling_max_high(self, window: int) -> cp.ndarray:
        """Get rolling max of high with caching."""
        return self.get_or_compute(
            "rolling_max_high",
            (window,),
            lambda: rolling_max_gpu(self.high, window),
        )

    def rsi(self, period: int) -> cp.ndarray:
        """Get RSI with caching."""
        return self.get_or_compute(
            "rsi", (period,), lambda: rsi_gpu(self.close, period)
        )

    def clear(self):
        """Clear the cache."""
        self._cache.clear()
