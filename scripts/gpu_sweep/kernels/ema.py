"""
CUDA RawKernels for exponential moving average computations.

These kernels process one symbol per thread, computing the sequential
bar-by-bar EMA/Wilder smoothing in parallel across all symbols.
"""

import cupy as cp

# EMA kernel: one thread per symbol
# Each thread computes EMA for all bars in its row
_ema_kernel_code = r"""
extern "C" __global__
void ema_kernel(
    const float* __restrict__ close,    // [num_symbols, num_bars]
    float* __restrict__ result,         // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int window,
    const float k                       // 2.0 / (window + 1)
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    // Row offset
    int offset = symbol * num_bars;

    // Initialize first (window-1) values to NaN
    for (int i = 0; i < window - 1; i++) {
        result[offset + i] = nanf("");
    }

    if (num_bars < window) return;

    // Seed with SMA of first window bars
    float sma = 0.0f;
    for (int i = 0; i < window; i++) {
        sma += close[offset + i];
    }
    sma /= (float)window;
    result[offset + window - 1] = sma;

    // EMA[t] = close[t] * k + EMA[t-1] * (1 - k)
    float ema = sma;
    float one_minus_k = 1.0f - k;
    for (int i = window; i < num_bars; i++) {
        ema = close[offset + i] * k + ema * one_minus_k;
        result[offset + i] = ema;
    }
}
"""

# Wilder smoothing kernel (for ATR, RSI)
# alpha = 1/window, smoothed[t] = smoothed[t-1] * (1-alpha) + value[t] * alpha
_wilder_kernel_code = r"""
extern "C" __global__
void wilder_smooth_kernel(
    const float* __restrict__ values,   // [num_symbols, num_bars] (e.g., TR for ATR)
    float* __restrict__ result,         // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int window
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    int offset = symbol * num_bars;
    float alpha = 1.0f / (float)window;
    float one_minus_alpha = 1.0f - alpha;

    // Initialize first (window-1) values to NaN
    for (int i = 0; i < window - 1; i++) {
        result[offset + i] = nanf("");
    }

    if (num_bars < window) return;

    // Seed with SMA of first window values
    float sma = 0.0f;
    for (int i = 0; i < window; i++) {
        sma += values[offset + i];
    }
    sma /= (float)window;
    result[offset + window - 1] = sma;

    // Wilder smoothing: result[t] = result[t-1] * (1-alpha) + value[t] * alpha
    float smoothed = sma;
    for (int i = window; i < num_bars; i++) {
        smoothed = smoothed * one_minus_alpha + values[offset + i] * alpha;
        result[offset + i] = smoothed;
    }
}
"""

# Compile kernels (lazy - compiled on first use)
_ema_kernel = None
_wilder_kernel = None


def get_ema_kernel():
    """Get compiled EMA kernel (compiled lazily on first call)."""
    global _ema_kernel
    if _ema_kernel is None:
        _ema_kernel = cp.RawKernel(_ema_kernel_code, "ema_kernel")
    return _ema_kernel


def get_wilder_kernel():
    """Get compiled Wilder smoothing kernel (compiled lazily on first call)."""
    global _wilder_kernel
    if _wilder_kernel is None:
        _wilder_kernel = cp.RawKernel(_wilder_kernel_code, "wilder_smooth_kernel")
    return _wilder_kernel


def ema_kernel(close: cp.ndarray, window: int) -> cp.ndarray:
    """
    Compute Exponential Moving Average using CUDA kernel.

    Uses multiplier k = 2 / (window + 1).
    Seeds with SMA of first `window` bars.

    Args:
        close: Close prices, shape [num_symbols, num_bars] (float32)
        window: Lookback window

    Returns:
        EMA values, same shape as input. NaN for first (window-1) bars.
    """
    # Ensure 2D and float32
    squeeze = False
    if close.ndim == 1:
        close = close.reshape(1, -1)
        squeeze = True

    close = cp.ascontiguousarray(close.astype(cp.float32))
    num_symbols, num_bars = close.shape

    result = cp.empty_like(close)

    if num_bars < window:
        result.fill(cp.nan)
        if squeeze:
            result = result.squeeze(0)
        return result

    k = cp.float32(2.0 / (window + 1))

    # Launch kernel: one thread per symbol
    kernel = get_ema_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (close, result, num_symbols, num_bars, window, k)
    )

    if squeeze:
        result = result.squeeze(0)

    return result


def wilder_smooth_kernel(values: cp.ndarray, window: int) -> cp.ndarray:
    """
    Apply Wilder smoothing using CUDA kernel.

    Wilder smoothing: result[t] = result[t-1] * (1 - 1/window) + value[t] / window
    Used for ATR, RSI average gains/losses.

    Args:
        values: Input values, shape [num_symbols, num_bars] (float32)
        window: Smoothing window

    Returns:
        Smoothed values, same shape as input. NaN for first (window-1) bars.
    """
    squeeze = False
    if values.ndim == 1:
        values = values.reshape(1, -1)
        squeeze = True

    values = cp.ascontiguousarray(values.astype(cp.float32))
    num_symbols, num_bars = values.shape

    result = cp.empty_like(values)

    if num_bars < window:
        result.fill(cp.nan)
        if squeeze:
            result = result.squeeze(0)
        return result

    kernel = get_wilder_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (values, result, num_symbols, num_bars, window)
    )

    if squeeze:
        result = result.squeeze(0)

    return result
