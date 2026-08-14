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
Assert-True ($servers.Count -eq 1) "Expected exactly one bundled MCP server package."
$server = $servers[0]
Assert-True ([string]$server.id -eq "neuro-image-search") "Bundled MCP server id mismatch."
Assert-True ([string]$server.qualifiedId -eq "neuro.official/neuro-image-search") "Bundled MCP package identity mismatch."
Assert-True (@($server.tools) -contains "brave_image_search") "Bundled MCP tool declaration is missing."
$zipPath = Join-Path $artifactRootPath ([string]$server.zip)
$sidecarPath = "$zipPath.sha256"
Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Bundled MCP ZIP is missing."
Assert-True (Test-Path -LiteralPath $sidecarPath -PathType Leaf) "Bundled MCP checksum is missing."
$hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
$fields = @((Get-Content -Raw -Encoding UTF8 -LiteralPath $sidecarPath).Trim() -split '\s+')
Assert-True ($fields.Count -eq 2) "Bundled MCP checksum format is invalid."
Assert-True ([string]::Equals($fields[0], $hash, [System.StringComparison]::OrdinalIgnoreCase)) "Bundled MCP checksum mismatch."
Assert-True ([string]$fields[1] -eq [string]$server.zip) "Bundled MCP checksum filename mismatch."
Assert-True ([string]::Equals([string]$server.sha256, $hash, [System.StringComparison]::OrdinalIgnoreCase)) "Bundled MCP summary hash mismatch."

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
try {
    $entries = @($archive.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
    Assert-True ($entries -contains "mcp.server.json") "MCP ZIP root manifest is missing."
    Assert-True ($entries -contains "runtime/image-search-mcp.ps1") "MCP ZIP runtime is missing."
    Assert-True (-not ($entries -contains "manifest.json")) "MCP ZIP must not contain an Art manifest."
}
finally {
    $archive.Dispose()
}

Write-Host "Independent MCP server package contract passed."
