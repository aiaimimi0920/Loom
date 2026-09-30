[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [string]$EvidenceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) {
    $EvidenceRoot = Join-Path $scriptRoot "..\target\qr-projection-smoke"
}
$node = (Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$nodeVersion = & $node --version
if ($LASTEXITCODE -ne 0 -or $nodeVersion -notmatch '^v(\d+)\.(\d+)\.') {
    throw "Unable to determine Node.js version."
}
if ([int]$Matches[1] -lt 22 -or ([int]$Matches[1] -eq 22 -and [int]$Matches[2] -lt 18)) {
    throw "QR projection smoke requires Node.js 22.18 or newer."
}
& $node --experimental-strip-types --experimental-default-type=module `
    (Join-Path $scriptRoot "qr-projection-smoke\run.ts") `
    --package-dir ([IO.Path]::GetFullPath($PackageDir)) `
    --evidence-root ([IO.Path]::GetFullPath($EvidenceRoot))
if ($LASTEXITCODE -ne 0) { throw "QR projection candidate smoke failed." }
