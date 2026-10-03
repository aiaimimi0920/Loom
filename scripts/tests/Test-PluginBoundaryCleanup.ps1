[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
. (Join-Path $repoRoot 'scripts\plugin-boundary-smoke\Diagnostics.ps1')
. (Join-Path $repoRoot 'scripts\plugin-boundary-smoke\Cleanup.ps1')
. (Join-Path $repoRoot 'scripts\plugin-boundary-smoke\WebSocket.ps1')
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $repoRoot 'scripts\Invoke-LoomPluginBoundarySmoke.ps1'), [ref]$tokens, [ref]$errors)
if (@($errors).Count) { throw 'Smoke script parse failed.' }
$mainTry = @($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.TryStatementAst] })
if ($mainTry.Count -ne 1) { throw 'Expected one top-level lifecycle try/catch/finally.' }
$stopDaemon = @($ast.EndBlock.Statements | Where-Object { $_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -eq 'Stop-TestDaemon' })[0]
. ([scriptblock]::Create($stopDaemon.Extent.Text))
# Execute the real main catch/finally verbatim, replacing only the expensive smoke body.
$harness = [scriptblock]::Create('try { if ($null -ne $injectedPrimary) { throw $injectedPrimary } } ' +
    $mainTry[0].CatchClauses[0].Extent.Text + ' finally ' + $mainTry[0].Finally.Extent.Text)
$root = Join-Path ([IO.Path]::GetTempPath()) ('loom-plugin-cleanup-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
function Assert-Cleanup([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
try {
    foreach ($scenario in @('primary-stop', 'primary-copy', 'primary-both', 'cleanup-only-stop', 'cleanup-only-copy', 'success')) {
        $caseRoot = Join-Path $root $scenario
        $evidencePath = Join-Path $caseRoot 'evidence'
        $controlPlane = Join-Path $caseRoot 'control'
        New-Item -ItemType Directory -Path $evidencePath, $controlPlane | Out-Null
        $stdoutPath = Join-Path $caseRoot 'source-stdout.log'
        $stderrPath = Join-Path $caseRoot 'source-stderr.log'
        [IO.File]::WriteAllText($stdoutPath, 'private-daemon-stdout')
        [IO.File]::WriteAllText($stderrPath, 'private-daemon-stderr')
        $copyFailure = $scenario -match 'copy|both'
        $stopFailure = $scenario -match 'stop|both'
        # Exclusive lock forces the real Copy-Item to fail, without changing ACLs.
        $locked = if ($copyFailure) { [IO.File]::Open($stdoutPath, 'Open', 'ReadWrite', 'None') } else { $null }
        # A real unassociated Process makes the real Stop-TestDaemon fail before any kill.
        $daemon = if ($stopFailure) { [Diagnostics.Process]::new() } else { $null }
        $hookBridgeClient = $null
        $hookBridgeRunning = $false
        $oldEnvironment = @{ LOOM_PLUGIN_CLEANUP_TEST = [Environment]::GetEnvironmentVariable('LOOM_PLUGIN_CLEANUP_TEST') }
        [Environment]::SetEnvironmentVariable('LOOM_PLUGIN_CLEANUP_TEST', 'changed')
        $primaryFailure = $null
        $injectedPrimary = if ($scenario.StartsWith('primary-')) { [OperationCanceledException]::new('private-original-websocket-failure') } else { $null }
        $caught = $null
        Set-LoomPluginBoundaryPhase -Phase 'execute-receive'
        try { & $harness } catch { $caught = $_ } finally {
            if ($null -ne $locked) { $locked.Dispose() }
            if ($null -ne $daemon) { $daemon.Dispose() }
        }
        if ($null -ne $injectedPrimary) {
            Assert-Cleanup ($null -ne $caught -and [object]::ReferenceEquals($caught.Exception, $injectedPrimary)) 'Main finally replaced the original WebSocket exception.'
        } elseif ($scenario -ne 'success') {
            Assert-Cleanup ($null -ne $caught) 'Cleanup-only failure was swallowed.'
        } else { Assert-Cleanup ($null -eq $caught) 'Successful cleanup threw.' }
        Assert-Cleanup (-not (Test-Path -LiteralPath $controlPlane)) 'Later control-plane cleanup was skipped.'
        Assert-Cleanup ([Environment]::GetEnvironmentVariable('LOOM_PLUGIN_CLEANUP_TEST') -eq $oldEnvironment.LOOM_PLUGIN_CLEANUP_TEST) 'Environment restoration was skipped.'
        Assert-Cleanup (Test-Path -LiteralPath (Join-Path $evidencePath 'daemon.stderr.log')) 'Later log copy was skipped.'
        if ($scenario -ne 'success') {
            $json = [IO.File]::ReadAllText((Join-Path $evidencePath 'plugin-boundary-diagnostic.json'))
            $record = $json | ConvertFrom-Json
            $expectedCount = if ($scenario -eq 'primary-both') { 2 } else { 1 }
            Assert-Cleanup ($record.cleanupFailureCount -eq $expectedCount) 'Cleanup failures were not separately counted.'
            Assert-Cleanup (@($record.firstCleanupFailure.exceptionTypes).Count -gt 0) 'Cleanup exception type missing.'
            Assert-Cleanup ([Text.Encoding]::UTF8.GetByteCount($json) -le 4096 -and -not $json.Contains('private-') -and -not $json.Contains($root)) 'Cleanup diagnostic leaked text or exceeded its cap.'
            if ($null -ne $injectedPrimary) {
                Assert-Cleanup (($record.exceptionTypes -join ',').Contains('OperationCanceledException')) 'Primary exception was replaced in the diagnostic.'
            }
        }
        Write-Output "PASS real main finally: $scenario"
    }
} finally {
    $full = [IO.Path]::GetFullPath($root)
    $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $full.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe cleanup test path.' }
    Remove-Item -LiteralPath $full -Recurse -Force
}
Write-Output 'Six real main catch/finally cleanup cases passed.'
