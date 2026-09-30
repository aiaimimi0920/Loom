param([string]$ArtDirectory = "")

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "stock-monitor-art/Helpers.ps1")
if ([string]::IsNullOrWhiteSpace($ArtDirectory)) {
    $ArtDirectory = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) "art-packages/samples/stock-monitor"
}

# Check the exact source bytes before Windows PowerShell can interpret them as ANSI.
foreach ($source in Get-ChildItem -LiteralPath (Join-Path $ArtDirectory "runtime") -Filter *.ps1 -File -Recurse) {
    $bytes = [IO.File]::ReadAllBytes($source.FullName)
    Assert-True ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) "Stock Monitor runtime source requires UTF-8 BOM for Windows PowerShell 5.1: $($source.Name)"
    $tokens = $null
    $parseErrors = $null
    $null = [Management.Automation.Language.Parser]::ParseFile($source.FullName, [ref]$tokens, [ref]$parseErrors)
    Assert-Equal 0 @($parseErrors).Count "Stock Monitor runtime source must parse: $($source.Name)"
}

$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("loom-stock-process-" + [Guid]::NewGuid().ToString("N"))
$runtime = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $ArtDirectory "art.runtime.json") | ConvertFrom-Json
$fixturePath = Join-Path $workRoot "fixture.ps1"
$manifestPath = Join-Path $workRoot "art.runtime.json"
$fixtureManifest = [ordered]@{
    entry = [ordered]@{ command = [string]$runtime.entry.command; args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "fixture.ps1") }
    limits = [ordered]@{ maxStdoutBytes = 4096; maxStderrBytes = 4096 }
}

function Set-ProcessFixture {
    param([string]$Source)
    [IO.File]::WriteAllText($fixturePath, $Source, [Text.UTF8Encoding]::new($true))
    [IO.File]::WriteAllText($manifestPath, ($fixtureManifest | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
}

function Assert-ProcessFailure {
    param([string]$Pattern, [object]$Request = @{}, [int]$TimeoutMilliseconds = 20000)
    $failure = ""
    try { $null = Invoke-StockRuntimeRequest $workRoot $Request -TimeoutMilliseconds $TimeoutMilliseconds }
    catch { $failure = $_.Exception.Message }
    Assert-True ($failure -match $Pattern) "Expected runtime failure [$Pattern], got [$failure]"
    Assert-True ($failure -match 'Invocation=\d+ RequestBytes=\d+ ChildExitCode=') "Runtime failure must identify the invocation and child exit state."
    Assert-True ($failure.Length -lt 4000) "Runtime diagnostics must be bounded."
    return $failure
}

New-Item -ItemType Directory -Path $workRoot | Out-Null
try {
    Set-ProcessFixture @'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$reader = [IO.StreamReader]::new([Console]::OpenStandardInput(), [Text.UTF8Encoding]::new($false, $true))
[Console]::Out.Write($reader.ReadToEnd())
'@
    $unicode = ([char]0x884C).ToString() + [char]0x60C5 + [char]::ConvertFromUtf32(0x1F600)
    $echo = Invoke-StockRuntimeRequest $workRoot @{ value = $unicode }
    Assert-Equal $unicode ([string]$echo.value) "The process transport must preserve non-ASCII UTF-8 request and response text."

    # An early exit must preserve child diagnostics instead of only an AggregateException.
    Set-ProcessFixture '[Console]::Error.Write("fixture-startup-failure token=fixture-secret"); exit 23'
    $failure = Assert-ProcessFailure 'ChildExitCode=23' @{ padding = "x" * (1024 * 1024) }
    Assert-True ($failure -match 'ChildExitCode=23') "Early-exit diagnostics must preserve the child exit code."
    Assert-True ($failure -notmatch 'fixture-secret') "Runtime diagnostics must redact credential-shaped values."

    Set-ProcessFixture 'function {'
    $null = Assert-ProcessFailure 'Category=PowerShellParserError' @{ padding = "x" * (1024 * 1024) }

    Set-ProcessFixture '[Console]::Error.Write("ParserError :" + ("9" * 1024) + " char:1"); exit 23'
    $failure = Assert-ProcessFailure 'Category=PowerShellParserError Line=unknown'
    Assert-True ($failure -notmatch '9{20}') "A forged parser line must not echo arbitrary numeric text."

    foreach ($secret in @('{"token":"fixture-json-secret"}', 'Authorization: Bearer fixture-bearer-secret', 'https://fixture-user:fixture-password@localhost/')) {
        Set-ProcessFixture ("[Console]::Error.Write('" + $secret + "'); exit 23")
        $failure = Assert-ProcessFailure 'ChildExitCode=23'
        Assert-True ($failure -notmatch 'fixture-json-secret|fixture-bearer-secret|fixture-user|fixture-password') "Child stderr must never disclose secret-bearing text."
    }
    Set-ProcessFixture '[Console]::Out.Write(''{"token":"fixture-stdout-secret", invalid}''); $null = [Console]::In.ReadToEnd()'
    $failure = Assert-ProcessFailure 'Reason=stdout-json'
    Assert-True ($failure -notmatch 'fixture-stdout-secret') "JSON parser exception messages must never echo child stdout."
    Set-ProcessFixture '$output = [Console]::OpenStandardOutput(); $output.WriteByte(255); $null = [Console]::In.ReadToEnd()'
    $null = Assert-ProcessFailure 'Reason=stdout-utf8'

    # Both streams can exceed a pipe buffer before the child starts reading stdin.
    $fixtureManifest.limits.maxStdoutBytes = 262144
    $fixtureManifest.limits.maxStderrBytes = 262144
    Set-ProcessFixture @'
[Console]::Error.Write(("e" * 131072))
[Console]::Out.Write('{"padding":"' + ("x" * 131072) + '"}')
$null = [Console]::In.ReadToEnd()
'@
    $large = Invoke-StockRuntimeRequest $workRoot @{ padding = "x" * (1024 * 1024) }
    Assert-Equal 131072 ([string]$large.padding).Length "Concurrent pipe draining must avoid a stdin/stdout/stderr deadlock."

    $fixtureManifest.limits.maxStdoutBytes = 4096
    $fixtureManifest.limits.maxStderrBytes = 4096
    foreach ($streamName in @("Out", "Error")) {
        Set-ProcessFixture "[Console]::$streamName.Write(('x' * 131072)); Start-Sleep -Seconds 30"
        $label = if ($streamName -eq "Out") { "stdout" } else { "stderr" }
        $null = Assert-ProcessFailure "Reason=$label-limit"
    }
    Set-ProcessFixture '$null = [Console]::In.ReadToEnd(); Start-Sleep -Seconds 30'
    $failure = Assert-ProcessFailure 'Reason=execution-timeout' -TimeoutMilliseconds 5000
    Assert-True ($failure -match 'ChildPid=(\d+)') "Timeout diagnostics must identify the child process."
    $childId = [int]$Matches[1]
    $remaining = Get-Process -Id $childId -ErrorAction SilentlyContinue
    Assert-True ($null -eq $remaining) "Timed-out runtime process must be terminated before returning."
    Set-ProcessFixture 'Start-Sleep -Seconds 30'
    $null = Assert-ProcessFailure 'Reason=stdin-timeout' @{ padding = "x" * (1024 * 1024) } -TimeoutMilliseconds 1000
    Set-ProcessFixture '$null = [Console]::In.ReadToEnd(); exit 0'
    $null = Assert-ProcessFailure 'Reason=stdout-empty'

    # Repeat success after failures to exercise pipe disposal and child cleanup.
    Set-ProcessFixture '$null = [Console]::In.ReadToEnd(); [Console]::Out.Write(''{"ok":true}'')'
    foreach ($iteration in 1..3) {
        $result = Invoke-StockRuntimeRequest $workRoot @{}
        Assert-True ([bool]$result.ok) "Runtime transport did not recover after child failures."
    }
    $interval = Invoke-StockRuntime -ArtDirectory $ArtDirectory -ActionId "stock_interval_commit" -Payload @{ value = 120; requestId = $unicode } -AuthoritativeState @{} -FrameworkData (New-McpData -Skipped)
    $state = $interval.output.surfaceAction.patches[0].statePatch
    Assert-Equal $unicode ([string]$state.lastRequestId) "The real runtime must round-trip a Unicode action request id."
    $expectedStatus = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("5q+PIDEyMCDnp5LliLfmlrA="))
    Assert-Equal $expectedStatus ([string]$state.statusText) "The real runtime must emit its localized source text as UTF-8."
    Write-Host "Stock Monitor process contract passed: source=UTF8-BOM transport=UTF8 diagnostics=bounded pipes=concurrent timeouts=bounded"
}
finally {
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
}
