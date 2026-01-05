"""Quick test of the batched GPU sweep implementation."""

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

print("Testing batched GPU sweep implementation...")
print()

# Test 1: Import check
print("1. Checking imports...")
try:
    from scripts.gpu_sweep.indicator_graph import (
        IndicatorSpec,
        IndicatorType,
        collect_strategy_indicators,
        topological_sort,
    )
    print("   indicator_graph: OK")
except Exception as e:
    print(f"   indicator_graph: FAILED - {e}")
    sys.exit(1)

try:
    from scripts.gpu_sweep.batched_cache import BatchedIndicatorCache
    print("   batched_cache: OK")
except Exception as e:
    print(f"   batched_cache: FAILED - {e}")
    sys.exit(1)

try:
    from scripts.gpu_sweep.signal_batched import compute_signals_batched
    print("   signal_batched: OK")
except Exception as e:
    print(f"   signal_batched: FAILED - {e}")
    sys.exit(1)

print()

# Test 2: Indicator collection
print("2. Testing indicator collection...")
configs = [
    {"fast_period": 10, "slow_period": 50, "ma_type": "sma"},
    {"fast_period": 20, "slow_period": 100, "ma_type": "ema"},
    {"fast_period": 10, "slow_period": 100, "ma_type": "sma"},  # Shares fast_period with first
]
specs = collect_strategy_indicators("ma_crossover", configs)
print(f"   MA Crossover with 3 configs needs {len(specs)} unique indicators:")
for spec in sorted(specs, key=lambda s: (s.type.name, s.params)):
    print(f"      {spec}")
print()

# Test 3: Topological sort
print("3. Testing topological sort...")
supertrend_specs = collect_strategy_indicators("supertrend", [
    {"atr_period": 10, "multiplier": 3.0},
    {"atr_period": 14, "multiplier": 2.0},
])
print(f"   Supertrend with 2 configs needs {len(supertrend_specs)} specs")
sorted_specs = topological_sort(supertrend_specs)
print(f"   After expanding dependencies: {len(sorted_specs)} specs in order:")
for spec in sorted_specs:
    print(f"      {spec}")
print()

# Test 4: Full sweep with batched implementation
print("4. Running full batched sweep (2 symbols)...")
try:
    from scripts.gpu_sweep.config import SweepConfig
    from scripts.gpu_sweep.engine import run_sweep

    config = SweepConfig.from_yaml('configs/gpu_mega_sweep.yaml')
    # Override to test with just 2 symbols
    original_symbols = config.symbols
    config.symbols = ['SPY', 'QQQ']

    print(f"   Symbols: {config.symbols} (overridden from {len(original_symbols)})")
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
    print()

    # Show sample results
    if len(results) > 0:
        print("   Sample results (first 5):")
        print(results.head(5))

except Exception as e:
    import traceback
    print(f"   FAILED: {e}")
    traceback.print_exc()
    sys.exit(1)

print()
print("All tests passed!")
