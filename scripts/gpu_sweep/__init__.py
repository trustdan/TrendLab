"""
GPU Mega-Sweep: GPU-accelerated parameter sweep for TrendLab.

This package provides GPU-accelerated backtesting using cuPy for massive
parameter sweeps (100k+ configs). It mirrors TrendLab's Rust abstractions
for portability of learnings.

Architecture:
- Polars (CPU): Data loading, config generation, results export
- cuPy (GPU): Indicators, signals, position scan, metrics

Usage:
    python -m gpu_sweep --config configs/mega_sweep.yaml
    python -m gpu_sweep --dry-run --config configs/mega_sweep.yaml
    python -m gpu_sweep gpu-info
"""

# CRITICAL: Add CUDA DLL directories BEFORE importing cupy
# This fixes "nvrtc64_120_0.dll not found" errors on Windows
import os
import sys

if sys.platform == "win32":
    # Find nvidia package DLLs in site-packages
    _site_packages = os.path.dirname(os.path.dirname(os.path.dirname(__file__)))
    if "site-packages" not in _site_packages:
        # Try absolute path if relative doesn't work
        import site
        for sp in site.getsitepackages():
            if os.path.exists(sp):
                _site_packages = sp
                break

    _cuda_dll_dirs = [
        os.path.join(_site_packages, "nvidia", "cuda_nvrtc", "bin"),
        os.path.join(_site_packages, "nvidia", "cuda_runtime", "bin"),
        os.path.join(_site_packages, "nvidia", "cublas", "bin"),
        os.path.join(_site_packages, "nvidia", "cufft", "bin"),
        os.path.join(_site_packages, "nvidia", "curand", "bin"),
        os.path.join(_site_packages, "nvidia", "cusolver", "bin"),
        os.path.join(_site_packages, "nvidia", "cusparse", "bin"),
    ]

    for _dll_dir in _cuda_dll_dirs:
        if os.path.exists(_dll_dir):
            os.add_dll_directory(_dll_dir)

__version__ = "0.1.0"

# Core configuration
from .config import (
    BacktestConfig,
    DataConfig,
    SweepConfig,
    StrategyGridConfig,
    STRATEGY_PARAMS_REGISTRY,
)

# Data loading
from .data import (
    scan_symbol_parquet,
    load_symbol_to_arrays,
    load_multiple_symbols_to_arrays,
)

# Engine
from .engine import run_sweep, estimate_batch_size

# Results
from .results import (
    write_results,
    load_results,
    filter_results,
    top_n_by_metric,
    strategy_summary,
    symbol_summary,
    robustness_analysis,
    generate_report,
)

__all__ = [
    # Version
    "__version__",
    # Config
    "BacktestConfig",
    "DataConfig",
    "SweepConfig",
    "StrategyGridConfig",
    "STRATEGY_PARAMS_REGISTRY",
    # Data
    "scan_symbol_parquet",
    "load_symbol_to_arrays",
    "load_multiple_symbols_to_arrays",
    # Engine
    "run_sweep",
    "estimate_batch_size",
    # Results
    "write_results",
    "load_results",
    "filter_results",
    "top_n_by_metric",
    "strategy_summary",
    "symbol_summary",
    "robustness_analysis",
    "generate_report",
]
