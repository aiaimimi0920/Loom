[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Assert-Contains {
    param(
        [string]$Needle,
        [string]$Haystack,
        [string]$Message
    )

    Assert-True $Haystack.Contains($Needle) "$Message Missing=[$Needle]"
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$smokePath = Join-Path $scriptRoot "Invoke-LoomRunPersistenceSmoke.ps1"
Assert-True (Test-Path -LiteralPath $smokePath -PathType Leaf) "Missing smoke script: $smokePath"

$raw = Get-Content -Raw -Encoding UTF8 -LiteralPath $smokePath
$tokens = $null
$parseErrors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile(
    $smokePath,
    [ref]$tokens,
    [ref]$parseErrors
)
Assert-Equal 0 @($parseErrors).Count "Smoke script must parse as PowerShell."

Assert-Contains '[string]$PackageDir' $raw "Smoke must accept a package directory."
Assert-Contains 'LOOM_CONTROL_PLANE_ROOT' $raw "Smoke must isolate the control-plane root."
Assert-Contains 'loom-runs.sqlite3' $raw "Smoke must assert the default database path."
Assert-Contains '/v1/runs/' $raw "Smoke must query persisted runs."
Assert-Contains 'LoomReleaseLayout.ps1' $raw "Smoke must use the shared release layout."
Assert-Contains 'Get-LoomReleaseLayout' $raw "Smoke must resolve the runtime daemon sidecar through the shared layout."
Assert-Contains '$desktopExe = $layout.desktopExe' $raw "Smoke must cover the single desktop entry."
Assert-Contains 'LOOM_SMOKE_ALLOW_MULTIPLE_INSTANCES = "1"' $raw "Smoke must isolate its desktop from a live user instance."
Assert-Contains 'ExecutablePath' $raw "Smoke cleanup must identify candidate processes by exact path."
Assert-Contains 'UTF8Encoding' $raw "Smoke evidence must be UTF-8 without BOM."
Assert-Contains 'ConvertTo-Json -InputObject' $raw "Smoke must serialize empty JSON arrays as arrays."
Assert-Contains 'candidateProcessesAfterCleanup' $raw "Smoke must record process cleanup."
Assert-Contains 'desktopAliveDuringAssertions' $raw "Smoke must prove the desktop remained alive."
Assert-Contains 'function Get-FailureMessage' $raw "Smoke must preserve cleanup failure diagnostics for all failure object types."

$runtimeTryIndex = $raw.LastIndexOf("try {")
$runtimeCatchIndex = $raw.IndexOf("catch {", $runtimeTryIndex)
$packagePreflightIndex = $raw.IndexOf('$packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)')
Assert-True ($runtimeTryIndex -ge 0 -and $runtimeCatchIndex -gt $runtimeTryIndex) "Smoke must have a main try/catch execution boundary."
Assert-True ($packagePreflightIndex -gt $runtimeTryIndex -and $packagePreflightIndex -lt $runtimeCatchIndex) "Package preflight must write a failed summary through the main finally block."

$processPath = Join-Path $scriptRoot "run-persistence-smoke\Process.ps1"
Assert-Contains '. (Join-Path $PSScriptRoot "run-persistence-smoke\Process.ps1")' $raw "Smoke must load its process identity definitions."
$null = [Management.Automation.Language.Parser]::ParseFile($processPath, [ref]$tokens, [ref]$parseErrors)
Assert-Equal 0 @($parseErrors).Count "Process definitions must parse."

# Run only the extracted process policy with deterministic mocks, never a real user process.
& {
    . $processPath
    $script:fixtureStoppedIds = @()
    $script:fixtureSnapshot = [pscustomobject]@{ ExecutablePath = "C:\fixture\other.exe" }
    function Get-ProcessSnapshotById { param([int]$ProcessId) return $script:fixtureSnapshot }
    function Stop-Process {
        param([int]$Id, [switch]$Force, [string]$ErrorAction)
        $script:fixtureStoppedIds += $Id
        $script:fixtureSnapshot = $null
    }
    Assert-Equal $true (Stop-ExactProcessById -ProcessId $null -ExpectedExecutablePath "C:\fixture\loom.exe") "Missing process id must be a no-op."
    Assert-Equal $false (Stop-ExactProcessById -ProcessId 42 -ExpectedExecutablePath "C:\fixture\loom.exe") "A reused id with another executable must be preserved."
    Assert-Equal 0 $script:fixtureStoppedIds.Count "Path mismatch must never stop a process."
    $script:fixtureSnapshot = [pscustomobject]@{ ExecutablePath = "C:\fixture\loom.exe" }
    Assert-Equal $true (Stop-ExactProcessById -ProcessId 42 -ExpectedExecutablePath "C:\fixture\loom.exe") "Owned process must stop after its snapshot disappears."
    Assert-Equal 1 $script:fixtureStoppedIds.Count "Owned process must be stopped once."
    Assert-Equal 42 $script:fixtureStoppedIds[0] "Cleanup must preserve process identity."
    function Get-CandidateProcessSnapshot {
        param([string[]]$ExecutablePaths)
        return @(
            [pscustomobject]@{ processId = 10; parentProcessId = 99 },
            [pscustomobject]@{ processId = 11; parentProcessId = 98 },
            [pscustomobject]@{ processId = 12; parentProcessId = 99 }
        )
    }
    $sibling = Wait-ForSiblingDaemon -DesktopPid 99 -DaemonExecutablePath "C:\fixture\loom.exe" -BaselinePidSet @{ "10" = $true }
    Assert-Equal 12 $sibling.processId "Sidecar selection must exclude baseline and unrelated parents."
}

Write-Host "Loom run persistence smoke contract passed."
