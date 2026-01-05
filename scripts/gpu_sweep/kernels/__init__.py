"""
CUDA RawKernels for stateful indicator computation.

These kernels eliminate Python bar-by-bar loops by running one CUDA thread
per symbol, with each thread handling the sequential computation for its symbol.
This parallelizes across the symbol dimension while handling the inherent
sequential nature of stateful indicators (EMA, ATR, RSI, Supertrend).
"""

from .ema import ema_kernel, wilder_smooth_kernel
from .rolling import rolling_max_kernel, rolling_min_kernel
from .rsi import rsi_kernel
from .supertrend import supertrend_kernel

__all__ = [
    "ema_kernel",
    "wilder_smooth_kernel",
    "rolling_max_kernel",
    "rolling_min_kernel",
    "rsi_kernel",
    "supertrend_kernel",
]
