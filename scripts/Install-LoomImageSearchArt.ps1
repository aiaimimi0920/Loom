<#
.SYNOPSIS
    Build and install Loom's image-search MCP art.

.DESCRIPTION
    Packages the image-search art as a formal Loom Art backed by the Brave Search MCP server:
      - builds a portable art zip containing only manifest.json
      - optionally publishes it into the local Loom art-store root
      - optionally configures the Brave Search MCP server in a running daemon
      - optionally installs the mcp framework plus the Art itself

    The generated Art mirrors Hook's expected MCP image-search shape:
      - toolName = brave_image_search
      - outputs = image
      - params = query/count/safesearch/spellcheck

.PARAMETER BaseUrl
    Loom daemon base URL used for optional server/framework/art installation.
    Default http://127.0.0.1:8765.

.PARAMETER ArtId
    Tool/art id to install. Defaults to custom-image-search.

.PARAMETER ArtName
    Display name. Defaults to the existing Chinese image-search label.

.PARAMETER ServerId
    MCP server id referenced by the Art. Defaults to brave-search.

.PARAMETER BraveApiKey
    Brave Search API key written into the MCP server env block when configuring
    the server through the daemon. Leave blank to keep the field empty.

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
      - local  : lay the packaged art into <control-plane>/arts/<id>/ and
                 update tools.json directly
      - store  : publish the zip into StoreRoot, then ask Loom to install it
                 from the local art store URL
      - upload : upload the full zip payload directly to /v1/arts/install
    Default upload because the package is tiny and does not depend on a local
    art-store process.

.PARAMETER SkipInstall
    Only build/publish/configure; do not install the Art itself.

.PARAMETER SkipPublish
    Do not copy the generated zip into the local art-store root.

.PARAMETER SkipServerConfig
    Do not PUT the Brave Search MCP server config into the daemon.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/Install-LoomImageSearchArt.ps1 `
      -BraveApiKey $env:BRAVE_API_KEY
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [string]$ArtId = "custom-image-search",
    [string]$ArtName = "__AUTO_ART_NAME__",
    [string]$ServerId = "brave-search",
    [string]$ServerName = "Brave Search",
    [string]$BraveApiKey = "",
    [string]$StoreRoot,
    [string]$StoreUrl = "http://127.0.0.1:8790",
    [string]$ControlPlaneRoot,
    [ValidateSet("local", "store", "upload")]
    [string]$InstallMode = "upload",
    [switch]$SkipInstall,
    [switch]$SkipPublish,
    [switch]$SkipServerConfig
)

$ErrorActionPreference = "Stop"

function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
}

$imageSearchLabel = ConvertFrom-UnicodeCodePoints @(0x56FE, 0x7247, 0x641C, 0x7D22)
$imageSearchDescription = ConvertFrom-UnicodeCodePoints @(
    0x901A, 0x8FC7, 0x0020, 0x0042, 0x0072, 0x0061, 0x0076, 0x0065, 0x0020,
    0x0053, 0x0065, 0x0061, 0x0072, 0x0063, 0x0068, 0x0020, 0x004D, 0x0043,
    0x0050, 0x0020, 0x641C, 0x7D22, 0x56FE, 0x7247, 0x5E76, 0x8FD4, 0x56DE,
    0x9996, 0x5F20, 0x53EF, 0x9884, 0x89C8, 0x7ED3, 0x679C
)
$queryLabel = ConvertFrom-UnicodeCodePoints @(0x641C, 0x7D22, 0x8BCD)
$countLabel = ConvertFrom-UnicodeCodePoints @(0x6570, 0x91CF)
$safeSearchLabel = ConvertFrom-UnicodeCodePoints @(0x5B89, 0x5168, 0x641C, 0x7D22)
$spellcheckLabel = ConvertFrom-UnicodeCodePoints @(0x62FC, 0x5199, 0x68C0, 0x67E5)

if ($ArtName -eq "__AUTO_ART_NAME__") {
    $ArtName = $imageSearchLabel
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $StoreRoot -or $StoreRoot.Trim().Length -eq 0) {
    $StoreRoot = Join-Path $repoRoot ".loom-art-store-data"
}
if (-not $ControlPlaneRoot -or $ControlPlaneRoot.Trim().Length -eq 0) {
    $ControlPlaneRoot = Join-Path $env:APPDATA "Loom\control-plane"
}

$workRoot = Join-Path $repoRoot "target\art-packages\image-search"
$stageRoot = Join-Path $workRoot "stage"
$packagePath = Join-Path $workRoot "$ArtId.zip"

Remove-Item -Recurse -Force $stageRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $workRoot, $stageRoot | Out-Null

$manifest = [ordered]@{
    id = $ArtId
    name = $ArtName
    description = $imageSearchDescription
    enabled = $true
    execution = [ordered]@{
        type = "mcp"
        serverId = $ServerId
        toolName = "brave_image_search"
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
            id = "query"
            label = $queryLabel
            widget = "text"
            default = ""
            disabled = $false
            data_type = "string"
        }
        [ordered]@{
            id = "count"
            label = $countLabel
            widget = "number"
            default = 1
            min = 1
            max = 20
            step = 1
            disabled = $false
            data_type = "number"
        }
        [ordered]@{
            id = "safesearch"
            label = $safeSearchLabel
            widget = "text"
            default = "off"
            disabled = $false
            data_type = "string"
        }
        [ordered]@{
            id = "spellcheck"
            label = $spellcheckLabel
            widget = "checkbox"
            default = $true
            disabled = $false
            data_type = "bool"
        }
    )
    metadata = [ordered]@{
        dependencies = [ordered]@{
            framework = "mcp"
        }
        artloomCompat = [ordered]@{
            defaults = [ordered]@{}
            executionType = "mcp"
            icon = "#1677ff"
            source = "loom-local"
            execution = [ordered]@{
                serverId = $ServerId
                toolName = "brave_image_search"
                outputs = @(
                    [ordered]@{
                        name = "output"
                        label = "output"
                        type = "image"
                        execution_type = "image_buffer"
                    }
                )
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
[System.IO.Compression.ZipFile]::CreateFromDirectory($stageRoot, $packagePath)

$publishedZipPath = $null
if (-not $SkipPublish) {
    $artsRoot = Join-Path $StoreRoot "arts"
    New-Item -ItemType Directory -Force -Path $artsRoot | Out-Null
    $publishedZipPath = Join-Path $artsRoot "$ArtId.zip"
    Copy-Item -LiteralPath $packagePath -Destination $publishedZipPath -Force
}

$serverConfig = $null
if (-not $SkipServerConfig) {
    $serverBody = @{
        id = $ServerId
        name = $ServerName
        command = "npx"
        args = @("-y", "@brave/brave-search-mcp-server", "--transport", "stdio")
        env = @{
            BRAVE_API_KEY = $BraveApiKey
        }
        enabled = $true
    } | ConvertTo-Json -Depth 20
    $serverConfig = Invoke-RestMethod `
        -Uri ($BaseUrl.TrimEnd('/') + "/v1/mcp/servers/$ServerId") `
        -Method Put `
        -ContentType "application/json" `
        -Body $serverBody `
        -TimeoutSec 30
}

$frameworkInstall = $null
$installReport = $null
if (-not $SkipInstall) {
    try {
        $frameworkInstall = Invoke-RestMethod `
            -Uri ($BaseUrl.TrimEnd('/') + "/v1/frameworks/mcp/install") `
            -Method Post `
            -ContentType "application/json" `
            -Body "{}" `
            -TimeoutSec 30
    }
    catch {
        if ($InstallMode -ne "local") {
            throw
        }
    }

    if ($InstallMode -eq "local") {
        $artDir = Join-Path $ControlPlaneRoot ("arts\" + $ArtId)
        $toolsDir = Join-Path $ControlPlaneRoot "tools"
        $toolsPath = Join-Path $toolsDir "tools.json"

        Remove-Item -Recurse -Force $artDir -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $artDir, $toolsDir | Out-Null
        Get-ChildItem -LiteralPath $stageRoot -Force | Copy-Item -Destination $artDir -Recurse -Force

        $tool = $manifest | ConvertTo-Json -Depth 30 | ConvertFrom-Json
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
        }

        $installReport = [ordered]@{
            mode = "local"
            artDir = $artDir
            toolsPath = $toolsPath
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
    serverId = $ServerId
    braveApiKeyConfigured = (-not [string]::IsNullOrWhiteSpace($BraveApiKey))
    packagePath = $packagePath
    publishedZipPath = $publishedZipPath
    storeUrl = $StoreUrl
    controlPlaneRoot = $ControlPlaneRoot
    installMode = $InstallMode
    serverConfig = $serverConfig
    frameworkInstall = $frameworkInstall
    installReport = $installReport
}

$result | ConvertTo-Json -Depth 20
