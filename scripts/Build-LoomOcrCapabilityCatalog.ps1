[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageRoot,
    [Parameter(Mandatory = $true)][string]$BaseUrl,
    [string]$SigningKeyPath = $env:LOOM_PACKAGE_SIGNING_KEY_PATH,
    [string]$SigningPublisherId = $env:LOOM_PACKAGE_SIGNING_PUBLISHER_ID,
    [ValidateRange(1, 90)][int]$ExpiresInDays = 30,
    [switch]$AllowHttpLoopback,
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($SigningPublisherId -cne "neuro.official") {
    throw "The official OCR catalog must be signed by publisher neuro.official."
}

$builder = Join-Path $PSScriptRoot "Build-LoomCapabilityCatalog.ps1"
& $builder `
    -PackageRoot $PackageRoot `
    -BaseUrl $BaseUrl `
    -SigningKeyPath $SigningKeyPath `
    -SigningPublisherId $SigningPublisherId `
    -ExpiresInDays $ExpiresInDays `
    -AllowHttpLoopback:$AllowHttpLoopback.IsPresent `
    -Force:$Force.IsPresent
if ($LASTEXITCODE -ne 0) {
    throw "Generic capability catalog builder failed for the OCR package."
}
