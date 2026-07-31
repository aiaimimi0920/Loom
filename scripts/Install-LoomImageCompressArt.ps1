<#
.SYNOPSIS
    Download the official Pingo portable package and install Loom's image-compress art.

.DESCRIPTION
    Rebuilds the image-compress art as a formal installable cli_wrapper package:
      - downloads the official Pingo Windows x64 portable zip
      - bundles pingo.exe into a Loom art package zip
      - optionally publishes that zip into the local Loom art-store root
      - optionally installs the art into a running Loom daemon

    The generated art keeps the existing production id by default so existing
    Hook/Loom nodes that already reference that art id continue to work after
    installation.

.PARAMETER BaseUrl
    Loom daemon base URL used for installation. Default http://127.0.0.1:8765.

.PARAMETER ArtId
    Tool/art id to install. Defaults to the current image-compress art id.

.PARAMETER ArtName
    Display name. Defaults to the existing Chinese image-compress label.

.PARAMETER StoreRoot
    Local art-store root used when publishing the generated zip. Defaults to
    <repo>/.loom-art-store-data.

.PARAMETER StoreUrl
    Local art-store URL used when installing through the store route. Default
    http://127.0.0.1:8790.

.PARAMETER ControlPlaneRoot
    Loom control-plane root used for local installation mode. Defaults to
    %APPDATA%\Loom\control-plane.

.PARAMETER InstallMode
    Install strategy:
      - local  : lay the bundled art into <control-plane>/arts/<id>/ and update
                 tools.json directly
      - store  : publish the zip to StoreRoot, then ask Loom to install it from
                 the local art store URL
      - upload : upload the full zip payload directly to /v1/arts/install
    Default local, because bundled binaries easily exceed the daemon's safe
    direct-upload body limit once base64 encoded and a local art-store server
    may not be running.

.PARAMETER SkipInstall
    Only build/publish the package; do not POST it into the running daemon.

.PARAMETER SkipPublish
    Do not copy the generated zip into the local art-store root.

.PARAMETER ForceDownload
    Always re-download pingo-win64.zip even when a cached copy already exists.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/Install-LoomImageCompressArt.ps1
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [string]$ArtId = "custom-1770146354922",
    [string]$ArtName = "__AUTO_ART_NAME__",
    [string]$StoreRoot,
    [string]$StoreUrl = "http://127.0.0.1:8790",
    [string]$ControlPlaneRoot,
    [ValidateSet("local", "store", "upload")]
    [string]$InstallMode = "local",
    [switch]$SkipInstall,
    [switch]$SkipPublish,
    [switch]$ForceDownload
)

$ErrorActionPreference = "Stop"

function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
}

$imageCompressLabel = ConvertFrom-UnicodeCodePoints @(0x56FE, 0x7247, 0x538B, 0x7F29)
$imageCompressDescription = ConvertFrom-UnicodeCodePoints @(
    0x4F7F, 0x7528, 0x0020, 0x0050, 0x0069, 0x006E, 0x0067, 0x006F, 0x0020,
    0x5BF9, 0x0020, 0x0050, 0x004E, 0x0047, 0x002F, 0x004A, 0x0050, 0x0045,
    0x0047, 0x002F, 0x0057, 0x0065, 0x0062, 0x0050, 0x002F, 0x0041, 0x0050,
    0x004E, 0x0047, 0x0020, 0x56FE, 0x7247, 0x6267, 0x884C, 0x672C, 0x5730,
    0x538B, 0x7F29
)
$compressLevelLabel = ConvertFrom-UnicodeCodePoints @(0x538B, 0x7F29, 0x7EA7, 0x522B)
$qualityLabel = ConvertFrom-UnicodeCodePoints @(0x8D28, 0x91CF)
$losslessLabel = ConvertFrom-UnicodeCodePoints @(0x65E0, 0x635F, 0x538B, 0x7F29)

if ($ArtName -eq "__AUTO_ART_NAME__") {
    $ArtName = $imageCompressLabel
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $StoreRoot -or $StoreRoot.Trim().Length -eq 0) {
    $StoreRoot = Join-Path $repoRoot ".loom-art-store-data"
}
if (-not $ControlPlaneRoot -or $ControlPlaneRoot.Trim().Length -eq 0) {
    $ControlPlaneRoot = Join-Path $env:APPDATA "Loom\control-plane"
}

$pingoUrl = "https://css-ig.net/bin/pingo-win64.zip"
$workRoot = Join-Path $repoRoot "target\art-packages\image-compress"
$downloadRoot = Join-Path $workRoot "download"
$extractRoot = Join-Path $workRoot "extract"
$stageRoot = Join-Path $workRoot "stage"
$packagePath = Join-Path $workRoot "$ArtId.zip"

New-Item -ItemType Directory -Force -Path $downloadRoot | Out-Null
New-Item -ItemType Directory -Force -Path $workRoot | Out-Null

$pingoZipPath = Join-Path $downloadRoot "pingo-win64.zip"
if ($ForceDownload -or -not (Test-Path -LiteralPath $pingoZipPath -PathType Leaf)) {
    Write-Host "Downloading official Pingo package..." -ForegroundColor Cyan
    curl.exe -L $pingoUrl -o $pingoZipPath
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to download $pingoUrl (exit $LASTEXITCODE)"
    }
}

Remove-Item -Recurse -Force $extractRoot, $stageRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $extractRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $stageRoot "bin") | Out-Null

tar -xf $pingoZipPath -C $extractRoot
if ($LASTEXITCODE -ne 0) {
    throw "Failed to extract $pingoZipPath with tar (exit $LASTEXITCODE)"
}

$pingoExe = Get-ChildItem -Path $extractRoot -Recurse -Filter "pingo.exe" | Select-Object -First 1
if ($null -eq $pingoExe) {
    throw "Downloaded package did not contain pingo.exe"
}

$bundledPingoPath = Join-Path $stageRoot "bin\pingo.exe"
Copy-Item -LiteralPath $pingoExe.FullName -Destination $bundledPingoPath -Force
$pingoSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $bundledPingoPath).Hash.ToLowerInvariant()

$manifest = [ordered]@{
    id = $ArtId
    name = $ArtName
    description = $imageCompressDescription
    enabled = $true
    execution = [ordered]@{
        type = "cli_wrapper"
        command = "bin/pingo.exe"
        args = @(
            "-s{{level_num}}"
            "-quality={{quality_num}}"
            "{{-lossless}}"
            "{{output}}"
        )
    }
    outputs = @(
        [ordered]@{
            name = "output"
            label = "output"
            type = "image"
            execution_type = "image_buffer"
        }
    )
    params = @(
        [ordered]@{
            id = "level_num"
            label = $compressLevelLabel
            widget = "slider"
            default = 2
            min = 1
            max = 4
            step = 1
            options = $null
            multiline = $null
            disabled = $false
            data_type = "number"
        }
        [ordered]@{
            id = "quality_num"
            label = $qualityLabel
            widget = "slider"
            default = 90
            min = 60
            max = 100
            step = 1
            options = $null
            multiline = $null
            disabled = $false
            data_type = "number"
        }
        [ordered]@{
            id = "lossless"
            label = $losslessLabel
            widget = "checkbox"
            default = $false
            min = $null
            max = $null
            step = $null
            options = $null
            multiline = $null
            disabled = $false
            data_type = "bool"
        }
    )
    metadata = [ordered]@{
        dependencies = [ordered]@{
            framework = "cli_wrapper"
            binaries = @(
                [ordered]@{
                    name = "bin/pingo.exe"
                    sha256 = $pingoSha256
                }
            )
        }
        artloomCompat = [ordered]@{
            defaults = [ordered]@{}
            executionType = "cli_wrapper"
            icon = "#52c41a"
            source = "loom-local"
            execution = [ordered]@{
                command = "bin/pingo.exe"
                args = "-s{{level_num}} -quality={{quality_num}} {{-lossless}} {{output}}"
                outputs = @(
                    [ordered]@{
                        name = "output"
                        label = "output"
                        type = "image"
                        execution_type = "image_buffer"
                    }
                )
                sourceType = "installed"
            }
        }
    }
}

$manifestPath = Join-Path $stageRoot "manifest.json"
[System.IO.File]::WriteAllText(
    $manifestPath,
    ($manifest | ConvertTo-Json -Depth 30),
    [System.Text.UTF8Encoding]::new($false)
)

if (Test-Path -LiteralPath $packagePath -PathType Leaf) {
    Remove-Item -LiteralPath $packagePath -Force
}

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::Open(
    $packagePath,
    [System.IO.Compression.ZipArchiveMode]::Create
)
try {
    $manifestEntry = $zip.CreateEntry("manifest.json")
    $manifestStream = $manifestEntry.Open()
    try {
        $manifestBytes = [System.Text.Encoding]::UTF8.GetBytes((Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8))
        $manifestStream.Write($manifestBytes, 0, $manifestBytes.Length)
    }
    finally {
        $manifestStream.Dispose()
    }

    $binaryEntry = $zip.CreateEntry("bin/pingo.exe")
    $binaryStream = $binaryEntry.Open()
    try {
        $binaryBytes = [System.IO.File]::ReadAllBytes($bundledPingoPath)
        $binaryStream.Write($binaryBytes, 0, $binaryBytes.Length)
    }
    finally {
        $binaryStream.Dispose()
    }
}
finally {
    $zip.Dispose()
}

$publishedZipPath = $null
if (-not $SkipPublish) {
    $artsRoot = Join-Path $StoreRoot "arts"
    New-Item -ItemType Directory -Force -Path $artsRoot | Out-Null
    $publishedZipPath = Join-Path $artsRoot "$ArtId.zip"
    Copy-Item -LiteralPath $packagePath -Destination $publishedZipPath -Force
}

$installReport = $null
if (-not $SkipInstall) {
    if ($InstallMode -eq "local") {
        $artDir = Join-Path $ControlPlaneRoot ("arts\" + $ArtId)
        $toolsDir = Join-Path $ControlPlaneRoot "tools"
        $toolsPath = Join-Path $toolsDir "tools.json"

        Remove-Item -Recurse -Force $artDir -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $artDir, $toolsDir | Out-Null
        Get-ChildItem -LiteralPath $stageRoot -Force | Copy-Item -Destination $artDir -Recurse -Force

        $tool = $manifest | ConvertTo-Json -Depth 30 | ConvertFrom-Json
        $tool.execution.command = (Join-Path $artDir "bin\pingo.exe")
        if ($null -ne $tool.metadata -and $null -ne $tool.metadata.artloomCompat -and $null -ne $tool.metadata.artloomCompat.execution) {
            $tool.metadata.artloomCompat.execution.command = $tool.execution.command
        }

        $tools = @()
        if (Test-Path -LiteralPath $toolsPath -PathType Leaf) {
            $parsed = Get-Content -LiteralPath $toolsPath -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($parsed -is [System.Array]) {
                $tools = @($parsed)
            }
            elseif ($null -ne $parsed) {
                $tools = @($parsed)
            }
        }

        $remaining = @($tools | Where-Object { [string]$_.id -ne $ArtId })
        $nextTools = @($remaining + $tool) | Sort-Object { [string]$_.id }
        [System.IO.File]::WriteAllText(
            $toolsPath,
            (($nextTools | ConvertTo-Json -Depth 40) + [Environment]::NewLine),
            [System.Text.UTF8Encoding]::new($false)
        )

        try {
            $null = Invoke-RestMethod `
                -Uri ($BaseUrl.TrimEnd('/') + "/v1/artloom-compat/arts/broadcast-updated") `
                -Method Post `
                -ContentType "application/json" `
                -Body "{}" `
                -TimeoutSec 15
        }
        catch {
            # Best effort only; direct local install still succeeds even if no
            # Hook bridge client is connected to consume the broadcast.
        }

        $installReport = [ordered]@{
            mode = "local"
            artDir = $artDir
            toolsPath = $toolsPath
            command = $tool.execution.command
        }
    }
    elseif ($InstallMode -eq "store") {
        if ($SkipPublish) {
            throw "InstallMode=store requires the package to be published into StoreRoot. Remove -SkipPublish or switch to -InstallMode upload."
        }
        try {
            $null = Invoke-RestMethod -Uri ($StoreUrl.TrimEnd('/') + "/health") -Method Get -TimeoutSec 5
        }
        catch {
            throw "InstallMode=store requires a running Loom art store at $StoreUrl. Start it first, for example with scripts/run-art-store.ps1."
        }
        $installBody = @{ artId = $ArtId; store = $StoreUrl } | ConvertTo-Json -Depth 5
        $installReport = Invoke-RestMethod `
            -Uri ($BaseUrl.TrimEnd('/') + "/v1/arts/store/install") `
            -Method Post `
            -ContentType "application/json" `
            -Body $installBody `
            -TimeoutSec 120
    }
    else {
        $zipBytes = [System.IO.File]::ReadAllBytes($packagePath)
        $zipBase64 = "data:application/zip;base64," + [Convert]::ToBase64String($zipBytes)
        $installBody = @{ zipBase64 = $zipBase64 } | ConvertTo-Json -Depth 5
        $installReport = Invoke-RestMethod `
            -Uri ($BaseUrl.TrimEnd('/') + "/v1/arts/install") `
            -Method Post `
            -ContentType "application/json" `
            -Body $installBody `
            -TimeoutSec 120
    }
}

$result = [ordered]@{
    artId = $ArtId
    artName = $ArtName
    pingoUrl = $pingoUrl
    pingoVersion = "1.28.4"
    pingoZipPath = $pingoZipPath
    bundledPingoPath = $bundledPingoPath
    pingoSha256 = $pingoSha256
    packagePath = $packagePath
    publishedZipPath = $publishedZipPath
    storeUrl = $StoreUrl
    controlPlaneRoot = $ControlPlaneRoot
    installMode = $InstallMode
    installReport = $installReport
}

$result | ConvertTo-Json -Depth 20
