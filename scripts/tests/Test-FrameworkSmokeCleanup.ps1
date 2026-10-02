[CmdletBinding()]
param([string]$ProcessModulePath = "")

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($ProcessModulePath)) {
    $ProcessModulePath = Join-Path $PSScriptRoot "..\framework-art-store-hook-smoke\Process.ps1"
}
. $ProcessModulePath

function Assert-Cleanup([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function New-CleanupTestChild([hashtable]$State) {
    $child = [pscustomobject]@{ State = $State; Id = $State.ChildId; StartTime = $State.ChildStart }
    $child | Add-Member ScriptProperty Handle {
        if ($this.State.Mode -eq "exited-during-acquire") { $this.State.Exited = $true; throw "synthetic acquisition exit race" }
        if ($this.State.Mode -eq "handle-denied") { throw [UnauthorizedAccessException]::new("synthetic handle denial") }
        return [IntPtr]1
    }
    $child | Add-Member ScriptProperty HasExited { return $this.State.Exited -or $this.State.HasExitedHint }
    $child | Add-Member ScriptMethod Kill {
        $this.State.KillCalls++
        if ($this.State.Mode -eq "exit-during-kill") { $this.State.Exited = $true; throw "synthetic exit race" }
        if ($this.State.Mode -eq "kill-denied") { throw [UnauthorizedAccessException]::new("synthetic kill denial") }
    }
    $child | Add-Member ScriptMethod WaitForExit {
        param([int]$Milliseconds)
        [void]$this.State.Waits.Add($Milliseconds)
        if ($Milliseconds -gt 0 -and $this.State.KillCalls -gt 0 -and $this.State.Signal) {
            $this.State.Exited = $true
        }
        return $this.State.Exited
    }
    $child | Add-Member ScriptMethod Dispose { $this.State.Disposals++ }
    return $child
}

$originalFunctions = @{}
foreach ($name in @("Get-CimInstance", "Get-WmiObject", "Get-Process", "Stop-Process")) {
    $existing = Get-Item -LiteralPath "Function:\$name" -ErrorAction SilentlyContinue
    if ($null -ne $existing) { $originalFunctions[$name] = $existing.ScriptBlock }
}

function Get-CimInstance {
    [CmdletBinding()]
    param([string]$ClassName, [string]$Filter)
    if ($script:CleanupState.Mode -eq "enumeration-denied") { throw "synthetic enumeration denial" }
    if ($Filter -eq "ParentProcessId=$($script:CleanupState.ParentId)") {
        foreach ($node in @($script:CleanupState, $script:CleanupSibling)) {
            [pscustomobject]@{ ProcessId = $node.ChildId; ParentProcessId = $script:CleanupState.ParentId }
        }
        return
    }
    foreach ($node in @($script:CleanupState, $script:CleanupSibling)) {
        if ($Filter -ne "ProcessId=$($node.ChildId)") { continue }
        if ($node.Mode -in @("parent-query-denied", "query-denied-after-exit")) {
            if ($node.Mode -eq "query-denied-after-exit") { $node.Exited = $true }
            throw "synthetic parent query denial"
        }
        if ($node.Mode -eq "disappeared-after-pin") { $node.Exited = $true; return @() }
        $parentId = $script:CleanupState.ParentId
        if ($node.Mode -in @("wrong-parent", "wrong-parent-after-exit")) {
            $parentId++
            if ($node.Mode -eq "wrong-parent-after-exit") { $node.Exited = $true }
        }
        return [pscustomobject]@{ ProcessId = $node.ChildId; ParentProcessId = $parentId }
    }
    return @()
}
function Get-WmiObject {
    [CmdletBinding()]
    param([string]$Class, [string]$Filter)
    throw "synthetic WMI fallback denial"
}
function Get-Process {
    [CmdletBinding()]
    param([int]$Id)
    $nodes = @($script:CleanupState, $script:CleanupSibling | Where-Object { $_.ChildId -eq $Id })
    Assert-Cleanup ($nodes.Count -eq 1) "Test tried to discover an unrelated process."
    $node = $nodes[0]
    if ($node.Mode -eq "acquire-denied") { throw [UnauthorizedAccessException]::new("synthetic acquisition denial") }
    if ($node.Mode -eq "disappeared" -or $node.Exited) {
        $record = [Management.Automation.ErrorRecord]::new([ArgumentException]::new("synthetic disappearance"),
            "NoProcessFoundForGivenId", [Management.Automation.ErrorCategory]::ObjectNotFound, $Id)
        $PSCmdlet.WriteError($record)
        return
    }
    $node.Acquisitions++
    return $node.Child
}
function Stop-Process {
    [CmdletBinding()]
    param([int]$Id, [switch]$Force)
    # This seam also lets the frozen old implementation run without touching any foreign PID.
    if ($Id -eq $script:CleanupState.ParentId) { $script:CleanupState.Root.Kill(); return }
    $nodes = @($script:CleanupState, $script:CleanupSibling | Where-Object { $_.ChildId -eq $Id })
    Assert-Cleanup ($nodes.Count -eq 1) "Test tried to stop an unrelated process."
    $nodes[0].Child.Kill()
}

function Invoke-CleanupScenario([string]$Mode) {
    $executable = if ($PSVersionTable.PSEdition -eq "Desktop") { "powershell.exe" }
        elseif ($env:OS -eq "Windows_NT") { "pwsh.exe" } else { "pwsh" }
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $PSHOME $executable
    $start.Arguments = '-NoLogo -NoProfile -NonInteractive -Command "Start-Sleep -Seconds 30"'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $root = [Diagnostics.Process]::Start($start)
    $observer = $null
    try {
        $observer = [Diagnostics.Process]::GetProcessById($root.Id)
        $null = $observer.Handle
        $childStart = $root.StartTime.AddSeconds(1)
        if ($Mode -eq "older-child") { $childStart = $root.StartTime.AddSeconds(-1) }
        $script:CleanupState = @{ Mode = $Mode; ParentId = $root.Id; Root = $root; ChildId = 2147483646;
            ChildStart = $childStart; Exited = $false; HasExitedHint = $false; KillCalls = 0; Disposals = 0; Acquisitions = 0;
            Signal = ($Mode -notin @("timeout", "kill-denied")); Waits = [Collections.Generic.List[int]]::new() }
        $script:CleanupState.Child = New-CleanupTestChild $script:CleanupState
        $script:CleanupSibling = @{ Mode = "delayed-exit"; ChildId = 2147483645; ChildStart = $root.StartTime.AddSeconds(2);
            Exited = $false; HasExitedHint = $false; KillCalls = 0; Disposals = 0; Acquisitions = 0;
            Signal = $true; Waits = [Collections.Generic.List[int]]::new() }
        $script:CleanupSibling.Child = New-CleanupTestChild $script:CleanupSibling
        $errors = @(Stop-SpawnedProcess -Process $root -TimeoutMilliseconds 5000 -WarningAction SilentlyContinue)
        Assert-Cleanup ($observer.WaitForExit(0)) "The real synthetic root leaked in $Mode."
        $success = $Mode -in @("delayed-exit", "exit-during-kill", "disappeared", "exited-during-acquire", "disappeared-after-pin")
        Assert-Cleanup (($errors.Count -eq 0) -eq $success) "Cleanup result mismatch in ${Mode}: expected success=$success; errors=$($errors.Count)."
        if ($Mode -in @("wrong-parent", "older-child", "handle-denied", "parent-query-denied", "acquire-denied", "exited-during-acquire", "query-denied-after-exit", "wrong-parent-after-exit", "disappeared-after-pin")) {
            Assert-Cleanup ($script:CleanupState.KillCalls -eq 0) "An unverified descendant was killed in $Mode."
        }
        if ($Mode -eq "delayed-exit") {
            Assert-Cleanup ($script:CleanupState.KillCalls -eq 1) "Termination must be requested once per retained identity."
            Assert-Cleanup ($script:CleanupState.Waits.Count -gt 0) "Cleanup never observed the descendant exit signal."
        }
        $expectedDisposals = $script:CleanupState.Acquisitions
        Assert-Cleanup ($script:CleanupState.Disposals -eq $expectedDisposals) "Owned or rejected child handle disposal mismatch in $Mode."
        Assert-Cleanup (@($script:CleanupState.Waits | Where-Object { $_ -lt 0 -or $_ -gt 5000 }).Count -eq 0) "Exit wait exceeded its shared budget."
        Assert-Cleanup ($script:CleanupSibling.Disposals -eq $script:CleanupSibling.Acquisitions) "Sibling handle leaked after another process failed."
        if ($Mode -ne "enumeration-denied") {
            Assert-Cleanup ($script:CleanupSibling.Exited -and $script:CleanupSibling.KillCalls -eq 1) "A process failure prevented sibling cleanup."
        }
    } finally {
        # Keep a second identity-pinning handle so even an assertion failure cannot leak this fixture.
        if ($null -ne $observer) {
            if (-not $observer.WaitForExit(0)) { $observer.Kill(); $null = $observer.WaitForExit(5000) }
            $observer.Dispose()
        }
        $root.Dispose()
    }
}

try {
    foreach ($mode in @("delayed-exit", "timeout", "enumeration-denied", "handle-denied", "parent-query-denied",
        "kill-denied", "wrong-parent", "older-child", "disappeared", "exit-during-kill", "acquire-denied", "exited-during-acquire", "query-denied-after-exit", "wrong-parent-after-exit", "disappeared-after-pin")) {
        Invoke-CleanupScenario $mode
        Write-Output "Framework cleanup case passed: $mode"
    }
    $state = @{ Mode = "delayed-exit"; ChildId = 42; ChildStart = [DateTime]::UtcNow; Exited = $false;
        HasExitedHint = $false; KillCalls = 1; Disposals = 0; Signal = $true; Waits = [Collections.Generic.List[int]]::new() }
    $child = New-CleanupTestChild $state
    $clock = [pscustomobject]@{ ElapsedMilliseconds = 1000 }
    Assert-Cleanup (Wait-SmokeOwnedProcessExit $child $clock 5000) "First bounded exit wait failed."
    $state.Exited = $false
    $clock.ElapsedMilliseconds = 4000
    Assert-Cleanup (Wait-SmokeOwnedProcessExit $child $clock 5000) "Second bounded exit wait failed."
    $state.Exited = $false
    $clock.ElapsedMilliseconds = 6000
    Assert-Cleanup (-not (Wait-SmokeOwnedProcessExit $child $clock 5000)) "Expired budget falsely accepted a live handle."
    $state.HasExitedHint = $true
    Assert-Cleanup (-not (Wait-SmokeOwnedProcessExit $child $clock 5000)) "HasExited replaced the final handle-signal check."
    Assert-Cleanup (($state.Waits -join ",") -eq "4000,1000,0,0") "The deadline was reset per process or allowed a negative wait."
    Write-Output "Framework cleanup shared deadline and final exit-signal contracts passed."
} finally {
    foreach ($name in @("Get-CimInstance", "Get-WmiObject", "Get-Process", "Stop-Process")) {
        if ($originalFunctions.ContainsKey($name)) { Set-Item -LiteralPath "Function:\$name" -Value $originalFunctions[$name] }
        else { Remove-Item -LiteralPath "Function:\$name" -ErrorAction SilentlyContinue }
    }
}
