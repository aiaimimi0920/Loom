[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Invoke-InvalidConformance {
    param([string]$PublisherId, [string]$PackageId, [string]$EvidenceRoot)
    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $conformancePath `
            -PublisherId $PublisherId -PackageId $PackageId `
            -DaemonExecutable missing-daemon.exe -PluginCliExecutable missing-plugin.exe `
            -EvidenceRoot $EvidenceRoot 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    return [pscustomobject]@{ ExitCode = $exitCode; Output = @($output) }
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$conformancePath = Join-Path $repoRoot "scripts\Invoke-LoomCapabilityPluginConformance.ps1"
$evidenceRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-unsafe-id-" + [Guid]::NewGuid().ToString("N"))

try {
    $packageResult = Invoke-InvalidConformance -PublisherId publisher.example -PackageId "..\escape" -EvidenceRoot $evidenceRoot
    Assert-True ($packageResult.ExitCode -ne 0) "Unsafe package id must fail before running conformance."
    Assert-True (($packageResult.Output -join "`n").Contains("Unsafe capability package id")) "Unsafe package id failure was not explicit."
    Assert-True (-not (Test-Path -LiteralPath $evidenceRoot)) "Unsafe package id created conformance evidence."

    $publisherResult = Invoke-InvalidConformance -PublisherId "bad/publisher" -PackageId safe-package -EvidenceRoot $evidenceRoot
    Assert-True ($publisherResult.ExitCode -ne 0) "Unsafe publisher id must fail before running conformance."
    Assert-True (($publisherResult.Output -join "`n").Contains("Unsafe capability publisher id")) "Unsafe publisher id failure was not explicit."
    Assert-True (-not (Test-Path -LiteralPath $evidenceRoot)) "Unsafe publisher id created conformance evidence."
}
finally {
    if (Test-Path -LiteralPath $evidenceRoot) {
        Remove-Item -LiteralPath $evidenceRoot -Recurse -Force
    }
}

Write-Output "Capability plugin conformance input contract passed."
