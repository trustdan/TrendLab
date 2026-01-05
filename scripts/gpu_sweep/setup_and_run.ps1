# GPU Mega-Sweep Setup and Run Script
# Creates venv, installs dependencies, and runs the sweep tool

param(
    [switch]$SetupOnly,      # Only setup, don't run
    [switch]$SkipSetup,      # Skip setup, just run
    [switch]$GpuInfo,        # Show GPU info
    [switch]$DryRun,         # Dry run mode
    [string]$Config = "",    # Config file path
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = (Get-Item $ScriptDir).Parent.Parent.FullName
$VenvDir = Join-Path $ProjectRoot ".venv-gpu"
$RequirementsFile = Join-Path $ScriptDir "requirements-gpu.txt"

function Write-Header {
    param([string]$Text)
    Write-Host ""
    Write-Host "=" * 60 -ForegroundColor Cyan
    Write-Host "  $Text" -ForegroundColor Cyan
    Write-Host "=" * 60 -ForegroundColor Cyan
    Write-Host ""
}

function Write-Step {
    param([string]$Text)
    Write-Host "[*] $Text" -ForegroundColor Yellow
}

function Write-Success {
    param([string]$Text)
    Write-Host "[+] $Text" -ForegroundColor Green
}

function Write-Error {
    param([string]$Text)
    Write-Host "[!] $Text" -ForegroundColor Red
}

function Show-Help {
    Write-Host @"

GPU Mega-Sweep Setup and Run Script
====================================

Usage:
    .\setup_and_run.ps1 [options]

Options:
    -SetupOnly      Only create venv and install dependencies
    -SkipSetup      Skip setup, just run (assumes venv exists)
    -GpuInfo        Show GPU information
    -DryRun         Run in dry-run mode (show config count)
    -Config <path>  Path to YAML config file
    -Help           Show this help message

Examples:
    # First time setup + GPU info check
    .\setup_and_run.ps1 -SetupOnly
    .\setup_and_run.ps1 -GpuInfo

    # Run with config file
    .\setup_and_run.ps1 -Config configs/gpu_mega_sweep.yaml

    # Dry run to see config count
    .\setup_and_run.ps1 -DryRun -Config configs/gpu_mega_sweep.yaml

    # Skip setup on subsequent runs
    .\setup_and_run.ps1 -SkipSetup -Config configs/gpu_mega_sweep.yaml

"@
}

function Test-CudaAvailable {
    Write-Step "Checking CUDA availability..."

    # Check for nvcc
    $nvcc = Get-Command nvcc -ErrorAction SilentlyContinue
    if ($nvcc) {
        $nvccVersion = & nvcc --version 2>&1 | Select-String "release"
        Write-Success "CUDA Toolkit found: $nvccVersion"
        return $true
    }

    # Check for nvidia-smi
    $nvidiaSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
    if ($nvidiaSmi) {
        Write-Success "NVIDIA driver found (nvidia-smi available)"
        return $true
    }

    Write-Error "CUDA not found. Please install CUDA Toolkit 12.x"
    Write-Host "  Download from: https://developer.nvidia.com/cuda-downloads" -ForegroundColor Gray
    return $false
}

function Setup-Venv {
    Write-Header "Setting up Python Virtual Environment"

    # Check Python version
    Write-Step "Checking Python version..."
    $pythonVersion = & python --version 2>&1
    Write-Success "Found: $pythonVersion"

    # Check CUDA
    $cudaOk = Test-CudaAvailable
    if (-not $cudaOk) {
        Write-Host "  Warning: CUDA may not work correctly" -ForegroundColor Yellow
    }

    # Create venv if it doesn't exist
    if (-not (Test-Path $VenvDir)) {
        Write-Step "Creating virtual environment at $VenvDir..."
        & python -m venv $VenvDir
        Write-Success "Virtual environment created"
    } else {
        Write-Success "Virtual environment already exists"
    }

    # Activate venv
    Write-Step "Activating virtual environment..."
    $activateScript = Join-Path $VenvDir "Scripts\Activate.ps1"
    . $activateScript
    Write-Success "Virtual environment activated"

    # Upgrade pip
    Write-Step "Upgrading pip..."
    & python -m pip install --upgrade pip --quiet
    Write-Success "pip upgraded"

    # Install dependencies
    Write-Step "Installing dependencies from $RequirementsFile..."
    & pip install -r $RequirementsFile
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to install dependencies"
        exit 1
    }
    Write-Success "Dependencies installed"

    Write-Header "Setup Complete!"
    Write-Host "Virtual environment: $VenvDir" -ForegroundColor Gray
    Write-Host ""
    Write-Host "To activate manually:" -ForegroundColor Gray
    Write-Host "  $activateScript" -ForegroundColor White
    Write-Host ""
}

function Run-GpuSweep {
    param(
        [string[]]$Arguments
    )

    # Activate venv
    $activateScript = Join-Path $VenvDir "Scripts\Activate.ps1"
    if (-not (Test-Path $activateScript)) {
        Write-Error "Virtual environment not found. Run with -SetupOnly first."
        exit 1
    }
    . $activateScript

    # Set PYTHONPATH to include scripts directory
    $scriptsDir = Join-Path $ProjectRoot "scripts"
    $env:PYTHONPATH = $scriptsDir

    Write-Step "Running gpu_sweep with PYTHONPATH=$scriptsDir"
    Write-Host ""

    # Run the module
    & python -m gpu_sweep @Arguments
}

# Main logic
if ($Help) {
    Show-Help
    exit 0
}

# Change to project root
Push-Location $ProjectRoot

try {
    if (-not $SkipSetup) {
        Setup-Venv
    }

    if ($SetupOnly) {
        Write-Host "Setup complete. Use -GpuInfo to verify GPU, or -Config to run a sweep." -ForegroundColor Green
        exit 0
    }

    # Build arguments for gpu_sweep
    $gpuArgs = @()

    if ($GpuInfo) {
        $gpuArgs += "gpu-info"
    } elseif ($Config) {
        # "run" is a subcommand
        $gpuArgs += "run"
        if ($DryRun) {
            $gpuArgs += "--dry-run"
        }
        $gpuArgs += "--config"
        $gpuArgs += $Config
    } else {
        # Default: show help
        $gpuArgs += "--help"
    }

    Run-GpuSweep -Arguments $gpuArgs

} finally {
    Pop-Location
}
