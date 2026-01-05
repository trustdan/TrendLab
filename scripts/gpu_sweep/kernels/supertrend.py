"""
CUDA RawKernel for Supertrend indicator computation.

Supertrend is a complex stateful indicator that tracks trend direction
with upper/lower bands that adapt based on price action.
"""

import cupy as cp

# Supertrend kernel: one thread per symbol
# Computes ATR, bands, trend direction, and supertrend line all in one pass
_supertrend_kernel_code = r"""
extern "C" __global__
void supertrend_kernel(
    const float* __restrict__ high,         // [num_symbols, num_bars]
    const float* __restrict__ low,          // [num_symbols, num_bars]
    const float* __restrict__ close,        // [num_symbols, num_bars]
    float* __restrict__ supertrend_out,     // [num_symbols, num_bars]
    int* __restrict__ direction_out,        // [num_symbols, num_bars] (1=up, 0=down)
    float* __restrict__ upper_band_out,     // [num_symbols, num_bars]
    float* __restrict__ lower_band_out,     // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int atr_period,
    const float multiplier
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    int offset = symbol * num_bars;
    float alpha = 1.0f / (float)atr_period;
    float one_minus_alpha = 1.0f - alpha;

    // First pass: compute True Range and ATR using Wilder smoothing
    // Also compute basic bands as we go

    // Initialize warmup period to NaN
    for (int i = 0; i < atr_period - 1; i++) {
        supertrend_out[offset + i] = nanf("");
        direction_out[offset + i] = 0;
        upper_band_out[offset + i] = nanf("");
        lower_band_out[offset + i] = nanf("");
    }

    if (num_bars < atr_period) return;

    // Compute TR for all bars
    float tr_sum = 0.0f;
    for (int i = 0; i < atr_period; i++) {
        float hl = high[offset + i] - low[offset + i];
        float tr;
        if (i == 0) {
            tr = hl;
        } else {
            float prev_close = close[offset + i - 1];
            float hpc = fabsf(high[offset + i] - prev_close);
            float lpc = fabsf(low[offset + i] - prev_close);
            tr = fmaxf(fmaxf(hl, hpc), lpc);
        }
        tr_sum += tr;
    }

    // Initial ATR (SMA of first atr_period TRs)
    float atr = tr_sum / (float)atr_period;

    // Initialize bands and trend at first valid bar
    int start = atr_period - 1;
    float hl2 = (high[offset + start] + low[offset + start]) / 2.0f;
    float basic_upper = hl2 + multiplier * atr;
    float basic_lower = hl2 - multiplier * atr;

    float final_upper = basic_upper;
    float final_lower = basic_lower;
    int is_uptrend = 0;  // Start in downtrend
    float st = final_upper;

    upper_band_out[offset + start] = final_upper;
    lower_band_out[offset + start] = final_lower;
    direction_out[offset + start] = is_uptrend;
    supertrend_out[offset + start] = st;

    // Process remaining bars
    for (int i = start + 1; i < num_bars; i++) {
        // Update ATR using Wilder smoothing
        float prev_close = close[offset + i - 1];
        float hl = high[offset + i] - low[offset + i];
        float hpc = fabsf(high[offset + i] - prev_close);
        float lpc = fabsf(low[offset + i] - prev_close);
        float tr = fmaxf(fmaxf(hl, hpc), lpc);
        atr = atr * one_minus_alpha + tr * alpha;

        // Basic bands
        hl2 = (high[offset + i] + low[offset + i]) / 2.0f;
        basic_upper = hl2 + multiplier * atr;
        basic_lower = hl2 - multiplier * atr;

        // Final upper band: use basic if lower than prev or if close broke above prev
        float prev_upper = final_upper;
        if (basic_upper < prev_upper || prev_close > prev_upper) {
            final_upper = basic_upper;
        }
        // else keep prev_upper (final_upper unchanged)

        // Final lower band: use basic if higher than prev or if close broke below prev
        float prev_lower = final_lower;
        if (basic_lower > prev_lower || prev_close < prev_lower) {
            final_lower = basic_lower;
        }
        // else keep prev_lower (final_lower unchanged)

        // Determine trend
        float curr_close = close[offset + i];
        if (is_uptrend) {
            // Was uptrend, check if still uptrend
            is_uptrend = (curr_close >= final_lower) ? 1 : 0;
        } else {
            // Was downtrend, check if switched to uptrend
            is_uptrend = (curr_close > final_upper) ? 1 : 0;
        }

        // Supertrend value
        st = is_uptrend ? final_lower : final_upper;

        // Store results
        upper_band_out[offset + i] = final_upper;
        lower_band_out[offset + i] = final_lower;
        direction_out[offset + i] = is_uptrend;
        supertrend_out[offset + i] = st;
    }
}
"""

_supertrend_kernel = None


def get_supertrend_kernel():
    """Get compiled Supertrend kernel (compiled lazily on first call)."""
    global _supertrend_kernel
    if _supertrend_kernel is None:
        _supertrend_kernel = cp.RawKernel(_supertrend_kernel_code, "supertrend_kernel")
    return _supertrend_kernel


def supertrend_kernel(
    high: cp.ndarray,
    low: cp.ndarray,
    close: cp.ndarray,
    atr_period: int,
    multiplier: float,
) -> tuple[cp.ndarray, cp.ndarray, cp.ndarray, cp.ndarray]:
    """
    Compute Supertrend indicator using CUDA kernel.

    Args:
        high: High prices, shape [num_symbols, num_bars] (float32)
        low: Low prices, same shape
        close: Close prices, same shape
        atr_period: ATR lookback period
        multiplier: ATR multiplier for bands

    Returns:
        Tuple of:
        - supertrend: Supertrend line values
        - is_uptrend: Boolean array (1 = uptrend/bullish, 0 = downtrend)
        - upper_band: Upper band values
        - lower_band: Lower band values
    """
    squeeze = False
    if high.ndim == 1:
        high = high.reshape(1, -1)
        low = low.reshape(1, -1)
        close = close.reshape(1, -1)
        squeeze = True

    high = cp.ascontiguousarray(high.astype(cp.float32))
    low = cp.ascontiguousarray(low.astype(cp.float32))
    close = cp.ascontiguousarray(close.astype(cp.float32))
    num_symbols, num_bars = close.shape

    supertrend_out = cp.empty((num_symbols, num_bars), dtype=cp.float32)
    direction_out = cp.empty((num_symbols, num_bars), dtype=cp.int32)
    upper_band_out = cp.empty((num_symbols, num_bars), dtype=cp.float32)
    lower_band_out = cp.empty((num_symbols, num_bars), dtype=cp.float32)

    if num_bars < atr_period:
        supertrend_out.fill(cp.nan)
        direction_out.fill(0)
        upper_band_out.fill(cp.nan)
        lower_band_out.fill(cp.nan)
        if squeeze:
            return (supertrend_out.squeeze(0), direction_out.squeeze(0).astype(cp.bool_),
                    upper_band_out.squeeze(0), lower_band_out.squeeze(0))
        return supertrend_out, direction_out.astype(cp.bool_), upper_band_out, lower_band_out

    kernel = get_supertrend_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (high, low, close, supertrend_out, direction_out, upper_band_out, lower_band_out,
         num_symbols, num_bars, atr_period, cp.float32(multiplier))
    )

    is_uptrend = direction_out.astype(cp.bool_)

    if squeeze:
        return (supertrend_out.squeeze(0), is_uptrend.squeeze(0),
                upper_band_out.squeeze(0), lower_band_out.squeeze(0))

    return supertrend_out, is_uptrend, upper_band_out, lower_band_out
