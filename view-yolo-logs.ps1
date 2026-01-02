# View YOLO-related log entries from the most recent log file

$logFiles = Get-ChildItem data\logs\trendlab.log.* -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending
if ($logFiles.Count -eq 0) {
    Write-Host "No log files found in data/logs/"
    Write-Host "Run with: `$env:TRENDLAB_LOG_ENABLED='1'; cargo run --release --bin trendlab-tui"
    Write-Host "Or use the launcher with logging enabled (select 'Yes' at the logging prompt)"
    exit 1
}

$latestLog = $logFiles[0]
Write-Host "Reading from: $($latestLog.Name)"
Write-Host "=" * 80
Write-Host ""

Get-Content $latestLog | Select-String -Pattern "YOLO|universe|ticker|symbol" -Context 2 | Select-Object -Last 50
