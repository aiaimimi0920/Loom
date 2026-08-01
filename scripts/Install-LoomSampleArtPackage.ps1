[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("image-compress", "remove-bg", "image-search", "color-transfer", "image-blend", "image-blend-compress")]
    [string]$PackageName,
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [string]$StoreRoot,
    [string]$StoreUrl = "http://127.0.0.1:8790",
    [ValidateSet("store", "upload")]
    [string]$InstallMode = "upload",
    [switch]$SkipInstall,
    [switch]$SkipPublish,
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-JsonPost {
    param([string]$Url, [object]$Body)
    return Invoke-RestMethod -Uri $Url -Method Post -ContentType "application/json" -Body ($Body | ConvertTo-Json -Depth 40 -Compress) -TimeoutSec 180
}

function Copy-PackageToStore {
    param([string]$Source, [string]$Destination)
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
$frameworkOutput = Join-Path $repoRoot ".loom-art-store-data\frameworks"
$artOutput = Join-Path $repoRoot ".loom-art-store-data\arts"
$storeRootPath = if ([string]::IsNullOrWhiteSpace($StoreRoot)) {
    Join-Path $repoRoot ".loom-art-store-data"
}
elseif ([System.IO.Path]::IsPathRooted($StoreRoot)) {
    [System.IO.Path]::GetFullPath($StoreRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $StoreRoot))
}

$frameworkBuild = Join-Path $scriptRoot "Build-LoomArtFrameworkPackages.ps1"
$artBuild = Join-Path $scriptRoot "Build-LoomSampleArtPackages.ps1"
& powershell -NoProfile -ExecutionPolicy Bypass -File $frameworkBuild -OutputRoot $frameworkOutput -Configuration $Configuration
if ($LASTEXITCODE -ne 0) {
    throw "Framework package build failed."
}
& powershell -NoProfile -ExecutionPolicy Bypass -File $artBuild -OutputRoot $artOutput -Configuration $Configuration
if ($LASTEXITCODE -ne 0) {
    throw "Sample Art package build failed."
}

$summary = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $artOutput "summary.json") | ConvertFrom-Json
$package = @($summary.packages | Where-Object { [string]$_.source -eq $PackageName }) | Select-Object -First 1
if ($null -eq $package) {
    throw "Sample Art package was not found in build summary: $PackageName"
}
$framework = [string]$package.framework
$artId = [string]$package.id
$frameworkZip = Join-Path $frameworkOutput "$framework.zip"
$artZip = Join-Path $artOutput "$artId.zip"
if (-not (Test-Path -LiteralPath $frameworkZip -PathType Leaf)) {
    throw "Framework package is missing: $frameworkZip"
}
if (-not (Test-Path -LiteralPath $artZip -PathType Leaf)) {
    throw "Art package is missing: $artZip"
}

$publishedFramework = $null
$publishedArt = $null
if (-not $SkipPublish) {
    $publishedFramework = Join-Path $storeRootPath "frameworks\$framework.zip"
    $publishedArt = Join-Path $storeRootPath "arts\$artId.zip"
    Copy-PackageToStore -Source $frameworkZip -Destination $publishedFramework
    Copy-PackageToStore -Source $artZip -Destination $publishedArt
}

$frameworkInstall = $null
$artInstall = $null
if (-not $SkipInstall) {
    $frameworkBytes = [System.IO.File]::ReadAllBytes($frameworkZip)
    $frameworkPayload = "data:application/zip;base64,$([Convert]::ToBase64String($frameworkBytes))"
    $frameworkInstall = Invoke-JsonPost -Url ($BaseUrl.TrimEnd('/') + "/v1/frameworks/install") -Body @{ zipBase64 = $frameworkPayload }

    if ($InstallMode -eq "store") {
        if ($SkipPublish) {
            throw "InstallMode=store requires publishing the Art package; remove -SkipPublish."
        }
        $null = Invoke-RestMethod -Uri ($StoreUrl.TrimEnd('/') + "/health") -Method Get -TimeoutSec 5
        $artInstall = Invoke-JsonPost -Url ($BaseUrl.TrimEnd('/') + "/v1/arts/store/install") -Body @{ artId = $artId; store = $StoreUrl }
    }
    else {
        $artBytes = [System.IO.File]::ReadAllBytes($artZip)
        $artPayload = "data:application/zip;base64,$([Convert]::ToBase64String($artBytes))"
        $artInstall = Invoke-JsonPost -Url ($BaseUrl.TrimEnd('/') + "/v1/arts/install") -Body @{ zipBase64 = $artPayload }
    }
}

[ordered]@{
    packageName = $PackageName
    artId = $artId
    framework = $framework
    frameworkZip = $frameworkZip
    artZip = $artZip
    publishedFramework = $publishedFramework
    publishedArt = $publishedArt
    installMode = $InstallMode
    frameworkInstall = $frameworkInstall
    artInstall = $artInstall
} | ConvertTo-Json -Depth 40
