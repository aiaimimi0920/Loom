[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OutputRoot,
    [string]$WslDistribution = '',
    [switch]$Resume
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)
if ((Test-Path -LiteralPath $OutputRoot) -and -not $Resume) { throw 'NATIVE_SOURCE_OUTPUT_EXISTS' }
$ancestor = $OutputRoot
while ($ancestor) {
    if ((Test-Path -LiteralPath $ancestor) -and ((Get-Item -LiteralPath $ancestor).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'NATIVE_SOURCE_LINK_REJECTED'
    }
    $ancestor = Split-Path -Parent $ancestor
}
$manifestPath = Join-Path $repo 'native/libaria2/toolchain-sources.json'
$manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.schemaVersion -ne 1) { throw 'NATIVE_SOURCE_SCHEMA_INVALID' }
foreach ($file in $manifest.files) {
    $uri = [Uri]$file.url
    if ($file.name -notmatch '^[A-Za-z0-9][A-Za-z0-9_.+-]*$' -or $file.sha256 -notmatch '^[0-9a-f]{64}$' -or
        $file.bytes -le 0 -or $file.bytes -gt 100MB -or $uri.Scheme -ne 'https' -or
        $uri.Host -notin @('archive.ubuntu.com', 'launchpad.net')) { throw 'NATIVE_SOURCE_RECORD_INVALID' }
}
New-Item -ItemType Directory -Path $OutputRoot -Force | Out-Null
$receiptPath = Join-Path $OutputRoot 'download-receipt.json'
if (Test-Path -LiteralPath $receiptPath) {
    $item = Get-Item -LiteralPath $receiptPath
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'NATIVE_SOURCE_RECEIPT_INVALID' }
}
function Get-SourceDigest([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $hasher = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($hasher.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $stream.Dispose(); $hasher.Dispose() }
}
$records = @()
foreach ($file in $manifest.files) {
    $destination = Join-Path $OutputRoot $file.name
    $complete = $false
    if (Test-Path -LiteralPath $destination) {
        $item = Get-Item -LiteralPath $destination
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -gt $file.bytes) {
            throw 'NATIVE_SOURCE_EXISTING_FILE_INVALID'
        }
        if ($item.Length -eq $file.bytes) {
            if ((Get-SourceDigest $destination) -ne $file.sha256) { throw 'NATIVE_SOURCE_DIGEST_MISMATCH' }
            $complete = $true
        }
    }
    if (-not $complete) {
        if ($WslDistribution) {
            $normalized = $destination.Replace('\', '/')
            if ($normalized -notmatch '^([A-Za-z]):/(.*)$') { throw 'NATIVE_SOURCE_WSL_LOCAL_DRIVE_REQUIRED' }
            $linuxPath = '/mnt/' + $Matches[1].ToLowerInvariant() + '/' + $Matches[2]
            & rtk proxy wsl.exe -d $WslDistribution -- curl --fail --location --silent --show-error --max-time 1200 --max-filesize $file.bytes --continue-at - --output $linuxPath $file.url
        } else {
            & rtk proxy curl.exe --fail --location --silent --show-error --max-time 1200 --max-filesize $file.bytes --continue-at - --output $destination $file.url
        }
        if ($LASTEXITCODE -ne 0) { throw "NATIVE_SOURCE_DOWNLOAD_FAILED: $($file.name)" }
    }
    $digest = Get-SourceDigest $destination
    if ((Get-Item -LiteralPath $destination).Length -ne $file.bytes -or $digest -ne $file.sha256) {
        throw "NATIVE_SOURCE_DIGEST_MISMATCH: $($file.name)"
    }
    $records += [ordered]@{ name = $file.name; sha256 = $digest; bytes = $file.bytes; url = $file.url }
}
$receipt = [ordered]@{ schemaVersion = 1; files = $records; releaseApproved = $false; pgpSignaturesVerified = $false }
[IO.File]::WriteAllText($receiptPath, ($receipt | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
Write-Output "Native source archives verified: $($records.Count)"
