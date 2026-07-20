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
$smokePath = Join-Path $scriptRoot "Invoke-LoomDaemonConcurrencySmoke.ps1"
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
Assert-Contains 'LOOM_DAEMON_WORKERS' $raw "Smoke must configure bounded workers."
Assert-Contains 'LOOM_DAEMON_QUEUE_CAPACITY' $raw "Smoke must configure queue capacity."
Assert-Contains 'requestExecutor' $raw "Smoke must inspect executor status."
Assert-Contains 'gatewayRequestEntered' $raw "Smoke must prove the Gateway call entered."
Assert-Contains 'probeCompletedBeforeGatewayRelease' $raw "Smoke must prove probe responsiveness."
Assert-Contains 'secondCapabilityCompletedBeforeGatewayRelease' $raw "Smoke must prove another capability can finish."
Assert-Contains 'candidateProcessesAfterCleanup' $raw "Smoke must prove cleanup."
Assert-Contains 'UTF8Encoding' $raw "Smoke evidence must be UTF-8 without BOM."
Assert-Contains 'ConvertTo-Json -InputObject' $raw "Smoke must serialize empty JSON arrays."
Assert-Contains 'ExecutablePath' $raw "Cleanup must use exact executable paths."

Write-Host "Loom daemon concurrency smoke contract passed."
