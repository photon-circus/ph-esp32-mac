# ph-esp32-mac Development Scripts
#
# PowerShell script for common development tasks across all crates.
# Usage: .\scripts\dev.ps1 <command>
#
# Commands:
#   fmt       - Format all code
#   clean     - Clean all build artifacts
#   check     - Check all crates
#   test      - Run all tests
#   clippy    - Run clippy on all crates

param(
    [Parameter(Position=0)]
    [string]$Command = "help"
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir

function Write-Header($msg) {
    Write-Host ""
    Write-Host "=== $msg ===" -ForegroundColor Cyan
}

function Invoke-InDir($dir, $cmd) {
    Push-Location $dir
    try {
        Invoke-Expression $cmd
        if ($LASTEXITCODE -ne 0) {
            throw "Command failed: $cmd"
        }
    } finally {
        Pop-Location
    }
}

switch ($Command.ToLower()) {
    "fmt" {
        Write-Header "Formatting main crate"
        cargo fmt
        
        Write-Header "Formatting integration_tests"
        Invoke-InDir "$RootDir\integration_tests" "cargo fmt"
        
        Write-Host "`nAll code formatted!" -ForegroundColor Green
    }
    
    "clean" {
        Write-Header "Cleaning main crate"
        cargo clean
        
        Write-Header "Cleaning integration_tests"
        Invoke-InDir "$RootDir\integration_tests" "cargo clean"
        
        Write-Host "`nAll build artifacts cleaned!" -ForegroundColor Green
    }
    
    "check" {
        Write-Header "Checking main crate"
        cargo check
        
        Write-Header "Checking integration_tests"
        Invoke-InDir "$RootDir\integration_tests" "cargo check"
        
        Write-Host "`nAll crates check passed!" -ForegroundColor Green
    }
    
    "test" {
        Write-Header "Running main crate tests"
        cargo test --lib
        
        Write-Host "`nAll tests passed!" -ForegroundColor Green
    }
    
    "clippy" {
        Write-Header "Running clippy on main crate"
        cargo clippy -- -D warnings
        
        Write-Header "Running clippy on integration_tests"
        Invoke-InDir "$RootDir\integration_tests" "cargo clippy -- -D warnings"
        
        Write-Host "`nClippy passed on all crates!" -ForegroundColor Green
    }
    
    "all" {
        # Run all checks
        & $PSCommandPath fmt
        & $PSCommandPath clippy
        & $PSCommandPath test
        
        Write-Host "`n All checks passed!" -ForegroundColor Green
    }
    
    default {
        Write-Host @"
ph-esp32-mac Development Scripts

Usage: .\scripts\dev.ps1 <command>

Commands:
  fmt       Format all code (main + integration_tests)
  clean     Clean all build artifacts
  check     Check all crates compile
  test      Run all host tests
  clippy    Run clippy on all crates
  all       Run fmt, clippy, and test

Examples:
  .\scripts\dev.ps1 fmt
  .\scripts\dev.ps1 all
"@
    }
}
