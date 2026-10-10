[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\NativeBridgeProbe.ps1')

function New-ProbeCleanupFixture {
    $process = [pscustomobject]@{
        StandardInput = [IO.StringWriter]::new()
        DenyKill = $true; Exited = $false; Disposed = $false; Kills = 0
    }
    $process | Add-Member ScriptMethod WaitForExit { param($BudgetMs) return $this.Exited }
    $process | Add-Member ScriptMethod Kill {
        $this.Kills++
        if ($this.DenyKill) { throw 'synthetic kill denial' }
        $this.Exited = $true
    }
    $process | Add-Member ScriptMethod Dispose { $this.Disposed = $true }
    return [pscustomobject]@{ Process = $process; State = 'Open'; Started = $true; Writer = $null }
}

$client = New-ProbeCleanupFixture
$caught = $false
try { Stop-LoomNativeBridgeProbe $client } catch { $caught = $true }
if (-not $caught -or $client.State -ne 'Closing' -or $client.Process.Disposed) {
    throw 'Failed termination must remain observable and retryable.'
}
$client.Process.DenyKill = $false
Stop-LoomNativeBridgeProbe $client
if ($client.State -ne 'Closed' -or -not $client.Process.Disposed -or $client.Process.Kills -ne 2) {
    throw 'Retry did not terminate and dispose the original owned process.'
}
Stop-LoomNativeBridgeProbe $client
if ($client.Process.Kills -ne 2) { throw 'Closed cleanup was not idempotent.' }
$client = New-ProbeCleanupFixture
$client.Started = $false
Stop-LoomNativeBridgeProbe $client
if ($client.State -ne 'Closed' -or -not $client.Process.Disposed -or $client.Process.Kills -ne 0) {
    throw 'Unstarted process cleanup attempted termination.'
}
Write-Output 'Native probe cleanup retry, ownership, startup failure and idempotence tests passed.'
