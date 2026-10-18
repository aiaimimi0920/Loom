[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BuildRoot,
    [Parameter(Mandatory = $true)][string]$Destination,
    [switch]$DevelopmentOnly
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$policyPath = Join-Path $repo 'security/libaria2-runtime.json'
$policy = Get-Content -Raw -Encoding UTF8 -LiteralPath $policyPath | ConvertFrom-Json
if (-not $DevelopmentOnly -and -not $policy.releaseApproved) { throw 'LIBARIA2_RELEASE_NOT_APPROVED' }
$BuildRoot = [IO.Path]::GetFullPath($BuildRoot)
$Destination = [IO.Path]::GetFullPath($Destination)
$receipt = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $BuildRoot 'build-receipt.json') | ConvertFrom-Json
$dll = Join-Path $BuildRoot 'loom_aria2.dll'
$actualHash = (Get-FileHash -LiteralPath $dll).Hash.ToLowerInvariant()
if ($actualHash -ne $policy.librarySha256 -or $actualHash -ne $receipt.librarySha256 -or
    $receipt.sourceSha256 -ne $policy.sourceSha256 -or
    $receipt.bridgeSha256 -ne (Get-FileHash -LiteralPath (Join-Path $repo 'native/libaria2/bridge.cpp')).Hash.ToLowerInvariant()) {
    throw 'LIBARIA2_BUILD_BINDING_MISMATCH'
}
$ready = Join-Path 'C:/Users/Public/nas_home/AI/GameEditor/linshi' ('loom-libaria2-ready-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $ready | Out-Null
foreach ($name in @('loom_aria2.dll', 'COPYING', 'build-receipt.json')) {
    Copy-Item -LiteralPath (Join-Path $BuildRoot $name) -Destination (Join-Path $ready $name)
}
Copy-Item -LiteralPath $policyPath -Destination (Join-Path $ready 'libaria2-runtime.json')
if (Test-Path -LiteralPath $Destination) {
    foreach ($file in Get-ChildItem -LiteralPath $ready -File) {
        $target = Join-Path $Destination $file.Name
        if (-not (Test-Path -LiteralPath $target -PathType Leaf) -or
            (Get-Item -LiteralPath $target).Attributes -band [IO.FileAttributes]::ReparsePoint -or
            (Get-FileHash -LiteralPath $target).Hash -ne (Get-FileHash -LiteralPath $file.FullName).Hash) {
            throw 'LIBARIA2_EXISTING_RUNTIME_CHANGED'
        }
    }
} else {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
    Move-Item -LiteralPath $ready -Destination $Destination
}
Write-Output "Pinned libaria2 staged: $Destination"
