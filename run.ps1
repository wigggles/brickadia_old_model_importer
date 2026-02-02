# Launch script for obj2brz GUI (PowerShell)
# Runs the application from source code in DEBUG mode for faster iteration
#
# Usage:
#   .\run.ps1           # Debug mode (fast compile, slower runtime)
#   .\run.ps1 -Release  # Release mode (slow compile, fast runtime)

param(
    [switch]$Release
)

Write-Host "===================================" -ForegroundColor Cyan
Write-Host "Launching obj2brz GUI..." -ForegroundColor Cyan
Write-Host "===================================" -ForegroundColor Cyan
Write-Host ""

# Check if cargo is available
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "Error: cargo not found. Please install Rust from https://rustup.rs/" -ForegroundColor Red
    exit 1
}

# Update PATH to use latest Rust if needed
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

if ($Release) {
    Write-Host "Building in RELEASE mode (optimized, slower compile)..." -ForegroundColor Yellow
    cargo run --release
} else {
    Write-Host "Building in DEBUG mode (fast compile, for development)..." -ForegroundColor Green
    $env:RUST_BACKTRACE = "1"
    cargo run
}

Write-Host ""
Write-Host "===================================" -ForegroundColor Cyan
Write-Host "obj2brz GUI closed" -ForegroundColor Cyan
Write-Host "===================================" -ForegroundColor Cyan
