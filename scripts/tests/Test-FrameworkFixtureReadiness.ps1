[CmdletBinding()]
param(
    [string]$EvidenceRoot = $env:TEMP,
    [string]$ModuleRoot = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($ModuleRoot)) { $ModuleRoot = Join-Path $PSScriptRoot '..\framework-art-store-hook-smoke' }
foreach ($module in @('Assertions.ps1', 'Paths.ps1', 'Process.ps1', 'Http.ps1')) {
    . (Join-Path $ModuleRoot $module)
}
$root = Initialize-SmokeRealDirectory -Path (Join-Path $EvidenceRoot ('fixture-readiness-' + [Guid]::NewGuid().ToString('N'))) -Label 'readiness test'
$passed = 0
function Expect-Failure {
    param([scriptblock]$Action, [string]$Text)
    try { & $Action } catch {
        Assert-Contains $Text $_.Exception.Message 'Unexpected readiness error.'
        return $_.Exception.Message
    }
    throw 'Expected readiness failure.'
}
function Start-Fixture {
    param([string]$Name, [string]$Body)
    $file = Join-Path $root "$Name.ps1"
    Write-Utf8NoBomFile -Path $file -Content $Body
    Start-SmokeProcess -FilePath (Join-Path $PSHOME 'powershell.exe') `
        -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $file) `
        -WorkingDirectory $root -StdoutPath (Join-Path $root "$Name.out") -StderrPath (Join-Path $root "$Name.err")
}

$exited = $null
$alive = $null
$listener = $null
try {
    $exited = Start-Fixture 'exit' '[Console]::Error.WriteLine("fixture-boom Bearer abc.def secret-sentinel"); exit 7'
    Assert-True ($exited.WaitForExit(10000)) 'Exit fixture did not finish.'
    $watch = [Diagnostics.Stopwatch]::StartNew()
    # Legacy module support is only for reproducing the old timeout symptom.
    $ownership = @{}
    if ((Get-Command Wait-TcpPort).Parameters.ContainsKey('Process')) {
        $ownership = @{ Process = $exited; StderrPath = (Join-Path $root 'exit.err'); Secrets = @('secret-sentinel') }
    }
    $errorText = Expect-Failure { Wait-TcpPort -HostName '127.0.0.1' -Port 1 -Message 'fixture startup' -TimeoutSeconds 1 @ownership } 'Fixture exited with code 7'
    Assert-True ($watch.ElapsedMilliseconds -lt 1000) 'Dead fixture waited for TCP timeout.'
    Assert-Contains 'fixture-boom' $errorText 'Missing bounded stderr.'
    Assert-True (-not $errorText.Contains('abc.def') -and -not $errorText.Contains('secret-sentinel')) 'Secret leaked.'
    $passed++

    $large = Join-Path $root 'large.err'
    Write-Utf8NoBomFile $large (('x' * 12000) + "`nfinal-fixture-error`n")
    $errorText = Expect-Failure { Wait-TcpPort '127.0.0.1' 1 'bounded' -Process $exited -StderrPath $large } 'Fixture exited with code 7'
    Assert-Contains 'final-fixture-error' $errorText 'Missing log tail.'
    Assert-True ($errorText.Length -lt 4300) 'Unbounded diagnostic.'
    $passed++

    Write-Utf8NoBomFile $large ('secret-sentinel' * 1000)
    $errorText = Expect-Failure { Wait-TcpPort '127.0.0.1' 1 'cut-line' -Process $exited -StderrPath $large -Secrets @('secret-sentinel') } 'Fixture exited with code 7'
    Assert-Contains '<stderr line exceeds limit>' $errorText 'Partial long line was exposed.'
    $passed++

    $errorText = Expect-Failure { Wait-TcpPort '127.0.0.1' 1 'missing-log' -Process $exited -StderrPath (Join-Path $root 'absent.err') } 'Fixture exited with code 7'
    Assert-Contains 'Fixture stderr unavailable' $errorText 'Log failure replaced process error.'
    $passed++

    $alive = Start-Fixture 'alive' 'Start-Sleep -Seconds 30'
    $watch.Restart()
    $errorText = Expect-Failure { Wait-TcpPort '127.0.0.1' 1 'live-timeout' -TimeoutSeconds 1 -Process $alive } 'live-timeout'
    Assert-True ($watch.ElapsedMilliseconds -ge 900 -and $watch.ElapsedMilliseconds -lt 3000) 'Readiness deadline changed.'
    Assert-True (-not $alive.WaitForExit(0)) 'Waiter stopped a caller-owned fixture.'
    Assert-True ($errorText -notmatch 'Last error:\s*$') 'Timeout lost its error.'
    $passed++

    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    Wait-TcpPort '127.0.0.1' $port 'live-ready' -Process $alive -TimeoutSeconds 1
    $connection = $listener.AcceptTcpClient()
    $connection.Dispose()
    Wait-TcpPort '127.0.0.1' $port 'legacy-ready' -TimeoutSeconds 1
    $connection = $listener.AcceptTcpClient()
    $connection.Dispose()
    # An unrelated listener must never hide an already-dead owned fixture.
    [void](Expect-Failure { Wait-TcpPort '127.0.0.1' $port 'dead-ready' -Process $exited } 'Fixture exited with code 7')
    $passed += 3
} finally {
    if ($null -ne $listener) { $listener.Stop() }
    foreach ($process in @($alive, $exited)) {
        $failures = @(Stop-SpawnedProcess $process)
        Assert-Equal 0 $failures.Count 'Owned fixture cleanup failed.'
    }
}
Write-Output "Framework fixture readiness: $passed checks passed; evidence=$root"
