<#
.SYNOPSIS
    Start the local Loom art store server.

.DESCRIPTION
    Builds (if needed) and runs loom-art-store, serving an art catalog, art
    packages, and third-party portable binaries out of a persistent data dir.

    Endpoints:
      GET  /health                -> { "ok": true }
      GET  /catalog               -> { "arts": [ {id,name,description,framework} ] }
      GET  /arts/<id>.zip         -> raw art package bytes
      GET  /binaries/<name>       -> raw portable-exe bytes
      POST /publish               -> body = zip, header X-Art-Id: <id>

    Point the Loom daemon at it by setting, before launching Loom:
      $env:LOOM_ART_STORE_URL = "http://127.0.0.1:8790"

.PARAMETER Port
    TCP port to listen on. Default 8790.

.PARAMETER Root
    Store data directory. Default <repo>\.loom-art-store-data.

.PARAMETER SkipBuild
    Skip cargo build and run the existing release binary.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-art-store.ps1
#>
[CmdletBinding()]
param(
    [int]$Port = 8790,
    [string]$Root,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not $Root -or $Root.Trim().Length -eq 0) {
    $Root = Join-Path $repoRoot ".loom-art-store-data"
}

$exe = Join-Path $repoRoot "target\release\loom-art-store.exe"

if (-not $SkipBuild) {
    Write-Host "Building loom-art-store (release)..." -ForegroundColor Cyan
    Push-Location $repoRoot
    try {
        cargo build --release -p loom-art-store
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path $exe)) {
    throw "loom-art-store.exe not found at $exe. Run without -SkipBuild first."
}

New-Item -ItemType Directory -Force -Path (Join-Path $Root "arts") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $Root "binaries") | Out-Null

$env:LOOM_ART_STORE_PORT = "$Port"
$env:LOOM_ART_STORE_ROOT = $Root

Write-Host "Starting Loom art store" -ForegroundColor Green
Write-Host "  url:  http://127.0.0.1:$Port"
Write-Host "  root: $Root"
Write-Host ""
Write-Host "Point the Loom daemon at it before launching Loom:" -ForegroundColor Yellow
Write-Host "  `$env:LOOM_ART_STORE_URL = `"http://127.0.0.1:$Port`""
Write-Host ""

& $exe
