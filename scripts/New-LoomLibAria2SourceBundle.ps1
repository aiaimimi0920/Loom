[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BuildRoot,
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [Parameter(Mandatory = $true)][string]$OutputRoot
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $OutputRoot) { throw 'SOURCE_OUTPUT_EXISTS' }
$receiptPath = Join-Path $BuildRoot 'build-receipt.json'
$receipt = Get-Content -Raw -Encoding UTF8 -LiteralPath $receiptPath | ConvertFrom-Json
foreach ($field in @('sourceSha256', 'bridgeSha256', 'buildScriptSha256', 'linkMapSha256', 'librarySha256')) {
    if ($receipt.PSObject.Properties.Name -notcontains $field) { throw 'SOURCE_BUILD_RECEIPT_INCOMPLETE' }
}
$policyPath = Join-Path $repo 'security/libaria2-runtime.json'
$policy = Get-Content -Raw -Encoding UTF8 -LiteralPath $policyPath | ConvertFrom-Json
function Get-Digest([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
if ((Get-Digest $ArchivePath) -ne $policy.sourceSha256 -or $receipt.sourceSha256 -ne $policy.sourceSha256 -or
    (Get-Digest (Join-Path $BuildRoot 'bridge.cpp')) -ne $receipt.bridgeSha256 -or
    (Get-Digest (Join-Path $BuildRoot 'build.sh')) -ne $receipt.buildScriptSha256 -or
    (Get-Digest (Join-Path $BuildRoot 'link.map')) -ne $receipt.linkMapSha256 -or
    (Get-Digest (Join-Path $BuildRoot 'loom_aria2.dll')) -ne $receipt.librarySha256 -or
    $receipt.librarySha256 -ne $policy.librarySha256) { throw 'SOURCE_BUILD_BINDING_MISMATCH' }
$files = [ordered]@{
    'aria2-1.37.0.tar.xz' = $ArchivePath
    'bridge.cpp' = (Join-Path $BuildRoot 'bridge.cpp')
    'build.sh' = (Join-Path $BuildRoot 'build.sh')
    'link.map' = (Join-Path $BuildRoot 'link.map')
    'COPYING' = (Join-Path $BuildRoot 'COPYING')
    'compiler.txt' = (Join-Path $BuildRoot 'compiler.txt')
    'build-receipt.json' = $receiptPath
    'libaria2-runtime.json' = $policyPath
    'SOURCE_BUILD.md' = (Join-Path $repo 'native/libaria2/SOURCE_BUILD.md')
}
foreach ($path in $files.Values) {
    $item = Get-Item -LiteralPath $path
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'SOURCE_INPUT_NOT_REGULAR' }
}
New-Item -ItemType Directory -Path $OutputRoot | Out-Null
$records = @()
foreach ($name in $files.Keys) {
    $target = Join-Path $OutputRoot $name
    Copy-Item -LiteralPath $files[$name] -Destination $target
    $records += [ordered]@{ path = $name; sha256 = (Get-Digest $target) }
}
$manifest = [ordered]@{
    schemaVersion = 1
    scope = 'libaria2-engine-source-materials-only'
    combinedApplicationSourceComplete = $false
    releaseApproved = $false
    librarySha256 = $receipt.librarySha256
    files = $records
}
[IO.File]::WriteAllText((Join-Path $OutputRoot 'source-manifest.json'), ($manifest | ConvertTo-Json -Depth 6), [Text.UTF8Encoding]::new($false))
Write-Output "Engine source materials staged: $OutputRoot"
