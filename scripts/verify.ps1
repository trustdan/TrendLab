Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-LastExit {
    param(
        [Parameter(Mandatory = $true)][string]$StepName
    )
    if ($LASTEXITCODE -ne 0) {
        throw "$StepName failed with exit code $LASTEXITCODE"
    }
}

Write-Host "== TrendLab verify =="
Write-Host ""

Write-Host "-> cargo fmt --check"
cargo fmt -- --check
Assert-LastExit "cargo fmt --check"

Write-Host ""
Write-Host "-> cargo clippy (deny warnings)"
cargo clippy --all-targets --all-features -- -D warnings
Assert-LastExit "cargo clippy"

Write-Host ""
Write-Host "-> cargo test"
cargo test
Assert-LastExit "cargo test"

Write-Host ""
Write-Host "-> cargo deny check (if installed)"
if (Get-Command cargo-deny -ErrorAction SilentlyContinue) {
    cargo deny check
    Assert-LastExit "cargo deny check"
} else {
    Write-Host "   (skipped: cargo-deny not installed)"
}

Write-Host ""
Write-Host "OK"


