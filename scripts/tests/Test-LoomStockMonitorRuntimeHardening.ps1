$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
. (Join-Path $scriptRoot "stock-monitor-art\Helpers.ps1")
$runtimeRoot = Join-Path $repoRoot "art-packages\samples\stock-monitor\runtime"
$script:StockMonitorRuntimeRoot = $runtimeRoot
foreach ($moduleName in @("Constants.ps1", "Protocol.ps1", "Domain.ps1", "Mcp.ps1", "Transforms.ps1", "Snapshot.ps1", "Output.ps1")) {
    . (Join-Path (Join-Path $runtimeRoot "lib") $moduleName)
}

Assert-True ($null -eq (Convert-NullableNumber @("1", "2"))) "Array values must not be coerced into provider numbers."
Assert-True ($null -eq (Convert-NullableNumber ([ordered]@{ value = 1 }))) "Object values must not be coerced into provider numbers."
Assert-True ($null -eq (Convert-NullableNumber $true)) "Boolean values must not be coerced into provider numbers."
Assert-True (-not (ConvertTo-StrictBoolean "false")) "The string false must not become a truthy provider boolean."
Assert-True (ConvertTo-StrictBoolean -Value $null -DefaultValue $true) "A missing provider boolean must use its explicit default."
Assert-Equal "fallback" (ConvertTo-BoundedText -Value "bad`ntext" -MaxLength 32 -DefaultValue "fallback") "Control-bearing provider text must be rejected."
Assert-Equal '"C:\Program Files\Loom\runtime.ps1"' (ConvertTo-ProcessArgument 'C:\Program Files\Loom\runtime.ps1') "Process argument quoting must preserve ordinary path separators."
Assert-JsonTextDepth -Value '{"value":"[[[[{{{{\"still text"}' -MaxDepth 1
$badLevels = @(ConvertTo-OrderBookLevels @([ordered]@{ level = $null; price = 1.0 }))
Assert-Equal 1 ([int]$badLevels[0].level) "An invalid provider order-book level must use its bounded ordinal."

$values = [object[]](1..2500)
$tail = @(Select-LastBoundedValue -Values $values -Limit 2000)
Assert-Equal 2000 $tail.Count "Bounded tail selection returned the wrong number of rows."
Assert-Equal 501 ([int]$tail[0]) "Bounded tail selection did not skip the unneeded array prefix."
Assert-Equal 2500 ([int]$tail[-1]) "Bounded tail selection lost the newest row."

$live = ConvertTo-LiveTape -Value ([ordered]@{
    now = 25.0
    isTrade = "false"
    observedAt = [DateTimeOffset]::UtcNow.ToString("o")
    source = "pysnowball"
}) -Code "SZ000034"
Assert-True (-not [bool]$live.isTrade) "String boolean coercion changed the trading-state decision."

$safeMessage = Get-SafeMcpErrorMessage "fixture history failure"
Assert-Equal "fixture history failure" $safeMessage "Safe provider diagnostics must remain actionable."
$secretMessage = Get-SafeMcpErrorMessage "token=not-for-output C:\Users\service\provider.log"
Assert-True ($secretMessage -notmatch "not-for-output|provider\.log|C:\\Users") "Credential-bearing provider diagnostics must be replaced."
$uncMessage = Get-SafeMcpErrorMessage "provider failed at \\server\share\provider.log"
Assert-True ($uncMessage -notmatch "server|share|provider\.log") "UNC paths in provider diagnostics must be replaced."
$falseErrorRequest = [ordered]@{ frameworkData = [ordered]@{ mcp = [ordered]@{ results = [ordered]@{
    quote = [ordered]@{ result = [ordered]@{ isError = "false"; structuredContent = [ordered]@{ ok = $true } } }
} } } }
Assert-True ([bool](Get-McpToolContent -Request $falseErrorRequest -CallId "quote").ok) "String false must not turn an MCP success into an error."
$emoji = [char]::ConvertFromUtf32(0x1F600)
$requestId = Get-ActionRequestId ([ordered]@{ payload = [ordered]@{ requestId = (("x" * 63) + $emoji + "tail") } })
Assert-Equal 63 $requestId.Length "Request id truncation must not split a UTF-16 surrogate pair."

$nodeLimitRejected = $false
try { Assert-RequestObjectGraph -Value ([object[]](1..$script:MaxRequestNodes)) }
catch { $nodeLimitRejected = $_.Exception.Message -match "too many JSON values" }
Assert-True $nodeLimitRejected "The request object-graph node limit must reject an oversized decoded graph."

function New-DeepAction {
    param([string]$Leaf)
    $payload = [ordered]@{ leaf = $Leaf }
    foreach ($index in 1..22) { $payload = [ordered]@{ next = $payload } }
    return [ordered]@{ actionId = "stock_refresh"; payload = $payload; authoritativeState = [ordered]@{} }
}

function New-BaseRuntimeRequest {
    return [ordered]@{
        protocolVersion = "loom.framework.v1"
        frameworkId = "mcp"
        artId = "custom-stock-monitor"
        inputs = [ordered]@{}
        frameworkData = New-McpData -Skipped
    }
}

$conflicting = [ordered]@{
    surfaceAction = New-DeepAction "root"
    params = [ordered]@{ surfaceAction = New-DeepAction "params" }
}
Assert-RequestObjectGraph -Value $conflicting
$conflictRejected = $false
try { $null = Resolve-SurfaceAction -Value $conflicting }
catch { $conflictRejected = $_.Exception.Message -eq "conflicting surfaceAction invocations were provided" }
Assert-True $conflictRejected "Surface actions that differ below the old depth-20 comparison must conflict."

$oversizedRequest = New-BaseRuntimeRequest
$oversizedRequest["params"] = [ordered]@{ padding = "x" * $script:MaxRequestBytes }
$oversized = Invoke-StockRuntimeRequest -ArtDirectory (Split-Path -Parent $runtimeRoot) -Request $oversizedRequest
Assert-Equal "error" ([string]$oversized.status) "Oversized Stock Monitor stdin must fail explicitly."
Assert-True ([string]$oversized.error.message -match "request exceeds $($script:MaxRequestBytes) bytes") "Oversized stdin returned the wrong bounded error."

$deepValue = [ordered]@{ leaf = $true }
foreach ($index in 1..35) { $deepValue = [ordered]@{ next = $deepValue } }
$deepRequest = New-BaseRuntimeRequest
$deepRequest["params"] = $deepValue
$deep = Invoke-StockRuntimeRequest -ArtDirectory (Split-Path -Parent $runtimeRoot) -Request $deepRequest
Assert-Equal "error" ([string]$deep.status) "Over-deep Stock Monitor JSON must fail explicitly."
Assert-True ([string]$deep.error.message -match "exceeds JSON depth $($script:MaxJsonDepth)") "Over-deep JSON returned the wrong bounded error."

Write-Host "Stock Monitor runtime hardening contract passed: stdin=4MiB depth=32 nodes=100000 scalar-types=strict history-tail=2000 secrets=redacted"
