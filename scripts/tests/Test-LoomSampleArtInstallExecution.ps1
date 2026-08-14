param(
    [string]$DaemonExecutable = ".\target\debug\loom-daemon.exe",
    [string]$FrameworkArtifactRoot = ".loom-art-store-data\frameworks",
    [string]$McpServerArtifactRoot = ".loom-art-store-data\mcp-servers",
    [string]$ArtArtifactRoot = ".loom-art-store-data\arts",
    [string]$LargeImagePath = ""
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

function Install-McpZip {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$ZipPath
    )

    $bytes = [System.IO.File]::ReadAllBytes($ZipPath)
    return Invoke-LoomJson -Method Post -Url ($Url.TrimEnd('/') + "/v1/mcp/servers/install") -Body @{
        zipBase64 = [Convert]::ToBase64String($bytes)
    }
}

function Write-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function New-ImageSearchFixtureMcpPackage {
    param(
        [Parameter(Mandatory = $true)][string]$SourceZip,
        [Parameter(Mandatory = $true)][string]$DestinationZip,
        [Parameter(Mandatory = $true)][string]$WorkRoot,
        [Parameter(Mandatory = $true)][string]$Endpoint
    )

    $stage = Join-Path $WorkRoot "image-search-mcp-fixture"
    Expand-Archive -LiteralPath $SourceZip -DestinationPath $stage -Force
    $serverPath = Join-Path $stage "runtime\image-search-mcp.ps1"
    Assert-True (Test-Path -LiteralPath $serverPath -PathType Leaf) "Independent image-search MCP server is missing: $serverPath"

    $manifestPath = Join-Path $stage "mcp.server.json"
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
    Assert-True ([string]$manifest.entry.command -eq "runtime/image-search-mcp.ps1") "Image-search fixture must preserve the independent MCP entry."
    $manifest.entry.args = @("-Endpoint", $Endpoint)
    Write-Utf8NoBomFile -Path $manifestPath -Content (($manifest | ConvertTo-Json -Depth 40) + "`n")
    if (Test-Path -LiteralPath $DestinationZip) {
        Remove-Item -LiteralPath $DestinationZip -Force
    }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $DestinationZip -CompressionLevel Optimal -Force
}

function Start-ImageSearchApiFixture {
    param(
        [Parameter(Mandatory = $true)][int]$Port,
        [Parameter(Mandatory = $true)][string]$WorkRoot,
        [Parameter(Mandatory = $true)][string]$ReadyPath,
        [Parameter(Mandatory = $true)][string]$RequestPath
    )

    $fixturePath = Join-Path $WorkRoot "image-search-api-fixture.ps1"
    $fixtureSource = @'
param(
    [int]$Port,
    [string]$ReadyPath,
    [string]$RequestPath
)
$ErrorActionPreference = "Stop"
$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
$listener.Start()
try {
    [System.IO.File]::WriteAllText($ReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
    $captured = @()
    for ($requestIndex = 0; $requestIndex -lt 2; $requestIndex++) {
        $client = $listener.AcceptTcpClient()
        try {
            $stream = $client.GetStream()
            $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::ASCII, $false, 1024, $true)
            $lines = @()
            try {
                while ($null -ne ($line = $reader.ReadLine())) {
                    if ($line.Length -eq 0) { break }
                    $lines += $line
                }
            }
            finally {
                $reader.Dispose()
            }
            $captured += $lines
            $captured += ""
            $requestLine = [string]$lines[0]
            if ($requestLine -like "GET /res/v1/images/search?*") {
                $body = @{
                    results = @(@{
                        title = "Installed package fixture"
                        url = "http://127.0.0.1:$Port/fixture.png"
                        source = "https://example.test/source"
                        thumbnail = @{ src = "http://127.0.0.1:$Port/fixture.png" }
                        properties = @{ url = "http://127.0.0.1:$Port/fixture.png"; width = 1; height = 1 }
                    })
                } | ConvertTo-Json -Depth 10 -Compress
                $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($body)
                $contentType = "application/json; charset=utf-8"
                $status = "200 OK"
            }
            elseif ($requestLine -like "GET /fixture.png *") {
                $bodyBytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=")
                $contentType = "image/png"
                $status = "200 OK"
            }
            else {
                $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes("not found")
                $contentType = "text/plain"
                $status = "404 Not Found"
            }
            $header = "HTTP/1.1 $status`r`nContent-Type: $contentType`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
            $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
            $stream.Write($headerBytes, 0, $headerBytes.Length)
            $stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $stream.Flush()
        }
        finally {
            $client.Dispose()
        }
    }
    [System.IO.File]::WriteAllLines($RequestPath, $captured, [System.Text.UTF8Encoding]::new($false))
}
finally {
    $listener.Stop()
}
'@
    Write-Utf8NoBomFile -Path $fixturePath -Content $fixtureSource
    $processInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $processInfo.FileName = "powershell.exe"
    $processInfo.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$fixturePath`" -Port $Port -ReadyPath `"$ReadyPath`" -RequestPath `"$RequestPath`""
    $processInfo.UseShellExecute = $false
    $processInfo.CreateNoWindow = $true
    $processInfo.RedirectStandardOutput = $true
    $processInfo.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $processInfo
    Assert-True $process.Start() "Failed to start image-search API fixture."
    return $process
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
$mcpServerRootPath = if ([System.IO.Path]::IsPathRooted($McpServerArtifactRoot)) {
    [System.IO.Path]::GetFullPath($McpServerArtifactRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $McpServerArtifactRoot))
}
$artRootPath = if ([System.IO.Path]::IsPathRooted($ArtArtifactRoot)) {
    [System.IO.Path]::GetFullPath($ArtArtifactRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArtArtifactRoot))
}
Assert-True (Test-Path -LiteralPath $daemonPath -PathType Leaf) "Loom daemon executable not found: $daemonPath"
$largeImagePathResolved = if ([string]::IsNullOrWhiteSpace($LargeImagePath)) {
    ""
}
elseif ([System.IO.Path]::IsPathRooted($LargeImagePath)) {
    [System.IO.Path]::GetFullPath($LargeImagePath)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $LargeImagePath))
}
if (-not [string]::IsNullOrWhiteSpace($largeImagePathResolved)) {
    Assert-True (Test-Path -LiteralPath $largeImagePathResolved -PathType Leaf) "Large image fixture not found: $largeImagePathResolved"
}

$controlPlane = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-sample-art-install-" + [guid]::NewGuid().ToString("N"))
$configuration = Join-Path $controlPlane "configuration"
$runStore = Join-Path $controlPlane "runs.sqlite3"
$stdoutPath = Join-Path $controlPlane "daemon.stdout.log"
$stderrPath = Join-Path $controlPlane "daemon.stderr.log"
$port = Get-FreePort
do {
    $imageSearchApiPort = Get-FreePort
} while ($imageSearchApiPort -eq $port)
$baseUrl = "http://127.0.0.1:$port"
$daemon = $null
$imageSearchApiFixture = $null
$imageSearchApiReadyPath = Join-Path $controlPlane "image-search-api-ready"
$imageSearchApiRequestPath = Join-Path $controlPlane "image-search-api-requests.txt"
$succeeded = $false
$oldEnvironment = @{}
foreach ($name in @("LOOM_DAEMON_HOST", "LOOM_DAEMON_PORT", "LOOM_CONTROL_PLANE_ROOT", "LOOM_CONFIGURATION_ROOT", "LOOM_RUN_STORE_PATH")) {
    $oldEnvironment[$name] = [System.Environment]::GetEnvironmentVariable($name)
}

$image = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
$frameworkIds = @("process", "cloud_api", "mcp", "workflow")
$colorTransferParams = @{
    strength = 80
    gamma = 1.2
    exposure = -0.2
    contrast = 10
    highlights = -5
    shadows = 5
    whites = 3
    blacks = -3
    temperature = 10
    tint = -5
    saturation = 12
    vibrance = 8
    hue = 15
    split_h_hue = 35
    split_h_sat = 10
    split_s_hue = 215
    split_s_sat = 8
    split_balance = -10
    skin_protection = $true
}
$artCases = @(
    @{ id = "custom-1770146354922"; arguments = @{ inputs = @{ input = $image }; params = @{ quality_num = 90; lossless = $true } } },
    @{ id = "custom-remove-bg-cloud"; arguments = @{ inputs = @{ input = $image }; params = @{} } },
    @{ id = "custom-1770131241684"; arguments = @{ inputs = @{ input = $image; reference = $image }; params = $colorTransferParams } },
    @{ id = "custom-image-blend-script"; arguments = @{ inputs = @{ input = $image; reference = $image }; params = @{ mix_ratio = 50 } } },
    @{ id = "custom-image-blend-compress-workflow"; arguments = @{ inputs = @{ input = $image; reference = $image }; params = @{ mix_ratio = 50; quality_num = 90 } } },
    @{ id = "custom-image-search"; arguments = @{ inputs = @{}; params = @{ query = "loom package smoke"; count = 3 } } }
)

New-Item -ItemType Directory -Force -Path $controlPlane, $configuration | Out-Null
try {
    $sourceImageSearchMcpZip = Join-Path $mcpServerRootPath "neuro-image-search.zip"
    Assert-True (Test-Path -LiteralPath $sourceImageSearchMcpZip -PathType Leaf) "Image-search MCP ZIP missing: $sourceImageSearchMcpZip"
    $imageSearchMcpFixtureZip = Join-Path $controlPlane "neuro-image-search-fixture.zip"
    $imageSearchApiFixture = Start-ImageSearchApiFixture `
        -Port $imageSearchApiPort `
        -WorkRoot $controlPlane `
        -ReadyPath $imageSearchApiReadyPath `
        -RequestPath $imageSearchApiRequestPath
    $fixtureDeadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $imageSearchApiReadyPath -PathType Leaf)) {
        if ($imageSearchApiFixture.HasExited) {
            throw "Image-search API fixture exited early: $($imageSearchApiFixture.StandardError.ReadToEnd())"
        }
        if ([DateTime]::UtcNow -ge $fixtureDeadline) {
            throw "Timed out waiting for image-search API fixture."
        }
        Start-Sleep -Milliseconds 50
    }
    New-ImageSearchFixtureMcpPackage `
        -SourceZip $sourceImageSearchMcpZip `
        -DestinationZip $imageSearchMcpFixtureZip `
        -WorkRoot $controlPlane `
        -Endpoint "http://127.0.0.1:$imageSearchApiPort/res/v1/images/search"

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

    $installedMcp = Install-McpZip -Url $baseUrl -ZipPath $imageSearchMcpFixtureZip
    Assert-True ([string]$installedMcp.server.id -eq "neuro-image-search") "Independent image-search MCP package was not installed."
    $configuredMcp = Invoke-LoomJson `
        -Method Put `
        -Url "$baseUrl/v1/mcp/servers/neuro-image-search/credentials" `
        -Body @{ values = @{ brave_api_key = "loom-package-smoke-key" }; clear = @() }
    Assert-True ([bool]$configuredMcp.server.credentialBound) "Image-search MCP credential was not stored in the MCP scope."

    foreach ($case in $artCases) {
        $zipPath = Join-Path $artRootPath "$($case.id).zip"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Art ZIP missing: $zipPath"
        $null = Install-Zip -Url $baseUrl -ZipPath $zipPath -Prefix "/v1/arts"
    }
    $installedWorkflow = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/workflows/image-blend-compress-workflow" -Body $null
    Assert-True ([string]$installedWorkflow.workflow.id -eq "image-blend-compress-workflow") "Workflow sample definition was not registered from its package."
    $tools = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools" -Body $null
    foreach ($case in $artCases) {
        Assert-True (@($tools.tools | Where-Object { [string]$_.id -eq $case.id }).Count -eq 1) "Installed Art is not listed: $($case.id)"
    }

    $updatedTools = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools" -Body $null
    $updatedImageSearch = @($updatedTools.tools | Where-Object { [string]$_.id -eq "custom-image-search" }) | Select-Object -First 1
    Assert-True ([string]$updatedImageSearch.metadata.mcp.packageId -eq "neuro.official/neuro-image-search") "Image-search Art does not reference the independent MCP package."
    Assert-True ($null -eq $updatedImageSearch.metadata.PSObject.Properties["artUserSettings"]) "Image-search Art must not receive MCP credential bindings."

    $colorTransferManagement = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/arts/custom-1770131241684/management" -Body $null
    $colorTransferParameterIds = @($colorTransferManagement.parameters | ForEach-Object { [string]$_.id })
    Assert-True ($colorTransferParameterIds.Count -eq 19) "Color Transfer management does not expose all 19 RBF-era parameters."
    foreach ($key in $colorTransferParams.Keys) {
        Assert-True ($colorTransferParameterIds -contains $key) "Color Transfer management parameter is missing: $key"
    }
    Assert-True ([string](@($colorTransferManagement.parameters | Where-Object { [string]$_.id -eq "gamma" })[0].parameterType) -eq "number") "Color Transfer gamma parameter type was not preserved."
    Assert-True ([string](@($colorTransferManagement.parameters | Where-Object { [string]$_.id -eq "skin_protection" })[0].parameterType) -eq "boolean") "Color Transfer skin protection parameter type was not preserved."

    foreach ($case in $artCases) {
        $executed = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/tools/$($case.id)/execute" -Body @{ arguments = $case.arguments }
        Assert-True ([string]$executed.status -eq "succeeded") "Art execution failed: $($case.id) -> $($executed | ConvertTo-Json -Depth 20 -Compress)"
        $outputBase64 = ""
        $outputBase64Property = $executed.result.PSObject.Properties["output_base64"]
        if ($null -ne $outputBase64Property) {
            $outputBase64 = [string]$outputBase64Property.Value
        }
        $nestedOutputProperty = $executed.result.PSObject.Properties["output"]
        if ([string]::IsNullOrWhiteSpace($outputBase64) -and $null -ne $nestedOutputProperty) {
            $nestedOutputBase64Property = $nestedOutputProperty.Value.PSObject.Properties["output_base64"]
            if ($null -ne $nestedOutputBase64Property) {
                $outputBase64 = [string]$nestedOutputBase64Property.Value
            }
        }
        $contentProperty = $executed.result.PSObject.Properties["content"]
        if ([string]::IsNullOrWhiteSpace($outputBase64) -and $null -ne $contentProperty) {
            $outputBase64 = [string]@($contentProperty.Value)[0].data
        }
        if ([string]::IsNullOrWhiteSpace($outputBase64) -and $null -ne $nestedOutputProperty) {
            $nestedContentProperty = $nestedOutputProperty.Value.PSObject.Properties["content"]
            if ($null -ne $nestedContentProperty) {
                $outputBase64 = [string]@($nestedContentProperty.Value)[0].data
            }
        }
        Assert-True ($outputBase64.StartsWith("data:image/", [System.StringComparison]::Ordinal)) "Art execution did not return an image data URL: $($case.id)"
        if ($case.id -eq "custom-1770131241684") {
            Assert-True ([string]$executed.result.algorithm -eq "oklab-statistical-transfer") "Installed Color Transfer did not execute the restored algorithm."
            $appliedParameterIds = @($executed.result.applied_params.PSObject.Properties.Name)
            Assert-True ($appliedParameterIds.Count -eq 19) "Installed Color Transfer did not receive all 19 parameters."
            foreach ($key in $colorTransferParams.Keys) {
                Assert-True ($appliedParameterIds -contains $key) "Installed Color Transfer did not apply parameter: $key"
            }
        }
        Write-Host "PASS installed/executed $($case.id)"
    }
    Assert-True $imageSearchApiFixture.WaitForExit(10000) "Image-search API fixture did not observe both the search and image requests."
    Assert-True ($imageSearchApiFixture.ExitCode -eq 0) "Image-search API fixture failed: $($imageSearchApiFixture.StandardError.ReadToEnd())"
    $capturedImageSearchRequests = Get-Content -Raw -Encoding UTF8 -LiteralPath $imageSearchApiRequestPath
    Assert-True ($capturedImageSearchRequests -match 'GET /res/v1/images/search\?q=loom%20package%20smoke&count=3&safesearch=strict HTTP/1\.1') "Installed image-search MCP request URI is invalid."
    Assert-True ($capturedImageSearchRequests -match '(?im)^X-Subscription-Token:\s*loom-package-smoke-key\s*$') "Installed image-search MCP did not receive the MCP-scoped credential."
    Assert-True ($capturedImageSearchRequests -match '(?m)^GET /fixture\.png HTTP/1\.1') "Installed image-search Art did not download the selected image."
    $uninstallImageSearch = Invoke-LoomJson `
        -Method Post `
        -Url "$baseUrl/v1/arts/neuro.official%2Fcustom-image-search/uninstall" `
        -Body @{ removeUnusedMcpServers = $true }
    Assert-True ([bool]$uninstallImageSearch.uninstalled) "Image-search Art uninstall did not succeed."
    Assert-True (@($uninstallImageSearch.removedMcpServers) -contains "neuro-image-search") "Unused image-search MCP server was not removed with the Art."
    $mcpAfterArtUninstall = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/mcp/servers" -Body $null
    Assert-True (@($mcpAfterArtUninstall.servers).Count -eq 0) "Independent MCP server remained after optional unused dependency cleanup."
    if (-not [string]::IsNullOrWhiteSpace($largeImagePathResolved)) {
        $executed = Invoke-LoomJson `
            -Method Post `
            -Url "$baseUrl/v1/tools/custom-1770146354922/execute" `
            -Body @{
                arguments = @{
                    inputs = @{ input = $largeImagePathResolved }
                    params = @{ quality_num = 90; lossless = $true }
                }
            }
        Assert-True ([string]$executed.status -eq "succeeded") "Large-image compression failed: $($executed | ConvertTo-Json -Depth 20 -Compress)"
        $content = @($executed.result.content)
        Assert-True ($content.Count -eq 1 -and [string]$content[0].data -like "data:image/png;base64,*") "Large-image compression did not return a normalized image."
        $executionMetadata = $executed.result.PSObject.Properties["_loomExecution"].Value
        $diagnostics = $executionMetadata.PSObject.Properties["diagnostics"].Value
        Assert-True ([int64]$diagnostics.stdoutBytes -lt 65536) "Large-image compression leaked the image through framework stdout: $($diagnostics.stdoutBytes) bytes"
        Write-Host "PASS large-image compression: input=$((Get-Item -LiteralPath $largeImagePathResolved).Length) stdout=$($diagnostics.stdoutBytes) output=$(([string]$content[0].data).Length)"
    }
    $succeeded = $true
}
finally {
    if ($null -ne $daemon -and -not $daemon.HasExited) {
        Stop-Process -Id $daemon.Id -Force -ErrorAction SilentlyContinue
        $null = $daemon.WaitForExit(5000)
    }
    if ($null -ne $imageSearchApiFixture -and -not $imageSearchApiFixture.HasExited) {
        Stop-Process -Id $imageSearchApiFixture.Id -Force -ErrorAction SilentlyContinue
        $null = $imageSearchApiFixture.WaitForExit(5000)
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
    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $resolvedControlPlane = [System.IO.Path]::GetFullPath($controlPlane)
    if ($resolvedControlPlane.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedControlPlane).StartsWith("loom-sample-art-install-")) {
        Remove-Item -LiteralPath $resolvedControlPlane -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "Loom sample Art install/execution smoke passed for $($artCases.Count) packages."
