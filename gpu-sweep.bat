@echo off
REM GPU Mega-Sweep Quick Launcher
REM Sets PYTHONPATH and runs gpu_sweep module
REM
REM Usage:
REM   gpu-sweep gpu-info                              - Show GPU info
REM   gpu-sweep run --config configs\gpu_mega_sweep.yaml --dry-run  - Dry run
REM   gpu-sweep run --config configs\gpu_mega_sweep.yaml            - Full run

setlocal
set "SCRIPT_DIR=%~dp0"
set "PYTHONPATH=%SCRIPT_DIR%scripts"

REM Check if venv exists and use it
if exist "%SCRIPT_DIR%.venv-gpu\Scripts\python.exe" (
    "%SCRIPT_DIR%.venv-gpu\Scripts\python.exe" -m gpu_sweep %*
) else (
    echo Virtual environment not found at .venv-gpu
    echo.
    echo Run setup first:
    echo   powershell -ExecutionPolicy Bypass -File scripts\gpu_sweep\setup_and_run.ps1 -SetupOnly
    echo.
    echo Or run directly with system Python (slower startup^):
    python -m gpu_sweep %*
)
endlocal
