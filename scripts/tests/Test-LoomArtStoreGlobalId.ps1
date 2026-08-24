param(
    [string]$DaemonExecutable = ".\target\debug\loom-daemon.exe",
    [string]$ArtStoreExecutable = ".\target\debug\loom-art-store.exe",
    [string]$FrameworkArtifactRoot = ".loom-art-store-data\frameworks",
    [switch]$BuildFrameworkArtifacts
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$script:DaemonAuthToken = $null

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ([string]$Expected -ne [string]$Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
}

function Invoke-LoomJson {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)][string]$Url,
        [AllowNull()][object]$Body
    )

    $json = if ($null -eq $Body) { $null } else { $Body | ConvertTo-Json -Depth 50 -Compress }
    $headers = @{ Authorization = "Bearer $script:DaemonAuthToken" }
    if ($null -eq $json) {
        return Invoke-RestMethod -Method $Method -Uri $Url -Headers $headers -TimeoutSec 30
    }
    return Invoke-RestMethod -Method $Method -Uri $Url -Headers $headers -ContentType "application/json" -Body $json -TimeoutSec 120
}

function Wait-HttpReady {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$Label
    )

    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        Start-Sleep -Milliseconds 250
        try {
            $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 2
            if ([int]$response.StatusCode -eq 200) {
                return
            }
        }
        catch {
            if ($Process.HasExited) {
                throw "$Label exited before readiness."
            }
        }
    }
    throw "$Label did not become ready."
}

function Install-FrameworkZip {
    param(
        [Parameter(Mandatory = $true)][string]$BaseUrl,
        [Parameter(Mandatory = $true)][string]$ZipPath
    )

    $bytes = [System.IO.File]::ReadAllBytes($ZipPath)
    $encoded = "data:application/zip;base64,$([Convert]::ToBase64String($bytes))"
    return Invoke-LoomJson -Method Post -Url "$BaseUrl/v1/frameworks/install" -Body @{ zipBase64 = $encoded }
}

function New-PublishFixtureTool {
    param([Parameter(Mandatory = $true)][string]$Version, [Parameter(Mandatory = $true)][string]$Endpoint)

    return [ordered]@{
        id = "authored-global-id-repository"
        name = "Global ID Publish Test"
        description = "Platform global Art ID fixture"
        enabled = $true
        execution = @{
            type = "cloud_api"
            endpoint = $Endpoint
            method = "GET"
            contentType = "application/json"
            headers = "{}"
            body = "{}"
        }
        inputs = @()
        outputs = @()
        params = @()
        metadata = @{
            packageSecurity = @{ version = $Version }
            dependencies = @{ framework = "cloud_api" }
            authoring = @{ origin = "local"; owner = "local-user" }
        }
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$daemonPath = if ([System.IO.Path]::IsPathRooted($DaemonExecutable)) {
    [System.IO.Path]::GetFullPath($DaemonExecutable)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $DaemonExecutable))
}
$artStorePath = if ([System.IO.Path]::IsPathRooted($ArtStoreExecutable)) {
    [System.IO.Path]::GetFullPath($ArtStoreExecutable)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArtStoreExecutable))
}
$frameworkRootPath = if ([System.IO.Path]::IsPathRooted($FrameworkArtifactRoot)) {
    [System.IO.Path]::GetFullPath($FrameworkArtifactRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $FrameworkArtifactRoot))
}

Assert-True (Test-Path -LiteralPath $daemonPath -PathType Leaf) "Loom daemon executable not found: $daemonPath"
Assert-True (Test-Path -LiteralPath $artStorePath -PathType Leaf) "Loom Art Store executable not found: $artStorePath"
$cloudFrameworkZip = Join-Path $frameworkRootPath "cloud_api.zip"
if ($BuildFrameworkArtifacts) {
    $frameworkBuilder = Join-Path $repoRoot "scripts\Build-LoomArtFrameworkPackages.ps1"
    Assert-True (Test-Path -LiteralPath $frameworkBuilder -PathType Leaf) "Framework package builder not found: $frameworkBuilder"
    & $frameworkBuilder -Configuration Debug -OutputRoot $frameworkRootPath
    if ($LASTEXITCODE -ne 0) {
        throw "Framework package builder failed with exit code $LASTEXITCODE."
    }
}
Assert-True (Test-Path -LiteralPath $cloudFrameworkZip -PathType Leaf) "Cloud framework ZIP not found: $cloudFrameworkZip"

$root = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-global-art-id-" + [guid]::NewGuid().ToString("N"))
$storeRoot = Join-Path $root "store"
$daemonRoot = Join-Path $root "daemon"
$configurationRoot = Join-Path $daemonRoot "configuration"
$installDaemonRoot = Join-Path $root "install-daemon"
$installConfigurationRoot = Join-Path $installDaemonRoot "configuration"
$storePort = Get-FreePort
$daemonPort = Get-FreePort
$installDaemonPort = Get-FreePort
$storeUrl = "http://127.0.0.1:$storePort"
$daemonUrl = "http://127.0.0.1:$daemonPort"
$installDaemonUrl = "http://127.0.0.1:$installDaemonPort"
$storeStdout = Join-Path $root "art-store.stdout.log"
$storeStderr = Join-Path $root "art-store.stderr.log"
$daemonStdout = Join-Path $root "daemon.stdout.log"
$daemonStderr = Join-Path $root "daemon.stderr.log"
$installDaemonStdout = Join-Path $root "install-daemon.stdout.log"
$installDaemonStderr = Join-Path $root "install-daemon.stderr.log"
$storeProcess = $null
$daemonProcess = $null
$installDaemonProcess = $null
$succeeded = $false
$environmentNames = @(
    "LOOM_ART_STORE_HOST",
    "LOOM_ART_STORE_PORT",
    "LOOM_ART_STORE_ROOT",
    "LOOM_ART_STORE_URL",
    "LOOM_DAEMON_HOST",
    "LOOM_DAEMON_PORT",
    "LOOM_DAEMON_TOKEN",
    "LOOM_CONTROL_PLANE_ROOT",
    "LOOM_CONFIGURATION_ROOT",
    "LOOM_RUN_STORE_PATH"
)
$oldEnvironment = @{}
foreach ($name in $environmentNames) {
    $oldEnvironment[$name] = [System.Environment]::GetEnvironmentVariable($name)
}

New-Item -ItemType Directory -Force -Path $root, $storeRoot, $daemonRoot, $configurationRoot, $installDaemonRoot, $installConfigurationRoot | Out-Null
$tokenBytes = New-Object byte[] 32
$tokenGenerator = [System.Security.Cryptography.RandomNumberGenerator]::Create()
try {
    $tokenGenerator.GetBytes($tokenBytes)
}
finally {
    $tokenGenerator.Dispose()
}
$script:DaemonAuthToken = [Convert]::ToBase64String($tokenBytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')

try {
    $env:LOOM_ART_STORE_HOST = "127.0.0.1"
    $env:LOOM_ART_STORE_PORT = [string]$storePort
    $env:LOOM_ART_STORE_ROOT = $storeRoot
    $storeProcess = Start-Process -FilePath $artStorePath -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput $storeStdout -RedirectStandardError $storeStderr
    Wait-HttpReady -Process $storeProcess -Url "$storeUrl/health" -Label "Loom Art Store"

    $env:LOOM_ART_STORE_URL = $storeUrl
    $env:LOOM_DAEMON_HOST = "127.0.0.1"
    $env:LOOM_DAEMON_PORT = [string]$daemonPort
    $env:LOOM_DAEMON_TOKEN = $script:DaemonAuthToken
    $env:LOOM_CONTROL_PLANE_ROOT = $daemonRoot
    $env:LOOM_CONFIGURATION_ROOT = $configurationRoot
    $env:LOOM_RUN_STORE_PATH = Join-Path $daemonRoot "runs.sqlite3"
    $daemonProcess = Start-Process -FilePath $daemonPath -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput $daemonStdout -RedirectStandardError $daemonStderr
    Wait-HttpReady -Process $daemonProcess -Url "$daemonUrl/health" -Label "Loom daemon"

    $publisherIdentity = Invoke-LoomJson -Method Get -Url "$daemonUrl/v1/publisher-identity" -Body $null
    Assert-Equal "L0000000000" ([string]$publisherIdentity.identity.userId) "Publisher identity did not receive the default test user ID."
    Assert-True ([bool]$publisherIdentity.hasPrivateKey) "Publisher identity did not persist its private key."

    $installedFramework = Install-FrameworkZip -BaseUrl $daemonUrl -ZipPath $cloudFrameworkZip
    Assert-Equal "cloud_api" ([string]$installedFramework.framework.id) "Cloud framework install returned the wrong ID."
    Assert-True ([bool]$installedFramework.framework.ready) "Cloud framework was not ready after installation."
    $repositoryName = "authored-global-id-repository"
    $firstTool = New-PublishFixtureTool -Version "0.1.0" -Endpoint "$daemonUrl/health"
    $null = Invoke-LoomJson -Method Post -Url "$daemonUrl/v1/arts/create" -Body @{ tool = $firstTool; files = @() }

    $beforePublish = Invoke-LoomJson -Method Get -Url "$daemonUrl/v1/tools" -Body $null
    $localTool = @($beforePublish.tools | Where-Object { [string]$_.id -eq $repositoryName }) | Select-Object -First 1
    Assert-True ($null -ne $localTool) "Locally authored Art was not registered."
    Assert-True (-not (($localTool | ConvertTo-Json -Depth 40 -Compress).Contains('"globalId"'))) "Local creation must not assign a platform global ID."

    $firstPublish = Invoke-LoomJson -Method Post -Url "$daemonUrl/v1/arts/store/publish" -Body @{ artId = $repositoryName }
    Assert-True ([string]$firstPublish.globalId -match '^NA\d{11}$') "Published Art did not receive a valid platform global ID."
    $globalId = [string]$firstPublish.globalId

    $afterPublish = Invoke-LoomJson -Method Get -Url "$daemonUrl/v1/tools" -Body $null
    $publishedTool = @($afterPublish.tools | Where-Object { [string]$_.id -eq $repositoryName }) | Select-Object -First 1
    Assert-Equal $globalId ([string]$publishedTool.metadata.art.globalId) "Daemon did not persist the platform global ID."

    $daemonCatalog = Invoke-LoomJson -Method Get -Url "$daemonUrl/v1/arts/store/catalog" -Body $null
    $daemonEntry = @($daemonCatalog.arts | Where-Object { [string]$_.id -eq $repositoryName }) | Select-Object -First 1
    Assert-Equal $globalId ([string]$daemonEntry.globalId) "Daemon catalog proxy dropped the platform global ID."

    $rotatedIdentity = Invoke-LoomJson -Method Post -Url "$daemonUrl/v1/publisher-identity/rotate" -Body @{}
    Assert-Equal ([string]$publisherIdentity.identity.userId) ([string]$rotatedIdentity.identity.userId) "Key rotation changed the platform user ID."
    Assert-True ([string]$publisherIdentity.identity.currentKeyId -ne [string]$rotatedIdentity.identity.currentKeyId) "Key rotation did not replace the current key."

    $secondTool = New-PublishFixtureTool -Version "0.2.0" -Endpoint "$daemonUrl/health"
    $null = Invoke-LoomJson -Method Post -Url "$daemonUrl/v1/arts/create" -Body @{ tool = $secondTool; files = @() }
    $secondPublish = Invoke-LoomJson -Method Post -Url "$daemonUrl/v1/arts/store/publish" -Body @{ artId = $repositoryName }
    Assert-Equal $globalId ([string]$secondPublish.globalId) "The same Art repository received a different global ID for a new version."

    $storeCatalog = Invoke-LoomJson -Method Get -Url "$storeUrl/catalog" -Body $null
    $storeEntry = @($storeCatalog.arts | Where-Object { [string]$_.id -eq $repositoryName }) | Select-Object -First 1
    Assert-Equal $globalId ([string]$storeEntry.globalId) "Art Store catalog global ID does not match the publish response."
    Assert-Equal "0.2.0" ([string]$storeEntry.latestVersion) "Art Store did not retain the latest published version."
    $versions = @($storeEntry.versions | ForEach-Object { [string]$_.version })
    Assert-True ($versions -contains "0.1.0" -and $versions -contains "0.2.0") "Art Store did not retain both published versions."

    $env:LOOM_DAEMON_PORT = [string]$installDaemonPort
    $env:LOOM_DAEMON_TOKEN = $script:DaemonAuthToken
    $env:LOOM_CONTROL_PLANE_ROOT = $installDaemonRoot
    $env:LOOM_CONFIGURATION_ROOT = $installConfigurationRoot
    $env:LOOM_RUN_STORE_PATH = Join-Path $installDaemonRoot "runs.sqlite3"
    $installDaemonProcess = Start-Process -FilePath $daemonPath -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput $installDaemonStdout -RedirectStandardError $installDaemonStderr
    Wait-HttpReady -Process $installDaemonProcess -Url "$installDaemonUrl/health" -Label "Loom install daemon"
    $installedFramework = Install-FrameworkZip -BaseUrl $installDaemonUrl -ZipPath $cloudFrameworkZip
    Assert-Equal "cloud_api" ([string]$installedFramework.framework.id) "Install daemon cloud framework returned the wrong ID."
    Assert-True ([bool]$installedFramework.framework.ready) "Install daemon cloud framework was not ready."
    $null = Invoke-LoomJson -Method Post -Url "$installDaemonUrl/v1/plugin-trust/users" -Body @{ userId = [string]$publisherIdentity.identity.userId }
    $frameworkStatus = Invoke-LoomJson -Method Get -Url "$installDaemonUrl/v1/frameworks" -Body $null
    $cloudFrameworkStatus = @($frameworkStatus.frameworks | Where-Object { [string]$_.id -eq "cloud_api" }) | Select-Object -First 1
    Assert-True ($null -ne $cloudFrameworkStatus -and [bool]$cloudFrameworkStatus.ready) "Cloud framework became unready before store installation."
    $null = Invoke-LoomJson -Method Post -Url "$installDaemonUrl/v1/arts/store/install" -Body @{ artId = $repositoryName; version = "0.1.0" }
    $null = Invoke-LoomJson -Method Post -Url "$installDaemonUrl/v1/arts/store/install" -Body @{ artId = $repositoryName; version = "0.2.0" }
    $installedTools = Invoke-LoomJson -Method Get -Url "$installDaemonUrl/v1/tools" -Body $null
    $installedTool = @($installedTools.tools | Where-Object { [string]$_.id -eq $repositoryName }) | Select-Object -First 1
    Assert-Equal $globalId ([string]$installedTool.metadata.art.globalId) "A clean store installation did not preserve the platform global ID."

    Write-Host "PASS platform assigned $globalId to $repositoryName"
    Write-Host "PASS default publisher ID $([string]$publisherIdentity.identity.userId) retained its retired key and verified both signed versions"
    Write-Host "PASS global ID remained stable across versions 0.1.0 and 0.2.0"
    Write-Host "PASS daemon registry and catalog preserved the platform global ID"
    Write-Host "PASS clean store installation preserved the platform global ID"
    $succeeded = $true
}
finally {
    foreach ($process in @($installDaemonProcess, $daemonProcess, $storeProcess)) {
        if ($null -ne $process -and -not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            $null = $process.WaitForExit(5000)
        }
    }
    if (-not $succeeded) {
        Write-Host "--- Art Store stdout ---"
        Get-Content -LiteralPath $storeStdout -ErrorAction SilentlyContinue
        Write-Host "--- Art Store stderr ---"
        Get-Content -LiteralPath $storeStderr -ErrorAction SilentlyContinue
        Write-Host "--- daemon stdout ---"
        Get-Content -LiteralPath $daemonStdout -ErrorAction SilentlyContinue
        Write-Host "--- daemon stderr ---"
        Get-Content -LiteralPath $daemonStderr -ErrorAction SilentlyContinue
        Write-Host "--- install daemon stdout ---"
        Get-Content -LiteralPath $installDaemonStdout -ErrorAction SilentlyContinue
        Write-Host "--- install daemon stderr ---"
        Get-Content -LiteralPath $installDaemonStderr -ErrorAction SilentlyContinue
    }
    foreach ($name in $oldEnvironment.Keys) {
        [System.Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name])
    }
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Loom Art Store global ID publish smoke passed."
