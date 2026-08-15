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

function New-McpData {
    param(
        [switch]$QuoteError,
        [switch]$HistoryError,
        [switch]$Skipped
    )

    if ($Skipped) {
        return [ordered]@{ mcp = [ordered]@{ serverId = "stock-api"; skipped = $true } }
    }
    $quoteResult = if ($QuoteError) {
        [ordered]@{
            isError = $true
            structuredContent = [ordered]@{
                response = [ordered]@{ code = "STOCK_API_TOOL_ERROR"; message = "fixture quote failure" }
            }
        }
    }
    else {
        [ordered]@{
            structuredContent = [ordered]@{
                input = [ordered]@{ code = "SZ000034"; source = "auto" }
                response = [ordered]@{
                    stock = [ordered]@{
                        code = "SZ000034"
                        name = "Digital China"
                        percent = 0.004
                        now = 24.99
                        low = 24.60
                        high = 25.20
                        yesterday = 24.89
                        source = "tencent"
                    }
                }
            }
        }
    }
    $historyResult = if ($HistoryError) {
        [ordered]@{
            isError = $true
            structuredContent = [ordered]@{
                response = [ordered]@{ code = "STOCK_API_TOOL_ERROR"; message = "fixture history failure" }
            }
        }
    }
    else {
        [ordered]@{
            structuredContent = [ordered]@{
                input = [ordered]@{ code = "SZ000034"; source = "auto"; period = "day"; count = 60; adjust = "none" }
                response = [ordered]@{
                    count = 3
                    klines = @(
                        [ordered]@{ date = "2026-08-12"; open = 24.50; close = 24.60; high = 24.80; low = 24.30; volume = 100000; source = "tencent" },
                        [ordered]@{ date = "2026-08-13"; open = 24.62; close = 24.75; high = 24.90; low = 24.55; volume = 120000; source = "tencent" },
                        [ordered]@{ date = "2026-08-14"; open = 24.80; close = 24.99; high = 25.20; low = 24.60; volume = 150000; source = "tencent" }
                    )
                }
            }
        }
    }
    return [ordered]@{
        mcp = [ordered]@{
            serverId = "stock-api"
            results = [ordered]@{
                quote = [ordered]@{ toolName = "get_stock"; result = $quoteResult }
                history = [ordered]@{ toolName = "get_klines"; result = $historyResult }
            }
        }
    }
}

function Invoke-StockRuntime {
    param(
        [string]$ArtDirectory,
        [AllowEmptyString()][string]$ActionId,
        [AllowNull()][object]$Payload,
        [AllowNull()][object]$AuthoritativeState,
        [AllowNull()][object]$FrameworkData,
        [AllowNull()][object]$Params = @{}
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
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    Assert-True $process.Start() "Failed to start Stock Monitor runtime."
    $request = [ordered]@{
        protocolVersion = "loom.framework.v1"
        frameworkId = "mcp"
        artId = "custom-stock-monitor"
        inputs = @{}
        params = $Params
        frameworkData = $FrameworkData
    }
    if (-not [string]::IsNullOrWhiteSpace($ActionId)) {
        $request.surfaceAction = [ordered]@{
            actionId = $ActionId
            payload = if ($null -eq $Payload) { @{} } else { $Payload }
            authoritativeState = if ($null -eq $AuthoritativeState) { @{} } else { $AuthoritativeState }
        }
    }
    $process.StandardInput.WriteLine(($request | ConvertTo-Json -Depth 40 -Compress))
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
$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("loom-stock-monitor-test-" + [Guid]::NewGuid().ToString("N"))
$artDirectory = Join-Path $repoRoot "art-packages\samples\stock-monitor"

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
        $artDirectory = Join-Path $workRoot "packaged-art"
        Expand-Archive -LiteralPath $zipPath -DestinationPath $artDirectory -Force
    }

    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $artDirectory "manifest.json") | ConvertFrom-Json
    Assert-Equal "mcp" ([string]$manifest.execution.framework) "Stock Monitor must execute through the MCP framework."
    Assert-Equal "=2.7.3" ([string]$manifest.metadata.mcp.version) "Stock Monitor stock-api version must be exact."
    Assert-Equal 2 @($manifest.metadata.mcp.calls).Count "Stock Monitor must declare quote and history MCP calls."
    Assert-Equal 0 @($manifest.metadata.mcp.surfaceActions.stock_interval_commit.calls).Count "Interval updates must skip remote MCP calls."
    Assert-Equal "neuro.official/stock-api" ([string]$manifest.metadata.dependencies.mcpServers[0].id) "Stock Monitor MCP dependency mismatch."

    $surfacePath = Join-Path $artDirectory "surface\main.js"
    $runtimePath = Join-Path $artDirectory "runtime\main.ps1"
    Assert-True (Test-Path -LiteralPath $surfacePath -PathType Leaf) "Stock Monitor JavaScript Surface is missing."
    $surfaceSource = Get-Content -Raw -Encoding UTF8 -LiteralPath $surfacePath
    $runtimeSource = Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimePath
    Assert-True ($surfaceSource.Contains("MAX_CANVAS_PIXELS") -and $surfaceSource.Contains("movingAverage")) "Stock Monitor Surface must cap Canvas allocation and draw MA5 candles."
    Assert-True ($runtimeSource -match 'frameworkData' -and $runtimeSource -match 'results') "Stock Monitor runtime must consume MCP framework results."
    Assert-True ($runtimeSource -notmatch 'Invoke-RestMethod|push2\.eastmoney\.com|push2his\.eastmoney\.com') "Stock Monitor runtime must not bypass the stock-api MCP server."

    $initialState = @{ code = "SZ000034"; market = "SZ"; intervalSeconds = 60 }
    $refresh = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $initialState -FrameworkData (New-McpData)
    Assert-Equal "success" ([string]$refresh.status) "Stock Monitor refresh did not return a runtime success envelope."
    $surfaceAction = $refresh.output.surfaceAction
    Assert-Equal "loom.surface.v1" ([string]$surfaceAction.protocolVersion) "Stock Monitor Surface protocol mismatch."
    Assert-Equal 1 @($surfaceAction.patches).Count "Stock Monitor refresh must return one authoritative patch."
    Assert-Equal 2 ([int]$surfaceAction.patches[0].statePatch.schemaVersion) "Stock Monitor state schema did not migrate."
    Assert-Equal "ready" ([string]$surfaceAction.patches[0].statePatch.status) "Stock Monitor refresh state did not become ready."
    $quote = $surfaceAction.result.outputs.quote.value
    Assert-Equal "stock-api" ([string]$quote.provider) "Stock Monitor formal quote provider mismatch."
    Assert-Equal "2.7.3" ([string]$quote.providerVersion) "Stock Monitor formal quote provider version mismatch."
    Assert-Equal "tencent" ([string]$quote.source) "Stock Monitor selected provider source mismatch."
    Assert-Equal "SZ000034" ([string]$quote.code) "Stock Monitor code normalization failed."
    Assert-Equal "Digital China" ([string]$quote.name) "Stock Monitor quote name mismatch."
    Assert-True ([double]$quote.price -eq 24.99) "Stock Monitor quote price parsing failed."
    Assert-True ([double]$quote.changePercent -eq 0.4) "Stock Monitor quote percent conversion failed."
    Assert-Equal 3 @($quote.history.rows).Count "Stock Monitor K-line parsing failed."
    Assert-Equal "day" ([string]$quote.history.period) "Stock Monitor K-line period mismatch."
    Assert-True ($null -eq $surfaceAction.result.outputs.PSObject.Properties["trade"]) "Stock Monitor must not return a trading output."

    $interval = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_interval_commit" -Payload @{ value = 120 } -AuthoritativeState $initialState -FrameworkData (New-McpData -Skipped)
    Assert-Equal 120 ([int]$interval.output.surfaceAction.patches[0].statePatch.intervalSeconds) "Stock Monitor interval commit failed."
    Assert-True ($null -eq $interval.output.surfaceAction.PSObject.Properties["result"]) "Interval changes must not fabricate a formal quote."

    $invalid = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_symbol_commit" -Payload @{ value = "INVALID" } -AuthoritativeState $initialState -FrameworkData (New-McpData -QuoteError)
    Assert-Equal "error" ([string]$invalid.output.surfaceAction.patches[0].statePatch.status) "Invalid Stock Monitor symbols must become an explicit error state."
    Assert-True ([string]$invalid.output.surfaceAction.patches[0].statePatch.error -match "fixture quote failure") "MCP error detail was not surfaced."
    Assert-True ($null -eq $invalid.output.surfaceAction.PSObject.Properties["result"]) "Invalid symbols must not produce a formal quote."

    $plain = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "" -Payload $null -AuthoritativeState $null -FrameworkData (New-McpData) -Params @{ code = "SZ000034"; interval_seconds = 60 }
    Assert-Equal "success" ([string]$plain.status) "Non-Surface Stock Monitor execution failed."
    Assert-Equal "SZ000034" ([string]$plain.output.quote.code) "Non-Surface Stock Monitor output mismatch."
    Assert-Equal 3 @($plain.output.quote.history.rows).Count "Non-Surface Stock Monitor history mismatch."

    Write-Host "Stock Monitor Art contract passed: provider=stock-api@2.7.3 source=tencent candles=3 no-trading=true"
}
finally {
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
}
