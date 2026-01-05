"""
Entry point for running gpu_sweep as a module.

Usage:
    python -m gpu_sweep --help
    python -m gpu_sweep run --config configs/mega_sweep.yaml
"""

# CRITICAL: Set up CUDA DLLs on Windows BEFORE importing anything that uses cupy
import os
import sys

if sys.platform == "win32":
    import ctypes
    import site
    
    # Find site-packages containing nvidia package
    for sp in site.getsitepackages():
        nvrtc_bin = os.path.join(sp, "nvidia", "cuda_nvrtc", "bin")
        if os.path.exists(nvrtc_bin):
            # Add DLL directory
            os.add_dll_directory(nvrtc_bin)
            
            # Preload builtins FIRST, then nvrtc
            builtins_path = os.path.join(nvrtc_bin, "nvrtc-builtins64_129.dll")
            nvrtc_path = os.path.join(nvrtc_bin, "nvrtc64_120_0.dll")
            
            if os.path.exists(builtins_path):
                ctypes.WinDLL(builtins_path)
            if os.path.exists(nvrtc_path):
                ctypes.WinDLL(nvrtc_path)
            
            # Also add cuda_runtime DLLs
            runtime_bin = os.path.join(sp, "nvidia", "cuda_runtime", "bin")
            if os.path.exists(runtime_bin):
                os.add_dll_directory(runtime_bin)
            break

from .cli import main

if __name__ == "__main__":
    main()
