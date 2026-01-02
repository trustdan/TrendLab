# Run TUI with logging enabled
# Logs will be written to data/logs/trendlab.log.YYYY-MM-DD

$env:TRENDLAB_LOG_ENABLED = "1"
$env:TRENDLAB_LOG_FILTER = "info,trendlab=debug"

Write-Host "Starting TUI with logging enabled..."
Write-Host "Logs will be written to: data/logs/trendlab.log.YYYY-MM-DD"
Write-Host ""
Write-Host "After running YOLO mode, check the latest log file with:"
Write-Host "  Get-Content data\logs\trendlab.log.* | Select-String -Pattern 'YOLO' -Context 5"
Write-Host ""

cargo run --release --bin trendlab-tui
