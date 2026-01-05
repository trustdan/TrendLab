"""Test CUDA RawKernels for indicator computation."""

import os
import sys

# Add project root to path
project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, project_root)

import ctypes
import site
import time

# Load CUDA NVRTC DLLs
for sp in site.getsitepackages():
    nvrtc_bin = os.path.join(sp, 'nvidia', 'cuda_nvrtc', 'bin')
    if os.path.exists(nvrtc_bin):
        os.add_dll_directory(nvrtc_bin)
        try:
            ctypes.WinDLL(os.path.join(nvrtc_bin, 'nvrtc-builtins64_129.dll'))
            ctypes.WinDLL(os.path.join(nvrtc_bin, 'nvrtc64_120_0.dll'))
        except Exception as e:
            print(f"Warning: Could not load NVRTC DLLs: {e}")
        break

print("Testing CUDA RawKernels for indicator computation...")
print()

import cupy as cp
import numpy as np

# Test data: 10 symbols, 5000 bars (realistic sweep data)
np.random.seed(42)
num_symbols = 10
num_bars = 5000

# Generate realistic price data
base_prices = np.random.uniform(50, 500, num_symbols)
returns = np.random.randn(num_symbols, num_bars) * 0.02
close_np = base_prices[:, np.newaxis] * np.exp(np.cumsum(returns, axis=1))
high_np = close_np * (1 + np.abs(np.random.randn(num_symbols, num_bars) * 0.01))
low_np = close_np * (1 - np.abs(np.random.randn(num_symbols, num_bars) * 0.01))

close = cp.asarray(close_np, dtype=cp.float32)
high = cp.asarray(high_np, dtype=cp.float32)
low = cp.asarray(low_np, dtype=cp.float32)

print(f"Test data: {num_symbols} symbols, {num_bars} bars")
print()

# Test 1: EMA kernel
print("1. Testing EMA kernel...")
from scripts.gpu_sweep.kernels.ema import ema_kernel, wilder_smooth_kernel
from scripts.gpu_sweep import indicators as ind

# Warm up (kernel compilation)
start = time.time()
ema_k = ema_kernel(close, 20)
cp.cuda.Stream.null.synchronize()
compile_time = time.time() - start
print(f"   First call (incl. compile): {compile_time*1000:.1f}ms")

# Benchmark kernel
iterations = 100
start = time.time()
for _ in range(iterations):
    ema_k = ema_kernel(close, 20)
cp.cuda.Stream.null.synchronize()
kernel_time = (time.time() - start) / iterations * 1000
print(f"   Kernel: {kernel_time:.3f}ms per call")

# Benchmark old Python loop version
start = time.time()
for _ in range(iterations):
    ema_old = ind.ema_gpu(close, 20)
cp.cuda.Stream.null.synchronize()
old_time = (time.time() - start) / iterations * 1000
print(f"   Python loop: {old_time:.3f}ms per call")
print(f"   Speedup: {old_time/kernel_time:.1f}x")

# Verify correctness
diff = cp.abs(ema_k - ema_old)
max_diff = float(cp.nanmax(diff))
print(f"   Max difference: {max_diff:.2e}")
print()

# Test 2: Rolling max kernel
print("2. Testing rolling max kernel...")
from scripts.gpu_sweep.kernels.rolling import rolling_max_kernel

# Warm up
start = time.time()
rmax_k = rolling_max_kernel(high, 50)
cp.cuda.Stream.null.synchronize()
compile_time = time.time() - start
print(f"   First call (incl. compile): {compile_time*1000:.1f}ms")

# Benchmark
start = time.time()
for _ in range(iterations):
    rmax_k = rolling_max_kernel(high, 50)
cp.cuda.Stream.null.synchronize()
kernel_time = (time.time() - start) / iterations * 1000
print(f"   Kernel: {kernel_time:.3f}ms per call")

start = time.time()
for _ in range(iterations):
    rmax_old = ind.rolling_max_gpu(high, 50)
cp.cuda.Stream.null.synchronize()
old_time = (time.time() - start) / iterations * 1000
print(f"   Python loop: {old_time:.3f}ms per call")
print(f"   Speedup: {old_time/kernel_time:.1f}x")

diff = cp.abs(rmax_k - rmax_old)
max_diff = float(cp.nanmax(diff))
print(f"   Max difference: {max_diff:.2e}")
print()

# Test 3: Wilder smoothing kernel (ATR)
print("3. Testing Wilder smoothing kernel (for ATR)...")
tr = ind.true_range_gpu(high, low, close)

# Warm up
start = time.time()
atr_k = wilder_smooth_kernel(tr, 14)
cp.cuda.Stream.null.synchronize()
compile_time = time.time() - start
print(f"   First call (incl. compile): {compile_time*1000:.1f}ms")

# Benchmark
start = time.time()
for _ in range(iterations):
    atr_k = wilder_smooth_kernel(tr, 14)
cp.cuda.Stream.null.synchronize()
kernel_time = (time.time() - start) / iterations * 1000
print(f"   Kernel: {kernel_time:.3f}ms per call")

start = time.time()
for _ in range(iterations):
    atr_old = ind.atr_wilder_gpu(high, low, close, 14)
cp.cuda.Stream.null.synchronize()
old_time = (time.time() - start) / iterations * 1000
print(f"   Python loop: {old_time:.3f}ms per call")
print(f"   Speedup: {old_time/kernel_time:.1f}x")

diff = cp.abs(atr_k - atr_old)
max_diff = float(cp.nanmax(diff))
print(f"   Max difference: {max_diff:.2e}")
print()

# Test 4: RSI kernel
print("4. Testing RSI kernel...")
from scripts.gpu_sweep.kernels.rsi import rsi_kernel

# Warm up
start = time.time()
rsi_k = rsi_kernel(close, 14)
cp.cuda.Stream.null.synchronize()
compile_time = time.time() - start
print(f"   First call (incl. compile): {compile_time*1000:.1f}ms")

# Benchmark
start = time.time()
for _ in range(iterations):
    rsi_k = rsi_kernel(close, 14)
cp.cuda.Stream.null.synchronize()
kernel_time = (time.time() - start) / iterations * 1000
print(f"   Kernel: {kernel_time:.3f}ms per call")

start = time.time()
for _ in range(iterations):
    rsi_old = ind.rsi_gpu(close, 14)
cp.cuda.Stream.null.synchronize()
old_time = (time.time() - start) / iterations * 1000
print(f"   Python loop: {old_time:.3f}ms per call")
print(f"   Speedup: {old_time/kernel_time:.1f}x")

diff = cp.abs(rsi_k - rsi_old)
max_diff = float(cp.nanmax(diff))
print(f"   Max difference: {max_diff:.2e}")
print()

# Test 5: Rolling std kernel
print("5. Testing rolling std kernel...")
from scripts.gpu_sweep.kernels.rolling import rolling_std_kernel

# Warm up
start = time.time()
std_k = rolling_std_kernel(close, 20)
cp.cuda.Stream.null.synchronize()
compile_time = time.time() - start
print(f"   First call (incl. compile): {compile_time*1000:.1f}ms")

# Benchmark
start = time.time()
for _ in range(iterations):
    std_k = rolling_std_kernel(close, 20)
cp.cuda.Stream.null.synchronize()
kernel_time = (time.time() - start) / iterations * 1000
print(f"   Kernel: {kernel_time:.3f}ms per call")

# Old version uses loop in bollinger_bands_gpu
# Let's compare with a direct loop
def rolling_std_loop(data, window):
    result = cp.full_like(data, cp.nan)
    for i in range(window - 1, data.shape[1]):
        window_data = data[:, i - window + 1:i + 1]
        result[:, i] = cp.std(window_data, axis=1)
    return result

start = time.time()
for _ in range(iterations):
    std_old = rolling_std_loop(close, 20)
cp.cuda.Stream.null.synchronize()
old_time = (time.time() - start) / iterations * 1000
print(f"   Python loop: {old_time:.3f}ms per call")
print(f"   Speedup: {old_time/kernel_time:.1f}x")

diff = cp.abs(std_k - std_old)
max_diff = float(cp.nanmax(diff))
print(f"   Max difference: {max_diff:.2e}")
print()

# Test 6: Full batched sweep with kernels
print("6. Running full batched sweep with kernels...")
try:
    from scripts.gpu_sweep.config import SweepConfig
    from scripts.gpu_sweep.engine import run_sweep

    config = SweepConfig.from_yaml('configs/gpu_mega_sweep.yaml')
    config.symbols = ['SPY', 'QQQ']  # Override for quick test

    print(f"   Symbols: {config.symbols}")
    print(f"   Strategies: {list(config.strategies.keys())}")

    start = time.time()

    def progress(msg):
        elapsed = time.time() - start
        print(f"   [{elapsed:6.1f}s] {msg}")

    results = run_sweep(config, progress_callback=progress)

    elapsed = time.time() - start
    print()
    print(f"   COMPLETED in {elapsed:.1f}s")
    print(f"   Results: {len(results)} rows")

    if len(results) > 0:
        print()
        print("   Sample results (top 5 by Sharpe):")
        top = results.nlargest(5, 'sharpe')
        print(top[['symbol', 'strategy', 'sharpe', 'cagr', 'max_drawdown']].to_string())

except Exception as e:
    import traceback
    print(f"   FAILED: {e}")
    traceback.print_exc()

print()
print("All kernel tests completed!")
