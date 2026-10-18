[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$OutputRoot)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path (Split-Path -Parent $PSScriptRoot) "NativePackageArchive.ps1")
Add-Type -AssemblyName System.IO.Compression.FileSystem
$run = Join-Path ([System.IO.Path]::GetFullPath($OutputRoot)) ([Guid]::NewGuid().ToString("N"))
$stage = Join-Path $run "stage"
New-Item -ItemType Directory -Path (Join-Path $stage "runtime") -Force | Out-Null
$expected = [ordered]@{
    "manifest.json" = "{`n  `"execution`": {`"type`": `"framework_art`"}`n}"
    "art.runtime.json" = '{"protocolVersion":"loom.art.runtime.v1"}'
    "runtime/main.ps1" = "Write-Output 'fixture only'"
    "signature.json" = '{"fixture":"not a real signature"}'
}
foreach ($name in $expected.Keys) {
    [System.IO.File]::WriteAllText((Join-Path $stage $name), $expected[$name], [System.Text.UTF8Encoding]::new($false))
}
$zip = Join-Path $run "package.zip"
Write-LoomNativePackageArchive -Stage $stage -Destination $zip
$archive = [System.IO.Compression.ZipFile]::OpenRead($zip)
try {
    if ($archive.Entries.Count -ne $expected.Count) { throw "ENTRY_COUNT_MISMATCH" }
    foreach ($entry in $archive.Entries) {
        if ($entry.FullName.Contains('\') -or -not $expected.Contains($entry.FullName)) { throw "ENTRY_PATH_MISMATCH" }
        $stream = $entry.Open()
        $memory = [System.IO.MemoryStream]::new()
        try {
            $stream.CopyTo($memory)
            $actual = [Convert]::ToBase64String($memory.ToArray())
            $original = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($expected[$entry.FullName]))
            if ($actual -ne $original) { throw "SIGNED_FILE_BYTES_CHANGED" }
        } finally { $stream.Dispose(); $memory.Dispose() }
    }
} finally { $archive.Dispose() }
$before = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
$rejected = $false
try { Write-LoomNativePackageArchive -Stage $stage -Destination $zip } catch { $rejected = $true }
if (-not $rejected -or (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash -ne $before) { throw "EXISTING_OUTPUT_CHANGED" }
$rejected = $false
try { Write-LoomNativePackageArchive -Stage $stage -Destination (Join-Path $stage "self.zip") } catch { $rejected = $true }
if (-not $rejected -or (Test-Path -LiteralPath (Join-Path $stage "self.zip"))) { throw "OUTPUT_STAGE_BOUNDARY_FAILED" }
Write-Output "Native package archive tests passed: $zip"
