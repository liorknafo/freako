$ErrorActionPreference = "Stop"

$crate = "freako-gui"
$binName = "freako-gui.exe"
$profile = "debug"

$rootDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$sourceBin = Join-Path $rootDir "target\$profile\$binName"
$launchRoot = Join-Path $rootDir "target\dev-run\freako-gui"
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$launchDir = Join-Path $launchRoot $timestamp
$launchBin = Join-Path $launchDir $binName

cargo build -p $crate

if (-not (Test-Path $sourceBin)) {
    throw "Built binary not found: $sourceBin"
}

New-Item -ItemType Directory -Force -Path $launchDir | Out-Null
Copy-Item -Path $sourceBin -Destination $launchBin -Force

Start-Process -FilePath $launchBin | Out-Null

Write-Host "Built:  $sourceBin"
Write-Host "Copied: $launchBin"
Write-Host "Launched copied GUI binary"
