# TrendLab Windows Build Script
# Usage: .\build.ps1 [-Release] [-Dev] [-Clean]

param(
    [switch]$Release,
    [switch]$Dev,
    [switch]$Clean,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$ProjectRoot = $PSScriptRoot

function Write-Status {
    param([string]$Message)
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Write-Success {
    param([string]$Message)
    Write-Host "[OK] $Message" -ForegroundColor Green
}

function Write-Error {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

if ($Help) {
    Write-Host @"
TrendLab Build Script

Usage: .\build.ps1 [options]

Options:
  -Release    Build optimized release binaries (default if no option given)
  -Dev        Build debug binaries and start dev server
  -Clean      Clean build artifacts before building
  -Help       Show this help message

Examples:
  .\build.ps1                 # Build release
  .\build.ps1 -Release        # Build release
  .\build.ps1 -Dev            # Start development mode
  .\build.ps1 -Clean -Release # Clean and rebuild

Output:
  Release binaries are placed in: target\release\
    - trendlab.exe       (launcher)
    - trendlab-gui.exe   (desktop GUI)
    - trendlab-tui.exe   (terminal UI)
"@
    exit 0
}

# Default to Release if no mode specified
if (-not $Release -and -not $Dev) {
    $Release = $true
}

Write-Host ""
Write-Host "================================" -ForegroundColor Yellow
Write-Host "   TrendLab Build Script" -ForegroundColor Yellow
Write-Host "================================" -ForegroundColor Yellow

# Check prerequisites
Write-Status "Checking prerequisites..."

# Check Rust
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Rust/Cargo not found. Install from https://rustup.rs"
    exit 1
}
Write-Success "Rust/Cargo found"

# Check Node.js
if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    Write-Error "Node.js/npm not found. Install from https://nodejs.org"
    exit 1
}
Write-Success "Node.js/npm found"

# Check Tauri CLI
$tauriInstalled = cargo install --list | Select-String "tauri-cli"
if (-not $tauriInstalled) {
    Write-Status "Installing Tauri CLI..."
    cargo install tauri-cli
}
Write-Success "Tauri CLI available"

# Clean if requested
if ($Clean) {
    Write-Status "Cleaning build artifacts..."

    if (Test-Path "$ProjectRoot\target") {
        Remove-Item -Recurse -Force "$ProjectRoot\target" -ErrorAction SilentlyContinue
    }

    if (Test-Path "$ProjectRoot\apps\trendlab-gui\ui\dist") {
        Remove-Item -Recurse -Force "$ProjectRoot\apps\trendlab-gui\ui\dist" -ErrorAction SilentlyContinue
    }

    if (Test-Path "$ProjectRoot\apps\trendlab-gui\ui\node_modules") {
        Remove-Item -Recurse -Force "$ProjectRoot\apps\trendlab-gui\ui\node_modules" -ErrorAction SilentlyContinue
    }

    Write-Success "Clean complete"
}

# Install frontend dependencies
Write-Status "Installing frontend dependencies..."
Push-Location "$ProjectRoot\apps\trendlab-gui\ui"
try {
    npm install
    Write-Success "Frontend dependencies installed"
} finally {
    Pop-Location
}

if ($Dev) {
    # Development mode
    Write-Status "Starting development mode..."
    Write-Host ""
    Write-Host "Starting Tauri dev server..." -ForegroundColor Yellow
    Write-Host "Press Ctrl+C to stop" -ForegroundColor Gray
    Write-Host ""

    Push-Location "$ProjectRoot\apps\trendlab-gui\src-tauri"
    try {
        cargo tauri dev
    } finally {
        Pop-Location
    }
}
else {
    # Release build
    Write-Status "Building frontend..."
    Push-Location "$ProjectRoot\apps\trendlab-gui\ui"
    try {
        npm run build
        Write-Success "Frontend built"
    } finally {
        Pop-Location
    }

    Write-Status "Building Tauri GUI (release)..."
    Push-Location "$ProjectRoot\apps\trendlab-gui\src-tauri"
    try {
        cargo tauri build
        Write-Success "GUI built"
    } finally {
        Pop-Location
    }

    Write-Status "Building Launcher (release)..."
    Push-Location $ProjectRoot
    try {
        cargo build --release -p trendlab-launcher
        Write-Success "Launcher built"
    } finally {
        Pop-Location
    }

    Write-Status "Building TUI (release)..."
    Push-Location $ProjectRoot
    try {
        cargo build --release -p trendlab-tui
        Write-Success "TUI built"
    } finally {
        Pop-Location
    }

    # Summary
    Write-Host ""
    Write-Host "================================" -ForegroundColor Green
    Write-Host "   Build Complete!" -ForegroundColor Green
    Write-Host "================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Release binaries:" -ForegroundColor Yellow

    $binaries = @(
        "target\release\trendlab.exe",
        "target\release\trendlab-gui.exe",
        "target\release\trendlab-tui.exe"
    )

    foreach ($bin in $binaries) {
        $fullPath = Join-Path $ProjectRoot $bin
        if (Test-Path $fullPath) {
            $size = [math]::Round((Get-Item $fullPath).Length / 1MB, 2)
            Write-Host "  $bin ($size MB)" -ForegroundColor White
        }
    }

    Write-Host ""
    Write-Host "To run: .\target\release\trendlab.exe" -ForegroundColor Cyan
    Write-Host ""
}
