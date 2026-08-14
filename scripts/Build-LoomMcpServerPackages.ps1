[CmdletBinding()]
param(
    [string]$OutputRoot = ".loom-art-store-data\mcp-servers"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Assert-PathInside {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $resolvedPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\') + '\'
    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    if (-not $resolvedPath.StartsWith($resolvedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label escaped its intended root: $Path"
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
$sourceRoot = Join-Path $repoRoot "mcp-server-packages"
$outputRootPath = if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    [System.IO.Path]::GetFullPath($OutputRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputRoot))
}
$stagingRoot = Join-Path $outputRootPath ".staging"
$packageSources = @("image-search")

New-Item -ItemType Directory -Force -Path $outputRootPath | Out-Null
Assert-PathInside -Path $stagingRoot -Root $outputRootPath -Label "MCP package staging root"
if (Test-Path -LiteralPath $stagingRoot) {
    Remove-Item -LiteralPath $stagingRoot -Recurse -Force
}
foreach ($existingZip in Get-ChildItem -LiteralPath $outputRootPath -File -Filter "*.zip") {
    Remove-Item -LiteralPath $existingZip.FullName -Force
    if (Test-Path -LiteralPath "$($existingZip.FullName).sha256" -PathType Leaf) {
        Remove-Item -LiteralPath "$($existingZip.FullName).sha256" -Force
    }
}
New-Item -ItemType Directory -Force -Path $stagingRoot | Out-Null

$servers = @()
try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    foreach ($sourceName in $packageSources) {
        $sourceDir = Join-Path $sourceRoot $sourceName
        $manifestPath = Join-Path $sourceDir "mcp.server.json"
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
            throw "MCP server package manifest not found: $manifestPath"
        }
        $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
        foreach ($required in @("id", "name", "version", "transport")) {
            if ([string]::IsNullOrWhiteSpace([string]$manifest.$required)) {
                throw "MCP server package $required is required: $manifestPath"
            }
        }
        if ([int]$manifest.schemaVersion -ne 1) {
            throw "MCP server package schemaVersion must be 1: $manifestPath"
        }
        $publisherId = [string]$manifest.publisher.id
        if ([string]::IsNullOrWhiteSpace($publisherId)) {
            throw "MCP server package publisher.id is required: $manifestPath"
        }
        $entry = ([string]$manifest.entry.command).Replace('/', '\')
        if ([string]$manifest.transport -eq "stdio" -and
            (-not (Test-Path -LiteralPath (Join-Path $sourceDir $entry) -PathType Leaf))) {
            throw "MCP server package entry is missing: $entry"
        }
        if (@($manifest.tools).Count -eq 0) {
            throw "MCP server package must declare at least one tool: $manifestPath"
        }

        $id = [string]$manifest.id
        $stageDir = Join-Path $stagingRoot $id
        Assert-PathInside -Path $stageDir -Root $stagingRoot -Label "MCP package staging directory"
        Copy-Item -LiteralPath $sourceDir -Destination $stageDir -Recurse -Force
        $zipPath = Join-Path $outputRootPath "$id.zip"
        [System.IO.Compression.ZipFile]::CreateFromDirectory($stageDir, $zipPath)
        $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-Utf8NoBomFile -Path "$zipPath.sha256" -Content "$hash  $id.zip`n"
        $servers += [ordered]@{
            id = $id
            qualifiedId = "$publisherId/$id"
            version = [string]$manifest.version
            transport = [string]$manifest.transport
            tools = @($manifest.tools | ForEach-Object { [string]$_ })
            manifest = "mcp-server-packages/$sourceName/mcp.server.json"
            zip = "$id.zip"
            bytes = (Get-Item -LiteralPath $zipPath).Length
            sha256 = $hash
        }
    }
}
finally {
    if (Test-Path -LiteralPath $stagingRoot) {
        Assert-PathInside -Path $stagingRoot -Root $outputRootPath -Label "MCP package staging cleanup"
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
}

Write-Utf8NoBomFile -Path (Join-Path $outputRootPath "summary.json") -Content (([ordered]@{
    schemaVersion = 1
    servers = $servers
} | ConvertTo-Json -Depth 20) + "`n")
Write-Host "Built $($servers.Count) independent MCP server packages under $outputRootPath"
