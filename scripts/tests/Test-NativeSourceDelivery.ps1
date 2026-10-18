[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$root = Join-Path ([IO.Path]::GetTempPath()) ('loom-native-source-test-' + [Guid]::NewGuid().ToString('N'))
$download = Join-Path $repo 'scripts/Get-LoomNativeSourceMaterials.ps1'
$export = Join-Path $repo 'scripts/New-LoomLibAria2SourceBundle.ps1'
function Assert-Rejected([scriptblock]$Action, [string]$Expected) {
    try { & $Action }
    catch {
        if (-not $_.Exception.Message.Contains($Expected)) { throw }
        return
    }
    throw "Expected rejection: $Expected"
}
New-Item -ItemType Directory -Path $root | Out-Null
try {
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repo 'native/libaria2/toolchain-sources.json') | ConvertFrom-Json
    if (@($manifest.files).Count -ne 5 -or @($manifest.files.name | Select-Object -Unique).Count -ne 5) {
        throw 'Unexpected native source inventory'
    }
    foreach ($file in $manifest.files) {
        if ([IO.Path]::GetFileName(([Uri]$file.url).AbsolutePath) -cne $file.name) { throw 'Source URL/name mismatch' }
    }
    Assert-Rejected { & $download -OutputRoot $root } 'NATIVE_SOURCE_OUTPUT_EXISTS'
    $first = $manifest.files[0]
    $filePath = Join-Path $root $first.name
    [IO.File]::WriteAllBytes($filePath, [byte[]]::new([int]$first.bytes))
    Assert-Rejected { & $download -OutputRoot $root -Resume } 'NATIVE_SOURCE_DIGEST_MISMATCH'
    [IO.File]::WriteAllBytes($filePath, [byte[]]::new([int]$first.bytes + 1))
    Assert-Rejected { & $download -OutputRoot $root -Resume } 'NATIVE_SOURCE_EXISTING_FILE_INVALID'
    $build = Join-Path $root 'build'
    New-Item -ItemType Directory -Path $build | Out-Null
    [IO.File]::WriteAllText((Join-Path $build 'build-receipt.json'), '{"sourceSha256":"fixture"}', [Text.UTF8Encoding]::new($false))
    $output = Join-Path $root 'export'
    Assert-Rejected { & $export -BuildRoot $build -ArchivePath $filePath -OutputRoot $output } 'SOURCE_BUILD_RECEIPT_INCOMPLETE'
    if (Test-Path -LiteralPath $output) { throw 'Rejected export created output' }
    New-Item -ItemType Directory -Path (Join-Path $root 'download-receipt.json') | Out-Null
    Assert-Rejected { & $download -OutputRoot $root -Resume } 'NATIVE_SOURCE_RECEIPT_INVALID'
    Write-Output 'Native source inventory, overwrite, corrupt resume, oversize and legacy receipt contracts passed.'
} finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase)) { throw 'TEST_CLEANUP_OUTSIDE_TEMP' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
