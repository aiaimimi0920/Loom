param(
    [string]$OutputRoot = ".loom-art-store-data\frameworks",
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Write-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

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
$packagesRoot = Join-Path $repoRoot "framework-packages"
$runtimeHostManifest = Join-Path $packagesRoot "runtime-host\Cargo.toml"
$outputRootPath = if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    [System.IO.Path]::GetFullPath($OutputRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputRoot))
}
$stagingRoot = Join-Path $outputRootPath ".staging"
$expectedIds = @(
    "cli_wrapper",
    "cloud_api",
    "script",
    "python_art",
    "mcp",
    "workflow"
)

if (-not (Test-Path -LiteralPath $runtimeHostManifest -PathType Leaf)) {
    throw "External framework runtime host manifest not found: $runtimeHostManifest"
}
New-Item -ItemType Directory -Force -Path $outputRootPath | Out-Null
Assert-PathInside -Path $stagingRoot -Root $outputRootPath -Label "framework staging root"
if (Test-Path -LiteralPath $stagingRoot) {
    Remove-Item -LiteralPath $stagingRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stagingRoot | Out-Null

$cargoArguments = @("build", "--manifest-path", $runtimeHostManifest)
if ($Configuration -eq "Release") {
    $cargoArguments += "--release"
}
& cargo @cargoArguments
if ($LASTEXITCODE -ne 0) {
    throw "Failed to build external framework runtime host."
}

$profile = if ($Configuration -eq "Release") { "release" } else { "debug" }
$hostExecutable = Join-Path $packagesRoot ("runtime-host\target\$profile\loom-framework-runtime-host.exe")
if (-not (Test-Path -LiteralPath $hostExecutable -PathType Leaf)) {
    throw "External framework runtime host executable not found: $hostExecutable"
}

$summary = @()
Add-Type -AssemblyName System.IO.Compression.FileSystem
try {
    foreach ($id in $expectedIds) {
        $sourceDir = Join-Path $packagesRoot $id
        $sourceManifestPath = Join-Path $sourceDir "framework.manifest.json"
        if (-not (Test-Path -LiteralPath $sourceManifestPath -PathType Leaf)) {
            throw "Framework manifest not found: $sourceManifestPath"
        }

        $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $sourceManifestPath | ConvertFrom-Json
        if ([string]$manifest.id -ne $id) {
            throw "Framework manifest id mismatch: expected $id, got $($manifest.id)"
        }
        $command = ([string]$manifest.entry.command).Replace('/', '\')
        if ([string]::IsNullOrWhiteSpace($command)) {
            throw "Framework manifest entry.command is empty: $id"
        }

        $stageDir = Join-Path $stagingRoot $id
        Assert-PathInside -Path $stageDir -Root $stagingRoot -Label "framework package staging directory"
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent (Join-Path $stageDir $command)) | Out-Null
        Copy-Item -LiteralPath $sourceManifestPath -Destination (Join-Path $stageDir "framework.manifest.json") -Force
        Copy-Item -LiteralPath $hostExecutable -Destination (Join-Path $stageDir $command) -Force

        $stagedManifest = $manifest | ConvertTo-Json -Depth 20 | ConvertFrom-Json
        $stagedArgs = @($stagedManifest.entry.args | ForEach-Object { [string]$_ })
        if ($stagedArgs -notcontains "--framework-id") {
            $stagedManifest.entry.args = @($stagedArgs + @("--framework-id", $id))
        }
        Write-Utf8NoBomFile `
            -Path (Join-Path $stageDir "framework.manifest.json") `
            -Content ($stagedManifest | ConvertTo-Json -Depth 20)

        $zipPath = Join-Path $outputRootPath "$id.zip"
        if (Test-Path -LiteralPath $zipPath) {
            Remove-Item -LiteralPath $zipPath -Force
        }
        [System.IO.Compression.ZipFile]::CreateFromDirectory($stageDir, $zipPath)
        $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-Utf8NoBomFile -Path "$zipPath.sha256" -Content "$hash  $id.zip`n"
        $summary += [ordered]@{
            id = $id
            manifest = $sourceManifestPath
            zip = $zipPath
            bytes = (Get-Item -LiteralPath $zipPath).Length
            sha256 = $hash
        }
    }
}
finally {
    if (Test-Path -LiteralPath $stagingRoot) {
        Assert-PathInside -Path $stagingRoot -Root $outputRootPath -Label "framework staging cleanup"
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
}

$summaryPath = Join-Path $outputRootPath "summary.json"
Write-Utf8NoBomFile -Path $summaryPath -Content (([ordered]@{
    configuration = $Configuration
    frameworks = $summary
} | ConvertTo-Json -Depth 20) + "`n")
Write-Host "Built $($summary.Count) independent Art framework packages under $outputRootPath"
