"""
CUDA RawKernel for RSI computation.

RSI requires Wilder smoothing of gains and losses, making it a stateful
indicator. This kernel computes everything in one pass per symbol.
"""

import cupy as cp

# RSI kernel: one thread per symbol
# Computes delta, gains, losses, Wilder smoothing, and RSI all in one pass
_rsi_kernel_code = r"""
extern "C" __global__
void rsi_kernel(
    const float* __restrict__ close,    // [num_symbols, num_bars]
    float* __restrict__ result,         // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int period
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    int offset = symbol * num_bars;

    // First (period) bars are NaN (need period for initial SMA)
    for (int i = 0; i <= period; i++) {
        result[offset + i] = nanf("");
    }

    if (num_bars <= period) return;

    // Compute initial average gain and loss (SMA of first period)
    float sum_gain = 0.0f;
    float sum_loss = 0.0f;
    for (int i = 1; i <= period; i++) {
        float delta = close[offset + i] - close[offset + i - 1];
        if (delta > 0) {
            sum_gain += delta;
        } else {
            sum_loss += -delta;
        }
    }

    float avg_gain = sum_gain / (float)period;
    float avg_loss = sum_loss / (float)period;

    // RSI at bar `period`
    float rs = avg_gain / (avg_loss + 1e-10f);
    result[offset + period] = 100.0f - 100.0f / (1.0f + rs);

    // Wilder smoothing for remaining bars
    float alpha = 1.0f / (float)period;
    float one_minus_alpha = 1.0f - alpha;

    for (int i = period + 1; i < num_bars; i++) {
        float delta = close[offset + i] - close[offset + i - 1];
        float gain = delta > 0 ? delta : 0.0f;
        float loss = delta < 0 ? -delta : 0.0f;

        // Wilder smoothing: avg = avg * (1 - 1/period) + value / period
        // Equivalent: avg = (avg * (period - 1) + value) / period
        avg_gain = avg_gain * one_minus_alpha + gain * alpha;
        avg_loss = avg_loss * one_minus_alpha + loss * alpha;

        rs = avg_gain / (avg_loss + 1e-10f);
        result[offset + i] = 100.0f - 100.0f / (1.0f + rs);
    }
}
"""

_rsi_kernel = None


def get_rsi_kernel():
    """Get compiled RSI kernel (compiled lazily on first call)."""
    global _rsi_kernel
    if _rsi_kernel is None:
        _rsi_kernel = cp.RawKernel(_rsi_kernel_code, "rsi_kernel")
    return _rsi_kernel


def rsi_kernel(close: cp.ndarray, period: int) -> cp.ndarray:
    """
    Compute RSI using CUDA kernel.

    Args:
        close: Close prices, shape [num_symbols, num_bars] (float32)
        period: RSI period

    Returns:
        RSI values (0-100). NaN for first `period` bars.
    """
    squeeze = False
    if close.ndim == 1:
        close = close.reshape(1, -1)
        squeeze = True

    close = cp.ascontiguousarray(close.astype(cp.float32))
    num_symbols, num_bars = close.shape

    result = cp.empty_like(close)

    if num_bars <= period:
        result.fill(cp.nan)
        if squeeze:
            result = result.squeeze(0)
        return result

    kernel = get_rsi_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (close, result, num_symbols, num_bars, period)
    )

    if squeeze:
        result = result.squeeze(0)

    return result
