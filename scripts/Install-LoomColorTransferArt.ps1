<#
.SYNOPSIS
    Build and install Loom's Color Transfer python_art package.

.DESCRIPTION
    Packages `Color Transfer (RBF)` as a formal Loom-managed `python_art` Art:
      - stages a Loom-local python_art runtime bundle;
      - vendors the Color Transfer Python Art source;
      - downloads CPython 3.12 Windows wheels for numpy + Pillow;
      - builds a portable Art ZIP plus a `python_art.zip` framework runtime ZIP;
      - optionally publishes those ZIPs into the local `.loom-art-store-data`; and
      - installs the framework/runtime and Art into `%APPDATA%\Loom\control-plane`.

    The generated tool keeps the live production tool id by default so existing
    Hook/Loom nodes continue to resolve after installation.

.PARAMETER BaseUrl
    Loom daemon base URL used for best-effort notifications and optional
    upload/store installs. Default http://127.0.0.1:8765.

.PARAMETER ArtId
    Tool/art id to install. Defaults to the existing Color Transfer tool id.

.PARAMETER ArtName
    Display name. Defaults to the legacy Color Transfer label.

.PARAMETER StoreRoot
    Local art-store root used when publishing the generated ZIPs. Defaults to
    <repo>/.loom-art-store-data.

.PARAMETER StoreUrl
    Local art-store URL used when InstallMode=store. Default
    http://127.0.0.1:8790.

.PARAMETER ControlPlaneRoot
    Loom control-plane root. Defaults to %APPDATA%\Loom\control-plane.

.PARAMETER InstallMode
    Install strategy:
      - local  : copy framework runtime + Art directly into the control-plane
      - store  : publish ZIPs and ask Loom to install the Art from StoreUrl
      - upload : upload the full Art ZIP payload to /v1/arts/install
    Default local because the packaged Art is large once numpy/Pillow are
    vendored and should also work without a running store process.

.PARAMETER SkipInstall
    Only build/publish the packages; do not install anything.

.PARAMETER SkipPublish
    Do not copy the generated ZIPs into StoreRoot.

.PARAMETER ForceDependencyDownload
    Always re-download the CPython 3.12 wheel payloads.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass `
      -File .\scripts\Install-LoomColorTransferArt.ps1
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [string]$ArtId = "custom-1770131241684",
    [string]$ArtName = "__AUTO_ART_NAME__",
    [string]$StoreRoot,
    [string]$StoreUrl = "http://127.0.0.1:8790",
    [string]$ControlPlaneRoot,
    [ValidateSet("local", "store", "upload")]
    [string]$InstallMode = "local",
    [switch]$SkipInstall,
    [switch]$SkipPublish,
    [switch]$ForceDependencyDownload
)

$ErrorActionPreference = "Stop"

function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
}

function Write-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$Content
    )

    [System.IO.File]::WriteAllText(
        $Path,
        $Content,
        [System.Text.UTF8Encoding]::new($false)
    )
}

function Read-JsonArrayFile {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return @()
    }

    $parsed = Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($parsed -is [System.Array]) {
        return @($parsed)
    }
    if ($null -eq $parsed) {
        return @()
    }
    return @($parsed)
}

function Expand-ZipIntoDirectory {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ZipPath,
        [Parameter(Mandatory = $true)]
        [string]$Destination
    )

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem

    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    $archive = [System.IO.Compression.ZipFile]::OpenRead($ZipPath)
    try {
        foreach ($entry in $archive.Entries) {
            if ([string]::IsNullOrEmpty($entry.FullName)) {
                continue
            }
            $targetPath = Join-Path $Destination $entry.FullName
            if ($entry.FullName.EndsWith('/')) {
                New-Item -ItemType Directory -Force -Path $targetPath | Out-Null
                continue
            }
            $parent = Split-Path -Parent $targetPath
            if (-not [string]::IsNullOrWhiteSpace($parent)) {
                New-Item -ItemType Directory -Force -Path $parent | Out-Null
            }
            [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $targetPath, $true)
        }
    }
    finally {
        $archive.Dispose()
    }
}

function Write-ZipFromDirectory {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceDir,
        [Parameter(Mandatory = $true)]
        [string]$ZipPath
    )

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem

    if (Test-Path -LiteralPath $ZipPath -PathType Leaf) {
        Remove-Item -LiteralPath $ZipPath -Force
    }
    $zipParent = Split-Path -Parent $ZipPath
    if (-not [string]::IsNullOrWhiteSpace($zipParent)) {
        New-Item -ItemType Directory -Force -Path $zipParent | Out-Null
    }
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $SourceDir,
        $ZipPath,
        [System.IO.Compression.CompressionLevel]::Optimal,
        $false
    )
}

function Get-EmbeddedPythonVersionTag {
    param([string]$PythonExe)

    $version = & $PythonExe --version 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to probe embedded Python version from $PythonExe"
    }

    if ($version -match 'Python\s+(\d+)\.(\d+)') {
        return "cp$($Matches[1])$($Matches[2])"
    }
    throw "Could not parse embedded Python version from: $version"
}

function Download-PythonArtDependencyWheels {
    param(
        [Parameter(Mandatory = $true)]
        [string]$WheelRoot,
        [Parameter(Mandatory = $true)]
        [string]$PythonTag
    )

    $numpyVersion = "1.26.4"
    $pillowVersion = "11.1.0"

    if ($ForceDependencyDownload) {
        Remove-Item -Recurse -Force -LiteralPath $WheelRoot -ErrorAction SilentlyContinue
    }
    New-Item -ItemType Directory -Force -Path $WheelRoot | Out-Null

    $existingWheels = @(Get-ChildItem -LiteralPath $WheelRoot -Filter *.whl -ErrorAction SilentlyContinue)
    if ($existingWheels.Count -ge 2) {
        return [ordered]@{
            numpyVersion = $numpyVersion
            pillowVersion = $pillowVersion
            wheelCount = $existingWheels.Count
        }
    }

    $pythonVersionDigits = $PythonTag.Substring(2)
    $downloadArgs = @(
        "-m", "pip", "download",
        "--disable-pip-version-check",
        "--only-binary=:all:",
        "--platform", "win_amd64",
        "--implementation", "cp",
        "--python-version", $pythonVersionDigits,
        "--abi", $PythonTag,
        "--dest", $WheelRoot,
        "numpy==$numpyVersion",
        "Pillow==$pillowVersion"
    )

    & python @downloadArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to download numpy/Pillow wheels for $PythonTag (exit $LASTEXITCODE)"
    }

    return [ordered]@{
        numpyVersion = $numpyVersion
        pillowVersion = $pillowVersion
        wheelCount = @(
            Get-ChildItem -LiteralPath $WheelRoot -Filter *.whl -ErrorAction SilentlyContinue
        ).Count
    }
}

function Install-FrameworkRuntimeLocally {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RuntimeStageRoot,
        [Parameter(Mandatory = $true)]
        [string]$ControlPlaneRoot
    )

    $frameworksPath = Join-Path $ControlPlaneRoot "frameworks.json"
    $runtimeDir = Join-Path $ControlPlaneRoot "framework-runtimes\python_art"

    Remove-Item -Recurse -Force -LiteralPath $runtimeDir -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null
    Get-ChildItem -LiteralPath $RuntimeStageRoot -Force | Copy-Item -Destination $runtimeDir -Recurse -Force

    $installedFrameworks = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($id in @("cli_wrapper", "cloud_api", "script", "workflow")) {
        $null = $installedFrameworks.Add($id)
    }
    foreach ($id in Read-JsonArrayFile -Path $frameworksPath) {
        $null = $installedFrameworks.Add([string]$id)
    }
    $null = $installedFrameworks.Add("python_art")
    $orderedFrameworks = @($installedFrameworks) | Sort-Object
    Write-Utf8NoBomFile -Path $frameworksPath -Content (($orderedFrameworks | ConvertTo-Json -Depth 5) + [Environment]::NewLine)

    return [ordered]@{
        frameworksPath = $frameworksPath
        runtimeDir = $runtimeDir
        pythonExe = Join-Path $runtimeDir "python-embed\python.exe"
        launcherPath = Join-Path $runtimeDir "python\Launcher.py"
    }
}

$colorTransferLabel = "Color Transfer (RBF)"
$shaderResultLabel = ConvertFrom-UnicodeCodePoints @(0x7ED3, 0x679C)
$referenceLabel = ConvertFrom-UnicodeCodePoints @(0x53C2, 0x8003, 0x56FE, 0x7247)

if ($ArtName -eq "__AUTO_ART_NAME__") {
    $ArtName = $colorTransferLabel
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $StoreRoot -or $StoreRoot.Trim().Length -eq 0) {
    $StoreRoot = Join-Path $repoRoot ".loom-art-store-data"
}
if (-not $ControlPlaneRoot -or $ControlPlaneRoot.Trim().Length -eq 0) {
    $ControlPlaneRoot = Join-Path $env:APPDATA "Loom\control-plane"
}

$sourceArtRoot = "Z:\project\project\ArtNexus\ArtLoom\python\Arts\Art_ColorTransfer"
$sourceArtJsonPath = Join-Path $sourceArtRoot "art.json"
$sourceMainPath = Join-Path $sourceArtRoot "main.py"
if (-not (Test-Path -LiteralPath $sourceArtJsonPath -PathType Leaf)) {
    throw "Color Transfer source art.json was not found: $sourceArtJsonPath"
}
if (-not (Test-Path -LiteralPath $sourceMainPath -PathType Leaf)) {
    throw "Color Transfer source main.py was not found: $sourceMainPath"
}

$sourceArtJson = Get-Content -LiteralPath $sourceArtJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
$sourceMain = Get-Content -LiteralPath $sourceMainPath -Raw -Encoding UTF8

$embeddedPythonExe = Join-Path $repoRoot "resources\python-embed\python.exe"
$launcherSourcePath = Join-Path $repoRoot "resources\python\Launcher.py"
$pythonEmbedSourceRoot = Join-Path $repoRoot "resources\python-embed"
if (-not (Test-Path -LiteralPath $embeddedPythonExe -PathType Leaf)) {
    throw "Embedded python executable was not found: $embeddedPythonExe"
}
if (-not (Test-Path -LiteralPath $launcherSourcePath -PathType Leaf)) {
    throw "Launcher.py was not found: $launcherSourcePath"
}
if (-not (Test-Path -LiteralPath $pythonEmbedSourceRoot -PathType Container)) {
    throw "python-embed resource directory was not found: $pythonEmbedSourceRoot"
}

$pythonTag = Get-EmbeddedPythonVersionTag -PythonExe $embeddedPythonExe

$workRoot = Join-Path $repoRoot "target\art-packages\color-transfer"
$wheelRoot = Join-Path $workRoot "wheels"
$stageRoot = Join-Path $workRoot "stage"
$artPluginRoot = Join-Path $stageRoot "python\Arts\Art_ColorTransfer"
$pluginSitePackagesRoot = Join-Path $artPluginRoot "site-packages"
$runtimeStageRoot = Join-Path $workRoot "runtime-stage"
$packagePath = Join-Path $workRoot "$ArtId.zip"
$runtimeZipPath = Join-Path $workRoot "python_art.zip"

Remove-Item -Recurse -Force -LiteralPath $stageRoot, $runtimeStageRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $workRoot, $artPluginRoot, $pluginSitePackagesRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $runtimeStageRoot "python") | Out-Null

$wheelReport = Download-PythonArtDependencyWheels -WheelRoot $wheelRoot -PythonTag $pythonTag
foreach ($wheel in Get-ChildItem -LiteralPath $wheelRoot -Filter *.whl) {
    Expand-ZipIntoDirectory -ZipPath $wheel.FullName -Destination $pluginSitePackagesRoot
}

Copy-Item -LiteralPath $sourceArtJsonPath -Destination (Join-Path $artPluginRoot "art.json") -Force
Copy-Item -LiteralPath $sourceMainPath -Destination (Join-Path $artPluginRoot "main.py") -Force

Copy-Item -LiteralPath $launcherSourcePath -Destination (Join-Path $runtimeStageRoot "python\Launcher.py") -Force
Copy-Item -LiteralPath $pythonEmbedSourceRoot -Destination (Join-Path $runtimeStageRoot "python-embed") -Recurse -Force

$params = New-Object System.Collections.Generic.List[object]
foreach ($input in @($sourceArtJson.signature.inputs)) {
    if ([string]$input.id -eq "input") {
        continue
    }
    $params.Add([ordered]@{
        id = [string]$input.id
        label = if ([string]::IsNullOrWhiteSpace([string]$input.label)) { $referenceLabel } else { [string]$input.label }
        widget = "image_link"
        default = ""
        disabled = $false
        data_type = "image_path"
    }) | Out-Null
}
foreach ($variable in @($sourceArtJson.variables)) {
    $rawType = [string]$variable.type
    $dataType = switch ($rawType.ToLowerInvariant()) {
        "boolean" { "bool" }
        "float" { "number" }
        "int" { "number" }
        default { if ([string]::IsNullOrWhiteSpace($rawType)) { "string" } else { $rawType } }
    }

    $params.Add([ordered]@{
        id = [string]$variable.id
        label = [string]$variable.label
        widget = [string]$variable.widget
        default = $variable.default
        min = $variable.min
        max = $variable.max
        step = $variable.step
        options = $variable.options
        multiline = $variable.multiline
        disabled = $false
        data_type = $dataType
    }) | Out-Null
}

$inputs = @()
foreach ($input in @($sourceArtJson.signature.inputs)) {
    $inputs += [ordered]@{
        name = [string]$input.id
        label = [string]$input.label
        type = "image"
        execution_type = "image_path"
    }
}

$outputs = @(
    [ordered]@{
        name = "output"
        label = if ([string]::IsNullOrWhiteSpace([string]$sourceArtJson.signature.outputs[0].label)) { $shaderResultLabel } else { [string]$sourceArtJson.signature.outputs[0].label }
        type = "image"
        execution_type = "image_path"
        captureMode = "explicit_path"
    }
)

$relativeArtPath = "python/Arts/Art_ColorTransfer"
$relativePythonPath = "python/Arts/Art_ColorTransfer/main.py"
$compatExecution = [ordered]@{}
$compatExecution.artPath = $relativeArtPath
$compatExecution.pythonPath = $relativePythonPath
$compatExecution.entry = "main.py"
$compatExecution.outputs = $outputs
$compatExecution.sourceType = "installed"

$compatMetadata = [ordered]@{}
$compatMetadata.defaults = [ordered]@{}
$compatMetadata.executionType = "shader"
$compatMetadata.icon = "#fa8c16"
$compatMetadata.source = "loom-local"
$compatMetadata.execution = $compatExecution

$metadata = [ordered]@{}
$metadata.dependencies = [ordered]@{ framework = "python_art" }
$metadata.artloomCompat = $compatMetadata

$execution = [ordered]@{}
$execution.type = "python_art"
$execution.artId = [string]$sourceArtJson.art_id
$execution.artPath = $relativeArtPath

$manifest = [ordered]@{}
$manifest.id = $ArtId
$manifest.name = $ArtName
$manifest.description = [string]$sourceArtJson.description
$manifest.enabled = $true
$manifest.execution = $execution
$manifest.inputs = $inputs
$manifest.outputs = $outputs
$manifest.params = [object[]]$params.ToArray()
$manifest.metadata = $metadata

Write-Utf8NoBomFile -Path (Join-Path $stageRoot "manifest.json") -Content ($manifest | ConvertTo-Json -Depth 50)
Write-ZipFromDirectory -SourceDir $stageRoot -ZipPath $packagePath
Write-ZipFromDirectory -SourceDir $runtimeStageRoot -ZipPath $runtimeZipPath

$publishedZipPath = $null
$publishedRuntimeZipPath = $null
if (-not $SkipPublish) {
    $artsRoot = Join-Path $StoreRoot "arts"
    $frameworksRoot = Join-Path $StoreRoot "frameworks"
    New-Item -ItemType Directory -Force -Path $artsRoot, $frameworksRoot | Out-Null
    $publishedZipPath = Join-Path $artsRoot "$ArtId.zip"
    $publishedRuntimeZipPath = Join-Path $frameworksRoot "python_art.zip"
    Copy-Item -LiteralPath $packagePath -Destination $publishedZipPath -Force
    Copy-Item -LiteralPath $runtimeZipPath -Destination $publishedRuntimeZipPath -Force
}

$runtimeInstallReport = $null
$installReport = $null
if (-not $SkipInstall) {
    $runtimeInstallReport = Install-FrameworkRuntimeLocally -RuntimeStageRoot $runtimeStageRoot -ControlPlaneRoot $ControlPlaneRoot

    if ($InstallMode -eq "local") {
        $artDir = Join-Path $ControlPlaneRoot ("arts\" + $ArtId)
        $toolsDir = Join-Path $ControlPlaneRoot "tools"
        $toolsPath = Join-Path $toolsDir "tools.json"
        $absoluteArtPath = Join-Path $artDir "python\Arts\Art_ColorTransfer"
        $absolutePythonPath = Join-Path $absoluteArtPath "main.py"

        Remove-Item -Recurse -Force -LiteralPath $artDir -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $artDir, $toolsDir | Out-Null
        Get-ChildItem -LiteralPath $stageRoot -Force | Copy-Item -Destination $artDir -Recurse -Force

        $tool = $manifest | ConvertTo-Json -Depth 50 | ConvertFrom-Json
        $tool.execution.artPath = $absoluteArtPath
        if ($null -ne $tool.metadata -and
            $null -ne $tool.metadata.artloomCompat -and
            $null -ne $tool.metadata.artloomCompat.execution) {
            $tool.metadata.artloomCompat.execution.artPath = $absoluteArtPath
            $tool.metadata.artloomCompat.execution.pythonPath = $absolutePythonPath
        }

        $tools = Read-JsonArrayFile -Path $toolsPath
        $remaining = @($tools | Where-Object { [string]$_.id -ne $ArtId })
        $nextTools = @($remaining + $tool) | Sort-Object { [string]$_.id }
        Write-Utf8NoBomFile -Path $toolsPath -Content (($nextTools | ConvertTo-Json -Depth 60) + [Environment]::NewLine)

        try {
            $null = Invoke-RestMethod `
                -Uri ($BaseUrl.TrimEnd('/') + "/v1/artloom-compat/arts/broadcast-updated") `
                -Method Post `
                -ContentType "application/json" `
                -Body "{}" `
                -TimeoutSec 15
        }
        catch {
        }

        $installReport = [ordered]@{
            mode = "local"
            artDir = $artDir
            toolsPath = $toolsPath
            absoluteArtPath = $absoluteArtPath
            absolutePythonPath = $absolutePythonPath
        }
    }
    elseif ($InstallMode -eq "store") {
        if ($SkipPublish) {
            throw "InstallMode=store requires the ZIPs to be published into StoreRoot. Remove -SkipPublish or switch to -InstallMode local."
        }
        try {
            $null = Invoke-RestMethod -Uri ($StoreUrl.TrimEnd('/') + "/health") -Method Get -TimeoutSec 5
        }
        catch {
            throw "InstallMode=store requires a running Loom art store at $StoreUrl."
        }
        $installBody = @{ artId = $ArtId; store = $StoreUrl } | ConvertTo-Json -Depth 5
        $installReport = Invoke-RestMethod `
            -Uri ($BaseUrl.TrimEnd('/') + "/v1/arts/store/install") `
            -Method Post `
            -ContentType "application/json" `
            -Body $installBody `
            -TimeoutSec 600
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
            -TimeoutSec 600
    }
}

$result = [ordered]@{
    artId = $ArtId
    artName = $ArtName
    pythonArtId = [string]$sourceArtJson.art_id
    sourceArtRoot = $sourceArtRoot
    embeddedPythonExe = $embeddedPythonExe
    pythonTag = $pythonTag
    wheelReport = $wheelReport
    packagePath = $packagePath
    runtimeZipPath = $runtimeZipPath
    publishedZipPath = $publishedZipPath
    publishedRuntimeZipPath = $publishedRuntimeZipPath
    storeUrl = $StoreUrl
    controlPlaneRoot = $ControlPlaneRoot
    installMode = $InstallMode
    runtimeInstallReport = $runtimeInstallReport
    installReport = $installReport
}

$result | ConvertTo-Json -Depth 30
