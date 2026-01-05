"""Test imports only."""
import sys
print("Step 0: Starting")
sys.stdout.flush()

import os
project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, project_root)
print("Step 1: Path set")
sys.stdout.flush()

import ctypes
import site
for sp in site.getsitepackages():
    nvrtc_bin = os.path.join(sp, 'nvidia', 'cuda_nvrtc', 'bin')
    if os.path.exists(nvrtc_bin):
        os.add_dll_directory(nvrtc_bin)
        break
print("Step 2: DLLs loaded")
sys.stdout.flush()

print("Step 3: Importing cupy...")
sys.stdout.flush()
import cupy as cp
print("Step 3: OK")
sys.stdout.flush()

print("Step 4: Importing kernels.ema...")
sys.stdout.flush()
from scripts.gpu_sweep.kernels import ema
print("Step 4: OK")
sys.stdout.flush()

print("Step 5: Getting kernel...")
sys.stdout.flush()
k = ema.get_ema_kernel()
print(f"Step 5: OK - {k}")
sys.stdout.flush()

print("Step 6: Creating test data...")
sys.stdout.flush()
x = cp.ones((2, 10), dtype=cp.float32)
print("Step 6: OK")
sys.stdout.flush()

print("Step 7: Calling ema_kernel...")
sys.stdout.flush()
result = ema.ema_kernel(x, 3)
print(f"Step 7: OK - shape={result.shape}")
sys.stdout.flush()

print("Done!")
