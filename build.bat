@echo off
:: TrendLab Windows Build Script (batch wrapper)
:: This calls the PowerShell build script with the appropriate execution policy

setlocal

:: Check if PowerShell is available
where powershell >nul 2>&1
if %errorlevel% neq 0 (
    echo ERROR: PowerShell not found
    exit /b 1
)

:: Pass all arguments to the PowerShell script
powershell -ExecutionPolicy Bypass -File "%~dp0build.ps1" %*

endlocal
