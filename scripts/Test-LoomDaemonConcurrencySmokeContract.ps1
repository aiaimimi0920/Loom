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
$moduleRoot = Join-Path $scriptRoot "daemon-concurrency-smoke"
$moduleNames = @("Common.ps1", "Process.ps1", "Http.ps1", "GatewayFixture.ps1")
$modulePaths = @($moduleNames | ForEach-Object { Join-Path $moduleRoot $_ })
Assert-True (Test-Path -LiteralPath $moduleRoot -PathType Container) "Missing smoke module directory: $moduleRoot"
$actualModuleNames = @(Get-ChildItem -LiteralPath $moduleRoot -File -Filter "*.ps1" | Sort-Object Name | ForEach-Object Name)
Assert-Equal (@($moduleNames | Sort-Object) -join ",") ($actualModuleNames -join ",") "Smoke module set drifted."

$raw = Get-Content -Raw -Encoding UTF8 -LiteralPath $smokePath
$tokens = $null
$parseErrors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile(
    $smokePath,
    [ref]$tokens,
    [ref]$parseErrors
)
Assert-Equal 0 @($parseErrors).Count "Smoke script must parse as PowerShell."
$previousModuleIndex = -1
foreach ($moduleName in $moduleNames) {
    $moduleIndex = $raw.IndexOf($moduleName, [System.StringComparison]::Ordinal)
    Assert-True ($moduleIndex -gt $previousModuleIndex) "Smoke module load order drifted at $moduleName."
    $previousModuleIndex = $moduleIndex
}
foreach ($modulePath in $modulePaths) {
    Assert-True (Test-Path -LiteralPath $modulePath -PathType Leaf) "Missing smoke module: $modulePath"
    $moduleTokens = $null
    $moduleParseErrors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile(
        $modulePath,
        [ref]$moduleTokens,
        [ref]$moduleParseErrors
    )
    Assert-Equal 0 @($moduleParseErrors).Count "Smoke module must parse as PowerShell: $modulePath"
    $raw += [Environment]::NewLine + (Get-Content -Raw -Encoding UTF8 -LiteralPath $modulePath)
}

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
Assert-Contains 'startTimeUtcTicks' $raw "Cleanup must bind process identity to its creation time."
Assert-Contains 'ReceiveTimeout' $raw "Gateway fixture must bound stalled reads."
Assert-Contains 'maximumBodyBytes' $raw "Gateway fixture must bound request bodies."
Assert-Contains 'strictEncoding' $raw "Gateway fixture must reject invalid UTF-8."
Assert-Contains 'requires a Content-Length' $raw "Gateway fixture must require a byte-exact body length."
Assert-Contains 'requires Content-Type application/json' $raw "Gateway fixture must require JSON requests."
Assert-Contains 'Read-BoundedUtf8Text' $raw "Log evidence reads must be bounded."
Assert-Contains 'access[_-]?token' $raw "Log evidence must redact generic secret fields."
Assert-Contains 'LoomConcurrencySmokeEnvironmentMutation' $raw "Daemon environment mutation must be serialized."

& (Join-Path $scriptRoot "tests\Test-LoomDaemonConcurrencySmokeHelpers.ps1")

Write-Host "Loom daemon concurrency smoke contract passed."
