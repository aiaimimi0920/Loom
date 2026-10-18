[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Destination,
    [string]$ArchivePath,
    [switch]$DevelopmentOnly
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$metadataPath = Join-Path $repo 'security/aria2-runtime.json'
$metadata = Get-Content -Raw -Encoding UTF8 -LiteralPath $metadataPath | ConvertFrom-Json
if (-not $DevelopmentOnly -and -not $metadata.releaseApproved) {
    throw 'ARIA2_RELEASE_NOT_APPROVED: verify bundled dependencies and corresponding-source distribution before release.'
}
$Destination = [IO.Path]::GetFullPath($Destination)
$cache = Join-Path 'C:/Users/Public/nas_home/AI/GameEditor/linshi' ('loom-aria2-stage-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $cache | Out-Null
if (-not $ArchivePath) {
    $ArchivePath = Join-Path $cache 'aria2.zip'
    Invoke-WebRequest -Uri $metadata.url -OutFile $ArchivePath -TimeoutSec 180
}
if ((Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $metadata.archiveSha256) {
    throw 'ARIA2_ARCHIVE_DIGEST_MISMATCH'
}
$unpacked = Join-Path $cache 'unpacked'
Expand-Archive -LiteralPath $ArchivePath -DestinationPath $unpacked
$source = Join-Path $unpacked $metadata.archiveRoot
if ((Get-FileHash -LiteralPath (Join-Path $source 'aria2c.exe') -Algorithm SHA256).Hash.ToLowerInvariant() -ne $metadata.executableSha256) {
    throw 'ARIA2_EXECUTABLE_DIGEST_MISMATCH'
}
$ready = [IO.Path]::GetFullPath((Join-Path $cache 'ready'))
New-Item -ItemType Directory -Path $ready | Out-Null
foreach ($file in @('aria2c.exe', 'COPYING', 'AUTHORS', 'LICENSE.OpenSSL', 'README.mingw')) {
    Copy-Item -LiteralPath (Join-Path $source $file) -Destination (Join-Path $ready $file)
}
Copy-Item -LiteralPath $metadataPath -Destination (Join-Path $ready 'aria2-runtime.json')
if (Test-Path -LiteralPath $Destination) {
    $existing = Get-Item -LiteralPath $Destination
    if (-not $existing.PSIsContainer -or ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'ARIA2_DESTINATION_INVALID' }
    foreach ($file in Get-ChildItem -LiteralPath $ready -File) {
        $target = Join-Path $Destination $file.Name
        if (-not (Test-Path -LiteralPath $target -PathType Leaf) -or
            (Get-FileHash -LiteralPath $target).Hash -ne (Get-FileHash -LiteralPath $file.FullName).Hash) {
            throw 'ARIA2_EXISTING_RUNTIME_CHANGED'
        }
    }
} else {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
    Move-Item -LiteralPath $ready -Destination $Destination
}
Write-Output "Pinned aria2 staged: $Destination"
