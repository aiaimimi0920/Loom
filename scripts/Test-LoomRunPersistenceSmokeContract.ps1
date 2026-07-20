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
Assert-Contains 'loom-desktop.exe' $raw "Smoke must retain desktop auto-start coverage."
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

Write-Host "Loom run persistence smoke contract passed."
