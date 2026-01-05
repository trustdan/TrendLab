"""Simple kernel test."""

import os
import sys

project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, project_root)

import ctypes
import site

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

print("Step 1: Import cupy")
import cupy as cp
print("  OK")

print("Step 2: Create test data")
close = cp.random.randn(10, 100).astype(cp.float32)
print(f"  OK: shape={close.shape}")

print("Step 3: Test basic CuPy RawKernel")
kernel_code = r'''
extern "C" __global__
void simple_kernel(float* x, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) {
        x[i] = x[i] * 2.0f;
    }
}
'''
try:
    kernel = cp.RawKernel(kernel_code, 'simple_kernel')
    print("  Kernel compiled OK")

    x = cp.ones(100, dtype=cp.float32)
    kernel((1,), (256,), (x, 100))
    cp.cuda.Stream.null.synchronize()
    print(f"  Kernel executed OK, result[0]={float(x[0])}")
except Exception as e:
    print(f"  FAILED: {e}")
    import traceback
    traceback.print_exc()

print("Step 4: Test EMA kernel import")
try:
    from scripts.gpu_sweep.kernels.ema import ema_kernel
    print("  Import OK")
except Exception as e:
    print(f"  FAILED: {e}")
    import traceback
    traceback.print_exc()
    sys.exit(1)

print("Step 5: Test EMA kernel execution")
try:
    result = ema_kernel(close, 10)
    cp.cuda.Stream.null.synchronize()
    print(f"  OK: result shape={result.shape}")
    print(f"  result[0,9]={float(result[0,9]):.4f}, result[0,-1]={float(result[0,-1]):.4f}")
except Exception as e:
    print(f"  FAILED: {e}")
    import traceback
    traceback.print_exc()
    sys.exit(1)

print()
print("All simple tests passed!")
