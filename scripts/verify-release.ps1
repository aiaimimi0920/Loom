[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [switch]$RunSmoke
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Resolve-PackageRelativePath {
    param(
        [string]$BasePath,
        [string]$RelativePath
    )

    Assert-True -Condition (-not [System.IO.Path]::IsPathRooted($RelativePath)) -Message "Package path must be relative: $RelativePath"
    Assert-True -Condition (-not $RelativePath.Contains("..")) -Message "Package path must not contain parent traversal: $RelativePath"
    return [System.IO.Path]::GetFullPath((Join-Path $BasePath $RelativePath))
}

function Get-ManifestRecord {
    param(
        [object]$Manifest,
        [string]$Name
    )

    $property = $Manifest.PSObject.Properties[$Name]
    Assert-True -Condition ($null -ne $property) -Message "Manifest is missing $Name."
    return @($property.Value)
}

function Assert-FileRecord {
    param(
        [string]$PackagePath,
        [object]$Record
    )

    $relativePath = [string]$Record.path
    Assert-True -Condition (-not [string]::IsNullOrWhiteSpace($relativePath)) -Message "Manifest file record has an empty path."
    $filePath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $relativePath
    Assert-True -Condition (Test-Path -LiteralPath $filePath -PathType Leaf) -Message "Manifest file is missing: $relativePath"
    $file = Get-Item -LiteralPath $filePath
    Assert-Equal -Expected ([int64]$Record.bytes) -Actual ([int64]$file.Length) -Message "Manifest byte count mismatch for $relativePath."
    $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert-Equal -Expected ([string]$Record.sha256) -Actual $actualHash -Message "Manifest SHA-256 mismatch for $relativePath."
    return $relativePath.Replace("/", "\")
}

function Get-ChecksumEntries {
    param([string]$PackagePath)

    $checksumPath = Join-Path $PackagePath "checksums.sha256"
    Assert-True -Condition (Test-Path -LiteralPath $checksumPath -PathType Leaf) -Message "Missing checksums.sha256."
    $entries = @{}
    foreach ($line in (Get-Content -LiteralPath $checksumPath -Encoding ASCII)) {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        if ($line -notmatch '^([0-9a-fA-F]{64})  (.+)$') {
            throw "Invalid checksum line: $line"
        }
        $relativePath = $Matches[2].Replace("/", "\")
        Assert-True -Condition (-not $entries.ContainsKey($relativePath)) -Message "Duplicate checksum entry: $relativePath"
        $entries[$relativePath] = $Matches[1].ToLowerInvariant()
    }
    return $entries
}

function Assert-Checksums {
    param(
        [string]$PackagePath,
        [hashtable]$Entries
    )

    $actualFiles = @(Get-ChildItem -LiteralPath $PackagePath -Recurse -File | ForEach-Object {
        $_.FullName.Substring($PackagePath.Length + 1).Replace("/", "\")
    } | Where-Object { $_ -ne "checksums.sha256" })
    Assert-Equal -Expected ($actualFiles.Count) -Actual ($Entries.Keys.Count) -Message "Checksum entry count must equal all package files except checksums.sha256."
    foreach ($relativePath in $actualFiles) {
        Assert-True -Condition $Entries.ContainsKey($relativePath) -Message "Missing checksum entry: $relativePath"
        $filePath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $relativePath
        $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
        Assert-Equal -Expected $Entries[$relativePath] -Actual $actualHash -Message "Checksum mismatch for $relativePath"
    }
    foreach ($relativePath in $Entries.Keys) {
        Assert-True -Condition ($actualFiles -contains $relativePath) -Message "Checksum references an untracked package file: $relativePath"
    }
}

function Assert-ZipPayload {
    param(
        [string]$PackagePath,
        [object]$Manifest,
        [string[]]$ExpectedPayloadPaths
    )

    $artifacts = Get-ManifestRecord -Manifest $Manifest -Name "artifacts"
    $zipRecord = @($artifacts | Where-Object { [string]$_.kind -eq "zip" })
    Assert-Equal -Expected 1 -Actual $zipRecord.Count -Message "Manifest must contain exactly one payload ZIP."
    $zipPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$zipRecord[0].path)
    Assert-True -Condition (Test-Path -LiteralPath $zipPath -PathType Leaf) -Message "Payload ZIP is missing."

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $actualEntries = @($archive.Entries | Where-Object { -not [string]::IsNullOrWhiteSpace($_.Name) } | ForEach-Object { $_.FullName.Replace("/", "\") } | Sort-Object)
    }
    finally {
        $archive.Dispose()
    }
    $expected = @($ExpectedPayloadPaths | Sort-Object)
    Assert-Equal -Expected ($expected -join "`n") -Actual ($actualEntries -join "`n") -Message "Payload ZIP contents do not match executable/support files."
}

$packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
Assert-True -Condition (Test-Path -LiteralPath $packageFullPath -PathType Container) -Message "Package directory is missing: $packageFullPath"

$manifestPath = Join-Path $packageFullPath "manifest.json"
Assert-True -Condition (Test-Path -LiteralPath $manifestPath -PathType Leaf) -Message "Missing manifest.json."
$manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
Assert-Equal -Expected 1 -Actual ([int]$manifest.schemaVersion) -Message "Manifest schema version mismatch."
Assert-Equal -Expected "Loom" -Actual ([string]$manifest.app) -Message "Manifest app mismatch."
Assert-Equal -Expected "Loom" -Actual ([string]$manifest.sourceProject) -Message "Manifest source project mismatch."
Assert-Equal -Expected "windows-x64" -Actual ([string]$manifest.target) -Message "Manifest target mismatch."
Assert-Equal -Expected "." -Actual (@($manifest.sourcePaths) -join ",") -Message "Manifest source paths must be standalone-relative."
Assert-True -Condition (-not ([string]$manifest.repoRoot).Contains("Neuro")) -Message "Manifest must not contain parent repository paths."
Assert-True -Condition (-not ([string]$manifest.destination).Contains(":")) -Message "Manifest destination must not contain an absolute local path."

$exeRecords = @(Get-ManifestRecord -Manifest $manifest -Name "exes")
$expectedExeNames = @("loom.exe", "loom-daemon.exe", "loom-desktop.exe")
Assert-Equal -Expected ($expectedExeNames -join ",") -Actual (@($exeRecords | ForEach-Object { [string]$_.name }) -join ",") -Message "Manifest executable set mismatch."
$payloadPaths = @()
foreach ($record in $exeRecords) {
    $payloadPaths += Assert-FileRecord -PackagePath $packageFullPath -Record $record
}

$supportRecords = @(Get-ManifestRecord -Manifest $manifest -Name "supportFiles")
foreach ($record in $supportRecords) {
    $payloadPaths += Assert-FileRecord -PackagePath $packageFullPath -Record $record
}

$buildInfo = @(Get-ManifestRecord -Manifest $manifest -Name "buildInfo")
Assert-Equal -Expected 1 -Actual $buildInfo.Count -Message "Manifest must contain one BUILD_INFO record."
[void](Assert-FileRecord -PackagePath $packageFullPath -Record $buildInfo[0])

$commands = @(Get-ManifestRecord -Manifest $manifest -Name "commands")
Assert-True -Condition ($commands.Count -ge 2) -Message "Manifest must retain build command provenance."
$artifactRecords = @(Get-ManifestRecord -Manifest $manifest -Name "artifacts")
foreach ($artifact in $artifactRecords) {
    [void](Assert-FileRecord -PackagePath $packageFullPath -Record $artifact)
}

$checksumEntries = Get-ChecksumEntries -PackagePath $packageFullPath
Assert-Checksums -PackagePath $packageFullPath -Entries $checksumEntries
Assert-ZipPayload -PackagePath $packageFullPath -Manifest $manifest -ExpectedPayloadPaths $payloadPaths

$smokeStatus = "not-run"
if ($RunSmoke) {
    $smokePath = Join-Path $repoRoot "scripts\smoke-release.ps1"
    Assert-True -Condition (Test-Path -LiteralPath $smokePath -PathType Leaf) -Message "Missing standalone smoke script: $smokePath"
    $evidenceRoot = Join-Path $repoRoot "target\runtime-smoke"
    $smokeOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $smokePath -PackageDir $packageFullPath -EvidenceRoot $evidenceRoot 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Standalone release smoke failed: $($smokeOutput -join [Environment]::NewLine)"
    }
    $smokeStatus = "passed"
}

$result = [ordered]@{
    schemaVersion = 1
    mode = "verify"
    app = "Loom"
    packageDir = $packageFullPath
    manifest = $manifestPath
    filesChecked = @($checksumEntries.Keys).Count
    smoke = $smokeStatus
}
Write-Output ($result | ConvertTo-Json -Depth 10)
