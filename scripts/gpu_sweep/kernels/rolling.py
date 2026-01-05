"""
CUDA RawKernels for rolling window computations (max, min).

These kernels process one symbol per thread, computing rolling operations
in parallel across all symbols. Uses O(n*window) per symbol - acceptable
for typical window sizes (20-250 bars) when parallelized across symbols.
"""

import cupy as cp

# Rolling max kernel: one thread per symbol
_rolling_max_kernel_code = r"""
extern "C" __global__
void rolling_max_kernel(
    const float* __restrict__ data,     // [num_symbols, num_bars]
    float* __restrict__ result,         // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int window
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    int offset = symbol * num_bars;

    // First (window-1) bars are NaN
    for (int i = 0; i < window - 1; i++) {
        result[offset + i] = nanf("");
    }

    // Rolling max for remaining bars
    for (int i = window - 1; i < num_bars; i++) {
        float max_val = data[offset + i - window + 1];
        for (int j = i - window + 2; j <= i; j++) {
            float val = data[offset + j];
            if (val > max_val) max_val = val;
        }
        result[offset + i] = max_val;
    }
}
"""

# Rolling min kernel: one thread per symbol
_rolling_min_kernel_code = r"""
extern "C" __global__
void rolling_min_kernel(
    const float* __restrict__ data,     // [num_symbols, num_bars]
    float* __restrict__ result,         // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int window
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    int offset = symbol * num_bars;

    // First (window-1) bars are NaN
    for (int i = 0; i < window - 1; i++) {
        result[offset + i] = nanf("");
    }

    // Rolling min for remaining bars
    for (int i = window - 1; i < num_bars; i++) {
        float min_val = data[offset + i - window + 1];
        for (int j = i - window + 2; j <= i; j++) {
            float val = data[offset + j];
            if (val < min_val) min_val = val;
        }
        result[offset + i] = min_val;
    }
}
"""

# Donchian-style rolling (uses PRIOR bars only, not including current)
_rolling_max_prior_kernel_code = r"""
extern "C" __global__
void rolling_max_prior_kernel(
    const float* __restrict__ data,     // [num_symbols, num_bars]
    float* __restrict__ result,         // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int lookback                  // Look at [i-lookback, i-1], not including i
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    int offset = symbol * num_bars;

    // First `lookback` bars are NaN (need lookback prior bars)
    for (int i = 0; i < lookback; i++) {
        result[offset + i] = nanf("");
    }

    // Rolling max of prior `lookback` bars
    for (int i = lookback; i < num_bars; i++) {
        float max_val = data[offset + i - lookback];
        for (int j = i - lookback + 1; j < i; j++) {  // Note: j < i, not j <= i
            float val = data[offset + j];
            if (val > max_val) max_val = val;
        }
        result[offset + i] = max_val;
    }
}
"""

_rolling_min_prior_kernel_code = r"""
extern "C" __global__
void rolling_min_prior_kernel(
    const float* __restrict__ data,     // [num_symbols, num_bars]
    float* __restrict__ result,         // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int lookback
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    int offset = symbol * num_bars;

    for (int i = 0; i < lookback; i++) {
        result[offset + i] = nanf("");
    }

    for (int i = lookback; i < num_bars; i++) {
        float min_val = data[offset + i - lookback];
        for (int j = i - lookback + 1; j < i; j++) {
            float val = data[offset + j];
            if (val < min_val) min_val = val;
        }
        result[offset + i] = min_val;
    }
}
"""

# Rolling std kernel for Bollinger Bands
_rolling_std_kernel_code = r"""
extern "C" __global__
void rolling_std_kernel(
    const float* __restrict__ data,     // [num_symbols, num_bars]
    float* __restrict__ result,         // [num_symbols, num_bars]
    const int num_symbols,
    const int num_bars,
    const int window
) {
    int symbol = blockIdx.x * blockDim.x + threadIdx.x;
    if (symbol >= num_symbols) return;

    int offset = symbol * num_bars;

    // First (window-1) bars are NaN
    for (int i = 0; i < window - 1; i++) {
        result[offset + i] = nanf("");
    }

    // Rolling std for remaining bars
    for (int i = window - 1; i < num_bars; i++) {
        // Calculate mean
        float sum = 0.0f;
        for (int j = i - window + 1; j <= i; j++) {
            sum += data[offset + j];
        }
        float mean = sum / (float)window;

        // Calculate variance
        float var_sum = 0.0f;
        for (int j = i - window + 1; j <= i; j++) {
            float diff = data[offset + j] - mean;
            var_sum += diff * diff;
        }
        result[offset + i] = sqrtf(var_sum / (float)window);
    }
}
"""


# Lazy kernel compilation
_rolling_max_k = None
_rolling_min_k = None
_rolling_max_prior_k = None
_rolling_min_prior_k = None
_rolling_std_k = None


def get_rolling_max_kernel():
    global _rolling_max_k
    if _rolling_max_k is None:
        _rolling_max_k = cp.RawKernel(_rolling_max_kernel_code, "rolling_max_kernel")
    return _rolling_max_k


def get_rolling_min_kernel():
    global _rolling_min_k
    if _rolling_min_k is None:
        _rolling_min_k = cp.RawKernel(_rolling_min_kernel_code, "rolling_min_kernel")
    return _rolling_min_k


def get_rolling_max_prior_kernel():
    global _rolling_max_prior_k
    if _rolling_max_prior_k is None:
        _rolling_max_prior_k = cp.RawKernel(_rolling_max_prior_kernel_code, "rolling_max_prior_kernel")
    return _rolling_max_prior_k


def get_rolling_min_prior_kernel():
    global _rolling_min_prior_k
    if _rolling_min_prior_k is None:
        _rolling_min_prior_k = cp.RawKernel(_rolling_min_prior_kernel_code, "rolling_min_prior_kernel")
    return _rolling_min_prior_k


def get_rolling_std_kernel():
    global _rolling_std_k
    if _rolling_std_k is None:
        _rolling_std_k = cp.RawKernel(_rolling_std_kernel_code, "rolling_std_kernel")
    return _rolling_std_k


def rolling_max_kernel(data: cp.ndarray, window: int) -> cp.ndarray:
    """
    Compute rolling maximum using CUDA kernel.

    Args:
        data: Input data, shape [num_symbols, num_bars] (float32)
        window: Lookback window

    Returns:
        Rolling max values. NaN for first (window-1) bars.
    """
    squeeze = False
    if data.ndim == 1:
        data = data.reshape(1, -1)
        squeeze = True

    data = cp.ascontiguousarray(data.astype(cp.float32))
    num_symbols, num_bars = data.shape

    result = cp.empty_like(data)

    if num_bars < window:
        result.fill(cp.nan)
        if squeeze:
            result = result.squeeze(0)
        return result

    kernel = get_rolling_max_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (data, result, num_symbols, num_bars, window)
    )

    if squeeze:
        result = result.squeeze(0)

    return result


def rolling_min_kernel(data: cp.ndarray, window: int) -> cp.ndarray:
    """
    Compute rolling minimum using CUDA kernel.

    Args:
        data: Input data, shape [num_symbols, num_bars] (float32)
        window: Lookback window

    Returns:
        Rolling min values. NaN for first (window-1) bars.
    """
    squeeze = False
    if data.ndim == 1:
        data = data.reshape(1, -1)
        squeeze = True

    data = cp.ascontiguousarray(data.astype(cp.float32))
    num_symbols, num_bars = data.shape

    result = cp.empty_like(data)

    if num_bars < window:
        result.fill(cp.nan)
        if squeeze:
            result = result.squeeze(0)
        return result

    kernel = get_rolling_min_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (data, result, num_symbols, num_bars, window)
    )

    if squeeze:
        result = result.squeeze(0)

    return result


def donchian_upper_kernel(high: cp.ndarray, lookback: int) -> cp.ndarray:
    """
    Compute Donchian upper band (rolling max of PRIOR bars only).

    Args:
        high: High prices, shape [num_symbols, num_bars]
        lookback: Number of prior bars to look at

    Returns:
        Upper band. NaN for first `lookback` bars.
    """
    squeeze = False
    if high.ndim == 1:
        high = high.reshape(1, -1)
        squeeze = True

    high = cp.ascontiguousarray(high.astype(cp.float32))
    num_symbols, num_bars = high.shape

    result = cp.empty_like(high)

    if num_bars <= lookback:
        result.fill(cp.nan)
        if squeeze:
            result = result.squeeze(0)
        return result

    kernel = get_rolling_max_prior_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (high, result, num_symbols, num_bars, lookback)
    )

    if squeeze:
        result = result.squeeze(0)

    return result


def donchian_lower_kernel(low: cp.ndarray, lookback: int) -> cp.ndarray:
    """
    Compute Donchian lower band (rolling min of PRIOR bars only).

    Args:
        low: Low prices, shape [num_symbols, num_bars]
        lookback: Number of prior bars to look at

    Returns:
        Lower band. NaN for first `lookback` bars.
    """
    squeeze = False
    if low.ndim == 1:
        low = low.reshape(1, -1)
        squeeze = True

    low = cp.ascontiguousarray(low.astype(cp.float32))
    num_symbols, num_bars = low.shape

    result = cp.empty_like(low)

    if num_bars <= lookback:
        result.fill(cp.nan)
        if squeeze:
            result = result.squeeze(0)
        return result

    kernel = get_rolling_min_prior_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (low, result, num_symbols, num_bars, lookback)
    )

    if squeeze:
        result = result.squeeze(0)

    return result


def rolling_std_kernel(data: cp.ndarray, window: int) -> cp.ndarray:
    """
    Compute rolling standard deviation using CUDA kernel.

    Args:
        data: Input data, shape [num_symbols, num_bars]
        window: Window size

    Returns:
        Rolling std. NaN for first (window-1) bars.
    """
    squeeze = False
    if data.ndim == 1:
        data = data.reshape(1, -1)
        squeeze = True

    data = cp.ascontiguousarray(data.astype(cp.float32))
    num_symbols, num_bars = data.shape

    result = cp.empty_like(data)

    if num_bars < window:
        result.fill(cp.nan)
        if squeeze:
            result = result.squeeze(0)
        return result

    kernel = get_rolling_std_kernel()
    threads_per_block = 256
    blocks = (num_symbols + threads_per_block - 1) // threads_per_block

    kernel(
        (blocks,), (threads_per_block,),
        (data, result, num_symbols, num_bars, window)
    )

    if squeeze:
        result = result.squeeze(0)

    return result
