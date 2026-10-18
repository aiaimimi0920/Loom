[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OutputRoot,
    [string]$ArchivePath,
    [string]$Distribution = 'Ubuntu'
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $OutputRoot) { throw 'LIBARIA2_OUTPUT_EXISTS' }
$metadata = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repo 'security/libaria2-runtime.json') | ConvertFrom-Json
New-Item -ItemType Directory -Path $OutputRoot | Out-Null
if (-not $ArchivePath) {
    $ArchivePath = Join-Path $OutputRoot 'aria2-1.37.0.tar.xz'
    Invoke-WebRequest -Uri $metadata.sourceUrl -OutFile $ArchivePath -TimeoutSec 180
}
$ArchivePath = [IO.Path]::GetFullPath($ArchivePath)
if ((Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $metadata.sourceSha256) { throw 'LIBARIA2_SOURCE_DIGEST_MISMATCH' }
function Convert-ToWslPath([string]$Path) {
    $Path = [IO.Path]::GetFullPath($Path).Replace('\', '/')
    if ($Path -notmatch '^([A-Za-z]):/(.*)$') { throw 'LIBARIA2_LOCAL_DRIVE_REQUIRED' }
    return '/mnt/' + $Matches[1].ToLowerInvariant() + '/' + $Matches[2]
}
$bridge = Join-Path $OutputRoot 'bridge.cpp'
$script = Join-Path $OutputRoot 'build.sh'
Copy-Item -LiteralPath (Join-Path $repo 'native/libaria2/bridge.cpp') -Destination $bridge
Copy-Item -LiteralPath (Join-Path $repo 'native/libaria2/build.sh') -Destination $script
& wsl.exe -d $Distribution -- bash (Convert-ToWslPath $script) (Convert-ToWslPath $OutputRoot) (Convert-ToWslPath $ArchivePath) (Convert-ToWslPath $bridge)
if ($LASTEXITCODE -ne 0) { throw "LIBARIA2_BUILD_FAILED: $OutputRoot" }
$dll = Join-Path $OutputRoot 'loom_aria2.dll'
$hash = (Get-FileHash -LiteralPath $dll -Algorithm SHA256).Hash.ToLowerInvariant()
$imports = Get-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputRoot 'imports.txt')
$dependencies = @($imports | ForEach-Object { if ($_ -match 'DLL Name:\s*(\S+)') { $Matches[1].ToLowerInvariant() } })
$allowed = @('advapi32.dll', 'crypt32.dll', 'iphlpapi.dll', 'kernel32.dll', 'msvcrt.dll', 'secur32.dll', 'ws2_32.dll')
if ($dependencies.Count -eq 0 -or @($dependencies | Where-Object { $_ -notin $allowed }).Count) { throw 'LIBARIA2_UNEXPECTED_DLL_DEPENDENCY' }
Copy-Item -LiteralPath (Join-Path $OutputRoot 'aria2-1.37.0/COPYING') -Destination (Join-Path $OutputRoot 'COPYING')
$receipt = [ordered]@{
    sourceSha256 = $metadata.sourceSha256
    bridgeSha256 = (Get-FileHash -LiteralPath $bridge).Hash.ToLowerInvariant()
    buildScriptSha256 = (Get-FileHash -LiteralPath $script).Hash.ToLowerInvariant()
    linkMapSha256 = (Get-FileHash -LiteralPath (Join-Path $OutputRoot 'link.map')).Hash.ToLowerInvariant()
    librarySha256 = $hash
    libraryBytes = (Get-Item -LiteralPath $dll).Length
    dependencies = $dependencies
    sourceDateEpoch = 1700000000
    matchesRuntimePin = $hash -eq $metadata.librarySha256
    releaseApproved = $false
}
[IO.File]::WriteAllText((Join-Path $OutputRoot 'build-receipt.json'), ($receipt | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
Write-Output ($receipt | ConvertTo-Json -Depth 5)
