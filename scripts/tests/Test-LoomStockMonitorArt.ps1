param(
    [string]$ArtifactRoot = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if (-not [object]::Equals($Expected, $Actual)) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)
    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) { return $Argument }
    return '"' + $Argument.Replace('\', '\\').Replace('"', '\"') + '"'
}

function Invoke-StockRuntime {
    param(
        [string]$ArtDirectory,
        [string]$ActionId,
        [hashtable]$Payload,
        [hashtable]$AuthoritativeState,
        [string]$ApiBaseUrl = ""
    )

    $runtime = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $ArtDirectory "art.runtime.json") | ConvertFrom-Json
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = [string]$runtime.entry.command
    $psi.Arguments = @($runtime.entry.args | ForEach-Object { ConvertTo-ProcessArgument ([string]$_) }) -join " "
    $psi.WorkingDirectory = $ArtDirectory
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.Environment["LOOM_STOCK_API_BASE_URL"] = $ApiBaseUrl
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    Assert-True $process.Start() "Failed to start Stock Monitor runtime."
    $request = [ordered]@{
        protocolVersion = "loom.framework.v1"
        frameworkId = "process"
        artId = "custom-stock-monitor"
        inputs = @{}
        params = @{}
        surfaceAction = [ordered]@{
            actionId = $ActionId
            payload = $Payload
            authoritativeState = $AuthoritativeState
        }
    }
    $process.StandardInput.WriteLine(($request | ConvertTo-Json -Depth 30 -Compress))
    $process.StandardInput.Close()
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    Assert-True $process.WaitForExit(20000) "Stock Monitor runtime timed out."
    Assert-Equal 0 $process.ExitCode "Stock Monitor runtime exited with an error: $stderr"
    Assert-True (-not [string]::IsNullOrWhiteSpace($stdout)) "Stock Monitor runtime returned no stdout: $stderr"
    return $stdout.Trim() | ConvertFrom-Json
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
. (Join-Path $repoRoot "scripts\LoomSmokePorts.ps1")
$fixtureScript = Join-Path $scriptRoot "fixtures\StockMonitorApiFixture.ps1"
$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("loom-stock-monitor-test-" + [Guid]::NewGuid().ToString("N"))
$readyPath = Join-Path $workRoot "fixture.ready"
$requestPath = Join-Path $workRoot "requests.log"
$fixtureStdout = Join-Path $workRoot "fixture.stdout.log"
$fixtureStderr = Join-Path $workRoot "fixture.stderr.log"
$artDirectory = Join-Path $repoRoot "art-packages\samples\stock-monitor"
$expandedArtDirectory = $null
$fixture = $null

New-Item -ItemType Directory -Force -Path $workRoot | Out-Null
try {
    if (-not [string]::IsNullOrWhiteSpace($ArtifactRoot)) {
        $artifactRootPath = if ([IO.Path]::IsPathRooted($ArtifactRoot)) {
            [IO.Path]::GetFullPath($ArtifactRoot)
        }
        else {
            [IO.Path]::GetFullPath((Join-Path $repoRoot $ArtifactRoot))
        }
        $zipPath = Join-Path $artifactRootPath "custom-stock-monitor.zip"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Packaged Stock Monitor ZIP is missing: $zipPath"
        $expandedArtDirectory = Join-Path $workRoot "packaged-art"
        Expand-Archive -LiteralPath $zipPath -DestinationPath $expandedArtDirectory -Force
        $artDirectory = $expandedArtDirectory
    }
    Assert-True (Test-Path -LiteralPath $fixtureScript -PathType Leaf) "Stock Monitor API fixture is missing."
    Assert-True (Test-Path -LiteralPath (Join-Path $artDirectory "surface\main.js") -PathType Leaf) "Stock Monitor JavaScript Surface is missing."

    $port = Get-LoomSmokePort
    $fixtureArguments = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $fixtureScript,
        "-Port", [string]$port,
        "-ReadyPath", $readyPath,
        "-RequestPath", $requestPath
    )
    $fixture = Start-Process -FilePath "powershell.exe" -ArgumentList (($fixtureArguments | ForEach-Object { ConvertTo-ProcessArgument $_ }) -join " ") -WindowStyle Hidden -PassThru -RedirectStandardOutput $fixtureStdout -RedirectStandardError $fixtureStderr
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $readyPath -PathType Leaf)) {
        if ($fixture.HasExited) {
            throw "Stock Monitor API fixture exited before readiness: $([IO.File]::ReadAllText($fixtureStderr))"
        }
        if ([DateTime]::UtcNow -ge $deadline) { throw "Timed out waiting for Stock Monitor API fixture." }
        Start-Sleep -Milliseconds 50
    }

    $initialState = @{ symbol = "SZ:000034"; market = "SZ"; intervalSeconds = 15 }
    $refresh = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_refresh" -Payload @{} -AuthoritativeState $initialState -ApiBaseUrl "http://127.0.0.1:$port"
    Assert-Equal "success" ([string]$refresh.status) "Stock Monitor refresh did not return a runtime success envelope."
    $surfaceAction = $refresh.output.surfaceAction
    Assert-Equal "loom.surface.v1" ([string]$surfaceAction.protocolVersion) "Stock Monitor Surface protocol mismatch."
    Assert-Equal 1 @($surfaceAction.patches).Count "Stock Monitor refresh must return one authoritative patch."
    Assert-Equal "ready" ([string]$surfaceAction.patches[0].statePatch.status) "Stock Monitor refresh state did not become ready."
    $quote = $surfaceAction.result.outputs.quote.value
    Assert-Equal "eastmoney" ([string]$quote.provider) "Stock Monitor formal quote provider mismatch."
    Assert-Equal "000034" ([string]$quote.symbol) "Stock Monitor symbol normalization failed."
    Assert-Equal "神州数码" ([string]$quote.name) "Stock Monitor quote name mismatch."
    Assert-True ([double]$quote.price -eq 24.99) "Stock Monitor quote price scaling failed."
    Assert-True ([double]$quote.changePercent -eq 0.4) "Stock Monitor quote percent scaling failed."
    Assert-Equal 3 @($quote.trend).Count "Stock Monitor trend parsing failed."
    Assert-True ($null -eq $surfaceAction.result.outputs.PSObject.Properties["trade"]) "Stock Monitor must not return a trading output."

    Assert-True $fixture.WaitForExit(10000) "Stock Monitor API fixture did not receive both requests."
    [void]$fixture.WaitForExit()
    $fixture.Refresh()
    $fixtureError = [IO.File]::ReadAllText($fixtureStderr)
    if ($null -ne $fixture.ExitCode) {
        Assert-Equal 0 $fixture.ExitCode "Stock Monitor API fixture failed: $fixtureError"
    }
    Assert-True ([string]::IsNullOrWhiteSpace($fixtureError)) "Stock Monitor API fixture wrote stderr: $fixtureError"
    $fixture.Dispose()
    $fixture = $null
    $requests = Get-Content -Raw -Encoding UTF8 -LiteralPath $requestPath
    Assert-True ($requests -match 'GET /api/qt/stock/get\?secid=0\.000034&fields=') "Stock Monitor quote request did not use the normalized Eastmoney secid."
    Assert-True ($requests -match 'GET /api/qt/stock/trends2/get\?secid=0\.000034&fields1=') "Stock Monitor trend request did not use the normalized Eastmoney secid."

    $interval = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_interval_commit" -Payload @{ value = 30 } -AuthoritativeState $initialState
    Assert-Equal 30 ([int]$interval.output.surfaceAction.patches[0].statePatch.intervalSeconds) "Stock Monitor interval commit failed."
    Assert-True ($null -eq $interval.output.surfaceAction.PSObject.Properties["result"]) "Interval changes must not fabricate a formal quote."

    $invalid = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_symbol_commit" -Payload @{ value = "INVALID" } -AuthoritativeState $initialState
    Assert-Equal "error" ([string]$invalid.output.surfaceAction.patches[0].statePatch.status) "Invalid Stock Monitor symbols must become an explicit error state."
    Assert-True ($null -eq $invalid.output.surfaceAction.PSObject.Properties["result"]) "Invalid symbols must not produce a formal quote."

    $unsafeOverride = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_refresh" -Payload @{} -AuthoritativeState $initialState -ApiBaseUrl "http://example.com"
    Assert-Equal "error" ([string]$unsafeOverride.output.surfaceAction.patches[0].statePatch.status) "Non-loopback Stock Monitor test overrides must be rejected."
    Assert-True ([string]$unsafeOverride.output.surfaceAction.patches[0].statePatch.error -match "回环") "Non-loopback override rejection message is missing."

    Write-Host "Stock Monitor Art contract passed: provider=eastmoney price=24.99 trend=3 no-trading=true"
}
finally {
    if ($null -ne $fixture) {
        if (-not $fixture.HasExited) { Stop-Process -Id $fixture.Id -Force -ErrorAction SilentlyContinue }
        $fixture.Dispose()
    }
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
}
