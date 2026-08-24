[CmdletBinding()]
param(
    [string]$ArtifactRoot = ".loom-art-store-data\mcp-servers"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Get-ArchiveEntrySha256 {
    param([Parameter(Mandatory = $true)][System.IO.Compression.ZipArchiveEntry]$Entry)

    $stream = $Entry.Open()
    $algorithm = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$artifactRootPath = if ([System.IO.Path]::IsPathRooted($ArtifactRoot)) {
    [System.IO.Path]::GetFullPath($ArtifactRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArtifactRoot))
}
$summaryPath = Join-Path $artifactRootPath "summary.json"
Assert-True (Test-Path -LiteralPath $summaryPath -PathType Leaf) "MCP package summary is missing: $summaryPath"
$summary = Get-Content -Raw -Encoding UTF8 -LiteralPath $summaryPath | ConvertFrom-Json
$servers = @($summary.servers)
$expected = [ordered]@{
    "neuro-image-search" = [ordered]@{
        qualifiedId = "neuro.official/neuro-image-search"
        version = "0.1.0"
        tools = @("brave_image_search")
        runtime = "runtime/image-search-mcp.ps1"
    }
    "stock-api" = [ordered]@{
        qualifiedId = "neuro.official/stock-api"
        version = "2.9.0"
        tools = @("get_stock", "get_stocks", "get_klines", "get_market_series", "get_order_book", "search_stocks", "inspect_stock")
        runtime = "runtime/stock-api-mcp.ps1"
    }
}
Assert-True ($servers.Count -eq $expected.Count) "Expected exactly $($expected.Count) bundled MCP server packages."
$actualIds = @($servers | ForEach-Object { [string]$_.id } | Sort-Object)
Assert-True (($actualIds -join ',') -eq (@($expected.Keys | Sort-Object) -join ',')) "Bundled MCP server id set mismatch."

Add-Type -AssemblyName System.IO.Compression.FileSystem
foreach ($entry in $expected.GetEnumerator()) {
    $server = @($servers | Where-Object { [string]$_.id -eq $entry.Key })
    Assert-True ($server.Count -eq 1) "Bundled MCP server summary entry mismatch: $($entry.Key)"
    $server = $server[0]
    Assert-True ([string]$server.qualifiedId -eq $entry.Value.qualifiedId) "Bundled MCP package identity mismatch: $($entry.Key)"
    Assert-True ([string]$server.version -eq $entry.Value.version) "Bundled MCP package version mismatch: $($entry.Key)"
    $actualTools = @($server.tools | ForEach-Object { [string]$_ } | Sort-Object)
    $expectedTools = @($entry.Value.tools | Sort-Object)
    Assert-True (($actualTools -join ',') -eq ($expectedTools -join ',')) "Bundled MCP tool declaration mismatch: $($entry.Key)"

    $zipPath = Join-Path $artifactRootPath ([string]$server.zip)
    $sidecarPath = "$zipPath.sha256"
    Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Bundled MCP ZIP is missing: $($entry.Key)"
    Assert-True (Test-Path -LiteralPath $sidecarPath -PathType Leaf) "Bundled MCP checksum is missing: $($entry.Key)"
    $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $fields = @((Get-Content -Raw -Encoding UTF8 -LiteralPath $sidecarPath).Trim() -split '\s+')
    Assert-True ($fields.Count -eq 2) "Bundled MCP checksum format is invalid: $($entry.Key)"
    Assert-True ([string]::Equals($fields[0], $hash, [System.StringComparison]::OrdinalIgnoreCase)) "Bundled MCP checksum mismatch: $($entry.Key)"
    Assert-True ([string]$fields[1] -eq [string]$server.zip) "Bundled MCP checksum filename mismatch: $($entry.Key)"
    Assert-True ([string]::Equals([string]$server.sha256, $hash, [System.StringComparison]::OrdinalIgnoreCase)) "Bundled MCP summary hash mismatch: $($entry.Key)"

    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $entries = @($archive.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
        Assert-True ($entries -contains "mcp.server.json") "MCP ZIP root manifest is missing: $($entry.Key)"
        Assert-True ($entries -contains [string]$entry.Value.runtime) "MCP ZIP runtime is missing: $($entry.Key)"
        Assert-True (-not ($entries -contains "manifest.json")) "MCP ZIP must not contain an Art manifest: $($entry.Key)"
        if ($entry.Key -eq "stock-api") {
            Assert-True ((Get-Item -LiteralPath $zipPath).Length -le 64MB) "stock-api MCP ZIP exceeds the installer package limit."
            foreach ($requiredPath in @(
                "runtime/stock-api-entry.js",
                "runtime/stock-api/constants.js",
                "runtime/stock-api/executors.js",
                "runtime/stock-api/helpers.js",
                "runtime/stock-api/parsers.js",
                "runtime/stock-api/providers.js",
                "runtime/stock-api/server.js",
                "runtime/stock-api/transport.js",
                "runtime/node/node.exe",
                "runtime/node/LICENSE",
                "runtime/node-runtime.json",
                "runtime/UPSTREAM.json",
                "runtime/PYSNOWBALL.json",
                "runtime/vendor/stock-api/package.json",
                "runtime/vendor/stock-api/LICENSE",
                "runtime/vendor/stock-api/dist/mcp/server.js",
                "runtime/vendor/pysnowball/LICENSE",
                "runtime/vendor/pysnowball/NOTICE.md"
            )) {
                Assert-True ($entries -contains $requiredPath) "stock-api MCP ZIP is missing: $requiredPath"
            }
            Assert-True (-not @($entries | Where-Object { $_ -match '(^|/)node_modules/' }).Count) "stock-api MCP ZIP must not contain node_modules."
            $nodeEntry = @($archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq "runtime/node/node.exe" })[0]
            $nodeHash = Get-ArchiveEntrySha256 -Entry $nodeEntry
            Assert-True ($nodeHash -eq "ae1a50511be58e987483fdbc12125407443926d2d394669ade2352776e920dd3") "Bundled Node.js SHA-256 mismatch."
        }
    }
    finally {
        $archive.Dispose()
    }
}

Write-Host "Independent MCP server package contract passed: packages=2 stock-api=2.9.0 upstream=2.7.3 pysnowball=0.1.8"
