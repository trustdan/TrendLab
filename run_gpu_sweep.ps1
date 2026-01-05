# GPU Mega Sweep Runner
# Run from TrendLab directory: .\run_gpu_sweep.ps1

$ErrorActionPreference = "Stop"

# Ensure we're in the right directory
Set-Location "C:\Users\Dan\TrendLab"

Write-Host "Starting GPU Mega Sweep..." -ForegroundColor Cyan
Write-Host "First symbol takes ~70s (kernel compilation), subsequent symbols ~4s each" -ForegroundColor Yellow
Write-Host ""

$pythonCode = @"
import os, sys, ctypes, site

# Load CUDA NVRTC DLLs
for sp in site.getsitepackages():
    nvrtc_bin = os.path.join(sp, 'nvidia', 'cuda_nvrtc', 'bin')
    if os.path.exists(nvrtc_bin):
        os.add_dll_directory(nvrtc_bin)
        ctypes.WinDLL(os.path.join(nvrtc_bin, 'nvrtc-builtins64_129.dll'))
        ctypes.WinDLL(os.path.join(nvrtc_bin, 'nvrtc64_120_0.dll'))
        break

from scripts.gpu_sweep.config import SweepConfig
from scripts.gpu_sweep.engine import run_sweep
import time

config = SweepConfig.from_yaml('configs/gpu_mega_sweep.yaml')
print(f'Running {len(config.strategies)} strategies on {len(config.symbols)} symbols')
print()

start = time.time()

def progress(msg):
    print(f'[{time.time()-start:6.1f}s] {msg}')

results = run_sweep(config, progress_callback=progress)

print()
print(f'Done in {time.time()-start:.1f}s - {len(results)} results')
results.write_parquet('reports/gpu_sweep_results.parquet')
print('Saved to reports/gpu_sweep_results.parquet')
"@

# Run with the GPU venv Python
& ".\.venv-gpu\Scripts\python.exe" -c $pythonCode

Write-Host ""
Write-Host "Sweep complete!" -ForegroundColor Green
