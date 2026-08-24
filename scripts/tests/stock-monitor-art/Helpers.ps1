# Shared assertion, fixture, Surface assembly and child-process helpers.
$script:StockRuntimeInvocationSequence = 0

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

function Read-JavaScriptSurfaceSource {
    param(
        [Parameter(Mandatory = $true)][string]$PackageRoot,
        [Parameter(Mandatory = $true)][object]$Variant
    )

    $root = [System.IO.Path]::GetFullPath($PackageRoot).TrimEnd('\') + '\'
    $descriptorPath = Join-Path $PackageRoot (([string]$Variant.entry) + ".sources.json")
    $descriptor = Get-Content -Raw -Encoding UTF8 -LiteralPath $descriptorPath | ConvertFrom-Json
    if ([int]$descriptor.schemaVersion -ne 1) { throw "JavaScript Surface source descriptor schema mismatch." }
    $sourceFiles = @($descriptor.sourceFiles)
    if ($sourceFiles.Count -lt 1 -or $sourceFiles.Count -gt 32) {
        throw "JavaScript Surface source descriptor must contain 1 to 32 files."
    }
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($sourceFile in $sourceFiles) {
        $source = [string]$sourceFile
        if ([System.IO.Path]::IsPathRooted($source) -or [System.IO.Path]::GetExtension($source) -cne ".js") {
            throw "JavaScript Surface source file is invalid: $source"
        }
        if ($source -ceq [string]$Variant.entry -or -not $seen.Add($source)) {
            throw "JavaScript Surface source files must be unique and must not repeat entry."
        }
    }
    $entries = @($sourceFiles | ForEach-Object { [string]$_ }) + @([string]$Variant.entry)
    $sources = foreach ($entry in $entries) {
        $path = [System.IO.Path]::GetFullPath((Join-Path $PackageRoot $entry))
        if (-not ($path + '\').StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "JavaScript Surface source escaped its package: $entry"
        }
        Get-Content -Raw -Encoding UTF8 -LiteralPath $path
    }
    return "(() => {`n`"use strict`";`n" + ($sources -join "`n;`n") + "`n;`n`n})();`n"
}

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)
    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) { return $Argument }
    $escaped = [regex]::Replace($Argument, '(\\*)"', '$1$1\"')
    $escaped = [regex]::Replace($escaped, '(\\+)$', '$1$1')
    return '"' + $escaped + '"'
}

function Stop-StockRuntimeProcess {
    param([Diagnostics.Process]$Process)

    try {
        if (-not $Process.HasExited) { $Process.Kill() }
    }
    catch {
        if (-not $Process.HasExited) { throw }
    }
    try {
        $Process.WaitForExit()
    }
    catch {
        if (-not $Process.HasExited) { throw }
    }
}

function Complete-StockRuntimeReadTasks {
    param([AllowNull()][object[]]$Tasks)

    foreach ($task in @($Tasks)) {
        if ($null -eq $task) { continue }
        try {
            if ($task.Wait(5000)) { $null = $task.GetAwaiter().GetResult() }
        }
        catch {
            # Observing a failed drain prevents an unobserved task from masking
            # the original process failure during test teardown.
        }
    }
}

function New-McpData {
    param(
        [switch]$QuoteError,
        [switch]$HistoryError,
        [switch]$Skipped,
        [switch]$HistoryOnly,
        [switch]$QuoteOnly,
        [switch]$OrderBookOnly,
        [switch]$OrderBookError,
        [switch]$FavoritesOmitted,
        [switch]$FavoritesError,
        [switch]$FavoritesMalformed,
        [string]$Period = "day",
        [string]$Code = "SZ000034",
        [double]$QuotePrice = 24.99,
        [double]$LivePrice = 24.99,
        [switch]$LiveTrading,
        [string]$LiveObservedAt = "2026-08-14T07:00:00.000Z",
        [string]$LiveSource = "pysnowball"
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
                input = [ordered]@{ code = $Code; source = "eastmoney" }
                response = [ordered]@{
                    fetchedAt = [DateTimeOffset]::UtcNow.ToString("o")
                    stock = [ordered]@{
                        code = $Code
                        name = "Digital China"
                        percent = 0.004
                        now = $QuotePrice
                        low = 24.60
                        high = 25.20
                        yesterday = 24.89
                        source = "eastmoney"
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
                input = [ordered]@{ code = $Code; source = "eastmoney"; period = $Period; count = 2000; adjust = "none" }
                response = [ordered]@{
                    fetchedAt = [DateTimeOffset]::UtcNow.ToString("o")
                    count = 3
                    period = $Period
                    lastTradingDate = "2026-08-14"
                    klines = @(
                        [ordered]@{ date = "2026-08-12"; open = 24.50; close = 24.60; high = 24.80; low = 24.30; volume = 100000; source = "tencent" },
                        [ordered]@{ date = "2026-08-13"; open = 24.62; close = 24.75; high = 24.90; low = 24.55; volume = 120000; source = "tencent" },
                        [ordered]@{ date = "2026-08-14"; open = 24.80; close = 24.99; high = 25.20; low = 24.60; volume = 150000; source = "tencent" }
                    )
                }
            }
        }
    }
    $orderBookResult = if ($OrderBookError) {
        [ordered]@{
            isError = $true
            structuredContent = [ordered]@{
                response = [ordered]@{ code = "STOCK_API_TOOL_ERROR"; message = "fixture order book failure" }
            }
        }
    }
    else {
        [ordered]@{
            structuredContent = [ordered]@{
                input = [ordered]@{ code = $Code; source = "auto"; symbol = $Code }
                response = [ordered]@{
                    fetchedAt = [DateTimeOffset]::UtcNow.ToString("o")
                    orderBook = [ordered]@{
                        code = $Code
                        bids = @(
                            [ordered]@{ level = 1; price = 24.98; volume = 152340; orders = 88 },
                            [ordered]@{ level = 2; price = 24.97; volume = 61200; orders = 41 }
                        )
                        asks = @(
                            [ordered]@{ level = 1; price = 24.99; volume = 98700; orders = 55 },
                            [ordered]@{ level = 2; price = 25.00; volume = 44100; orders = 30 }
                        )
                        buyPercent = 49.24
                        sellPercent = 50.76
                        netVolume = -11455
                        ratio = 1.08
                        levels = 2
                        observedAt = $LiveObservedAt
                        source = $LiveSource
                    }
                    realtime = [ordered]@{
                        code = $Code
                        now = $LivePrice
                        open = 25.25
                        high = 25.60
                        low = 24.44
                        yesterday = 25.20
                        avgPrice = 24.91
                        volume = 18220000
                        amount = 459000000
                        turnoverRate = 7.31
                        amplitude = 4.6
                        marketCapital = 39800000000
                        isTrade = [bool]$LiveTrading
                        tradeSession = 0
                        observedAt = $LiveObservedAt
                        source = $LiveSource
                    }
                }
            }
        }
    }
    $favoritesResult = if ($FavoritesError) {
        [ordered]@{
            isError = $true
            structuredContent = [ordered]@{
                response = [ordered]@{ code = "STOCK_API_TOOL_ERROR"; message = "fixture favorites failure" }
            }
        }
    }
    else {
        [ordered]@{
            structuredContent = [ordered]@{
                input = [ordered]@{ codes = @("SZ000034", "SH600519", "HK00700", "USAAPL"); source = "eastmoney" }
                response = [ordered]@{
                    count = if ($FavoritesMalformed) { 1 } else { 4 }
                    stocks = if ($FavoritesMalformed) {
                        "not-a-stock-array"
                    }
                    else {
                        @(
                            [ordered]@{ code = "SZ000034"; name = "Digital China"; now = 24.99; yesterday = 24.89; percent = 0.004018; source = "eastmoney" },
                            [ordered]@{ code = "SH600519"; name = "Kweichow Moutai"; now = 1418.00; yesterday = 1400.00; percent = 0.012857; source = "eastmoney" },
                            [ordered]@{ code = "HK00700"; name = "Tencent"; now = 612.50; yesterday = 606.00; percent = 0.010726; source = "eastmoney" },
                            [ordered]@{ code = "USAAPL"; name = "Apple"; now = 231.10; yesterday = 229.40; percent = 0.007411; source = "eastmoney" }
                        )
                    }
                }
            }
        }
    }
    $results = if ($HistoryOnly) {
        [ordered]@{
            history = [ordered]@{ toolName = "get_market_series"; result = $historyResult }
        }
    }
    elseif ($QuoteOnly) {
        [ordered]@{
            quote = [ordered]@{ toolName = "get_stock"; result = $quoteResult }
            orderbook = [ordered]@{ toolName = "get_order_book"; result = $orderBookResult }
        }
    }
    elseif ($OrderBookOnly) {
        [ordered]@{
            orderbook = [ordered]@{ toolName = "get_order_book"; result = $orderBookResult }
        }
    }
    else {
        [ordered]@{
            quote = [ordered]@{ toolName = "get_stock"; result = $quoteResult }
            history = [ordered]@{ toolName = "get_market_series"; result = $historyResult }
            orderbook = [ordered]@{ toolName = "get_order_book"; result = $orderBookResult }
        }
    }
    if (-not $FavoritesOmitted -and -not $HistoryOnly -and -not $QuoteOnly -and -not $OrderBookOnly) {
        $results["favorites"] = [ordered]@{ toolName = "get_stocks"; result = $favoritesResult }
    }
    return [ordered]@{
        mcp = [ordered]@{
            serverId = "stock-api"
            results = $results
        }
    }
}

function Invoke-StockRuntimeRequest {
    param(
        [string]$ArtDirectory,
        [object]$Request
    )

    $invocation = ++$script:StockRuntimeInvocationSequence
    $requestJson = $Request | ConvertTo-Json -Depth 40 -Compress
    $requestBytes = [Text.Encoding]::UTF8.GetByteCount($requestJson + "`n")
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
    $started = $false
    $stdoutTask = $null
    $stderrTask = $null
    $stdinTask = $null
    try {
        Assert-True $process.Start() "Failed to start Stock Monitor runtime."
        $started = $true
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $stdinTask = $process.StandardInput.WriteLineAsync($requestJson)
        # Windows PowerShell cold starts can exceed five seconds on constrained
        # CI hosts; keep this bounded without conflating startup with action time.
        if (-not $stdinTask.Wait(20000)) {
            $childExited = $process.HasExited
            $childExitCode = if ($childExited) { [string]$process.ExitCode } else { "pending" }
            $stderrState = if ($stderrTask.IsCompleted) { "ready" } else { "pending" }
            Stop-StockRuntimeProcess -Process $process
            throw "Stock Monitor runtime stdin write timed out. Invocation=$invocation RequestBytes=$requestBytes ChildExited=$childExited ChildExitCode=$childExitCode StderrState=$stderrState"
        }
        $null = $stdinTask.GetAwaiter().GetResult()
        $process.StandardInput.Close()
        if (-not $process.WaitForExit(20000)) {
            Stop-StockRuntimeProcess -Process $process
            throw "Stock Monitor runtime timed out."
        }
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        $stderrBytes = [Text.Encoding]::UTF8.GetByteCount([string]$stderr)
        Assert-Equal 0 $process.ExitCode "Stock Monitor runtime exited with an error. StderrBytes=$stderrBytes"
        Assert-True (-not [string]::IsNullOrWhiteSpace($stdout)) "Stock Monitor runtime returned no stdout. StderrBytes=$stderrBytes"
        return $stdout.Trim() | ConvertFrom-Json
    }
    finally {
        if ($started) { Stop-StockRuntimeProcess -Process $process }
        Complete-StockRuntimeReadTasks -Tasks @($stdinTask, $stdoutTask, $stderrTask)
        $process.Dispose()
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
    return Invoke-StockRuntimeRequest -ArtDirectory $ArtDirectory -Request $request
}
