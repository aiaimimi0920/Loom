param(
    [string]$DaemonExecutable = ".\target\debug\loom-daemon.exe",
    [string]$FrameworkArtifactRoot = ".loom-art-store-data\frameworks",
    [string]$ArtArtifactRoot = ".loom-art-store-data\arts"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
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

    $json = if ($null -eq $Body) { $null } else { $Body | ConvertTo-Json -Depth 40 -Compress }
    if ($null -eq $json) {
        return Invoke-RestMethod -Method $Method -Uri $Url -TimeoutSec 30
    }
    return Invoke-RestMethod -Method $Method -Uri $Url -ContentType "application/json" -Body $json -TimeoutSec 120
}

function Install-Zip {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$ZipPath,
        [Parameter(Mandatory = $true)][string]$Prefix
    )

    $bytes = [System.IO.File]::ReadAllBytes($ZipPath)
    $encoded = "data:application/zip;base64,$([Convert]::ToBase64String($bytes))"
    return Invoke-LoomJson -Method Post -Url ($Url.TrimEnd('/') + $Prefix + "/install") -Body @{ zipBase64 = $encoded }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$daemonPath = if ([System.IO.Path]::IsPathRooted($DaemonExecutable)) {
    [System.IO.Path]::GetFullPath($DaemonExecutable)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $DaemonExecutable))
}
$frameworkRootPath = if ([System.IO.Path]::IsPathRooted($FrameworkArtifactRoot)) {
    [System.IO.Path]::GetFullPath($FrameworkArtifactRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $FrameworkArtifactRoot))
}
$artRootPath = if ([System.IO.Path]::IsPathRooted($ArtArtifactRoot)) {
    [System.IO.Path]::GetFullPath($ArtArtifactRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArtArtifactRoot))
}
Assert-True (Test-Path -LiteralPath $daemonPath -PathType Leaf) "Loom daemon executable not found: $daemonPath"

$controlPlane = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-sample-art-install-" + [guid]::NewGuid().ToString("N"))
$configuration = Join-Path $controlPlane "configuration"
$runStore = Join-Path $controlPlane "runs.sqlite3"
$stdoutPath = Join-Path $controlPlane "daemon.stdout.log"
$stderrPath = Join-Path $controlPlane "daemon.stderr.log"
$port = Get-FreePort
$baseUrl = "http://127.0.0.1:$port"
$daemon = $null
$succeeded = $false
$oldEnvironment = @{}
foreach ($name in @("LOOM_DAEMON_HOST", "LOOM_DAEMON_PORT", "LOOM_CONTROL_PLANE_ROOT", "LOOM_CONFIGURATION_ROOT", "LOOM_RUN_STORE_PATH")) {
    $oldEnvironment[$name] = [System.Environment]::GetEnvironmentVariable($name)
}

$image = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
$frameworkIds = @("cli_wrapper", "cloud_api", "script", "python_art", "mcp", "workflow")
$artCases = @(
    @{ id = "custom-1770146354922"; arguments = @{ inputs = @{ input = $image }; params = @{ quality_num = 90; lossless = $true } } },
    @{ id = "custom-remove-bg-cloud"; arguments = @{ inputs = @{ input = $image }; params = @{} } },
    @{ id = "custom-image-search"; arguments = @{ inputs = @{}; params = @{ query = "loom package smoke"; count = 3 } } },
    @{ id = "custom-1770131241684"; arguments = @{ inputs = @{ input = $image; reference = $image }; params = @{ strength = 50 } } },
    @{ id = "custom-image-blend-script"; arguments = @{ inputs = @{ input = $image; reference = $image }; params = @{ mix_ratio = 50 } } },
    @{ id = "custom-image-blend-compress-workflow"; arguments = @{ inputs = @{ input = $image; reference = $image }; params = @{ mix_ratio = 50; quality_num = 90 } } }
)

New-Item -ItemType Directory -Force -Path $controlPlane, $configuration | Out-Null
try {
    $env:LOOM_DAEMON_HOST = "127.0.0.1"
    $env:LOOM_DAEMON_PORT = [string]$port
    $env:LOOM_CONTROL_PLANE_ROOT = $controlPlane
    $env:LOOM_CONFIGURATION_ROOT = $configuration
    $env:LOOM_RUN_STORE_PATH = $runStore
    $daemon = Start-Process -FilePath $daemonPath -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath

    $ready = $false
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        Start-Sleep -Milliseconds 250
        try {
            $health = Invoke-WebRequest -Uri "$baseUrl/health" -UseBasicParsing -TimeoutSec 2
            if ([int]$health.StatusCode -eq 200) {
                $ready = $true
                break
            }
        }
        catch {
            if ($daemon.HasExited) {
                throw "Loom daemon exited before readiness. See captured daemon logs."
            }
        }
    }
    Assert-True $ready "Loom daemon did not become ready. See captured daemon logs."

    foreach ($frameworkId in $frameworkIds) {
        $zipPath = Join-Path $frameworkRootPath "$frameworkId.zip"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Framework ZIP missing: $zipPath"
        $null = Install-Zip -Url $baseUrl -ZipPath $zipPath -Prefix "/v1/frameworks"
    }
    $frameworkStatus = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/frameworks" -Body $null
    foreach ($frameworkId in $frameworkIds) {
        $status = @($frameworkStatus.frameworks | Where-Object { [string]$_.id -eq $frameworkId }) | Select-Object -First 1
        Assert-True ($null -ne $status -and [bool]$status.installed -and [bool]$status.enabled -and [bool]$status.ready) "Framework is not ready after package installation: $frameworkId"
    }

    foreach ($case in $artCases) {
        $zipPath = Join-Path $artRootPath "$($case.id).zip"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Art ZIP missing: $zipPath"
        $null = Install-Zip -Url $baseUrl -ZipPath $zipPath -Prefix "/v1/arts"
    }
    $tools = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools" -Body $null
    foreach ($case in $artCases) {
        Assert-True (@($tools.tools | Where-Object { [string]$_.id -eq $case.id }).Count -eq 1) "Installed Art is not listed: $($case.id)"
    }

    foreach ($case in $artCases) {
        $executed = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/tools/$($case.id)/execute" -Body @{ arguments = $case.arguments }
        Assert-True ([string]$executed.status -eq "succeeded") "Art execution failed: $($case.id) -> $($executed | ConvertTo-Json -Depth 20 -Compress)"
        $outputBase64 = [string]$executed.result.output_base64
        if ([string]::IsNullOrWhiteSpace($outputBase64)) {
            $outputBase64 = [string]$executed.result.output.output_base64
        }
        Assert-True ($outputBase64.StartsWith("data:image/", [System.StringComparison]::Ordinal)) "Art execution did not return an image data URL: $($case.id)"
        Write-Host "PASS installed/executed $($case.id)"
    }
    $succeeded = $true
}
finally {
    if ($null -ne $daemon -and -not $daemon.HasExited) {
        Stop-Process -Id $daemon.Id -Force -ErrorAction SilentlyContinue
        $null = $daemon.WaitForExit(5000)
    }
    if (-not $succeeded) {
        Write-Host "--- daemon stdout ---"
        Get-Content -LiteralPath $stdoutPath -ErrorAction SilentlyContinue
        Write-Host "--- daemon stderr ---"
        Get-Content -LiteralPath $stderrPath -ErrorAction SilentlyContinue
    }
    foreach ($name in $oldEnvironment.Keys) {
        [System.Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name])
    }
    Remove-Item -LiteralPath $controlPlane -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Loom sample Art install/execution smoke passed for $($artCases.Count) packages."
