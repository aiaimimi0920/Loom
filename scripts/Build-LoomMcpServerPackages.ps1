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

function Get-DirectoryTreeDigest {
    param([Parameter(Mandatory = $true)][string]$Root)

    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\')
    $records = @(Get-ChildItem -LiteralPath $resolvedRoot -Recurse -File | ForEach-Object {
        [pscustomobject]@{
            relative = $_.FullName.Substring($resolvedRoot.Length + 1).Replace('\', '/')
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    } | Sort-Object relative)
    $content = (@($records | ForEach-Object { "$($_.sha256)  $($_.relative)" }) -join "`n") + "`n"
    $algorithm = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($content)
        $digest = ([System.BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $algorithm.Dispose()
    }
    return [pscustomobject]@{ count = $records.Count; sha256 = $digest }
}

function Add-StockApiNodeRuntime {
    param(
        [Parameter(Mandatory = $true)][string]$StageDirectory,
        [Parameter(Mandatory = $true)][object]$ServerManifest
    )

    $runtimeRoot = Join-Path $StageDirectory "runtime"
    $nodeMetadataPath = Join-Path $runtimeRoot "node-runtime.json"
    $upstreamMetadataPath = Join-Path $runtimeRoot "UPSTREAM.json"
    $vendoredRoot = Join-Path $runtimeRoot "vendor\stock-api"
    foreach ($required in @($nodeMetadataPath, $upstreamMetadataPath, (Join-Path $vendoredRoot "package.json"), (Join-Path $vendoredRoot "LICENSE"))) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Stock API supply-chain input is missing: $required"
        }
    }

    $nodeMetadata = Get-Content -Raw -Encoding UTF8 -LiteralPath $nodeMetadataPath | ConvertFrom-Json
    $upstreamMetadata = Get-Content -Raw -Encoding UTF8 -LiteralPath $upstreamMetadataPath | ConvertFrom-Json
    $vendoredPackage = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $vendoredRoot "package.json") | ConvertFrom-Json
    if ([string]$vendoredPackage.name -ne "stock-api" -or [string]$vendoredPackage.version -ne [string]$upstreamMetadata.version) {
        throw "Vendored stock-api package identity does not match UPSTREAM.json."
    }
    if ([string]$vendoredPackage.version -ne [string]$ServerManifest.version -or [string]$vendoredPackage.license -ne "MIT") {
        throw "Vendored stock-api version or license does not match the MCP server manifest."
    }
    $tree = Get-DirectoryTreeDigest -Root $vendoredRoot
    if ([int]$tree.count -ne [int]$upstreamMetadata.vendoredFileCount -or
        -not [string]::Equals([string]$tree.sha256, [string]$upstreamMetadata.vendoredTreeSha256, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Vendored stock-api tree does not match the pinned upstream digest."
    }

    $nodeCommand = Get-Command node.exe -ErrorAction Stop
    $nodePath = [System.IO.Path]::GetFullPath([string]$nodeCommand.Source)
    $nodeVersion = ((& $nodePath --version) | ForEach-Object { $_.ToString() }) -join ""
    if ($LASTEXITCODE -ne 0 -or $nodeVersion.TrimStart('v') -ne [string]$nodeMetadata.version) {
        throw "The build host must provide pinned Node.js v$($nodeMetadata.version); found $nodeVersion."
    }
    $nodeHash = (Get-FileHash -LiteralPath $nodePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if (-not [string]::Equals($nodeHash, [string]$nodeMetadata.sha256, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "The build host Node.js executable does not match the pinned SHA-256."
    }
    $nodeLicensePath = Join-Path (Split-Path -Parent $nodePath) ([string]$nodeMetadata.licenseFile)
    if (-not (Test-Path -LiteralPath $nodeLicensePath -PathType Leaf)) {
        throw "The pinned Node.js license file is missing: $nodeLicensePath"
    }

    $nodeTarget = Join-Path $runtimeRoot "node"
    New-Item -ItemType Directory -Force -Path $nodeTarget | Out-Null
    Copy-Item -LiteralPath $nodePath -Destination (Join-Path $nodeTarget "node.exe") -Force
    Copy-Item -LiteralPath $nodeLicensePath -Destination (Join-Path $nodeTarget "LICENSE") -Force
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
$packageSources = @("image-search", "stock-api")

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
        if ($id -eq "stock-api") {
            Add-StockApiNodeRuntime -StageDirectory $stageDir -ServerManifest $manifest
        }
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
