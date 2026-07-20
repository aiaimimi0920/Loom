[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.."))
$verifyPath = Join-Path $repoRoot "scripts/verify-release.ps1"
$layoutPath = Join-Path $repoRoot "scripts/LoomReleaseLayout.ps1"
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-release-integrity-" + [Guid]::NewGuid().ToString("N"))

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ($Expected -ne $Actual) { throw "$Message Expected=[$Expected] Actual=[$Actual]" }
}

function Write-Utf8NoBom {
    param([string]$Path, [string]$Value)
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    [System.IO.File]::WriteAllText($Path, $Value, [System.Text.UTF8Encoding]::new($false))
}

function Write-Ascii {
    param([string]$Path, [string]$Value)
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    [System.IO.File]::WriteAllText($Path, $Value, [System.Text.ASCIIEncoding]::new())
}

function Get-Record {
    param([string]$PackageDir, [string]$Kind, [string]$RelativePath)
    $path = Join-Path $PackageDir $RelativePath
    $file = Get-Item -LiteralPath $path
    return [ordered]@{
        kind = $Kind
        name = $file.Name
        path = $RelativePath.Replace("/", "\")
        bytes = [int64]$file.Length
        sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function Write-FixtureFile {
    param([string]$PackageDir, [string]$RelativePath, [string]$Content)
    Write-Utf8NoBom -Path (Join-Path $PackageDir $RelativePath) -Value $Content
}

function New-FixtureZip {
    param([string]$PackageDir, [string]$RelativeZipPath, [hashtable[]]$Entries)
    $stage = Join-Path $tempRoot ("stage-" + [Guid]::NewGuid().ToString("N"))
    $zipPath = Join-Path $PackageDir $RelativeZipPath
    try {
        New-Item -ItemType Directory -Path $stage -Force | Out-Null
        foreach ($entry in $Entries) {
            Write-FixtureFile -PackageDir $stage -RelativePath ([string]$entry.path) -Content ([string]$entry.content)
        }
        New-Item -ItemType Directory -Path (Split-Path -Parent $zipPath) -Force | Out-Null
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
    }
    finally {
        if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    }
}

function New-TraversalZip {
    param([string]$ZipPath)

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $stream = [System.IO.File]::Open($ZipPath, [System.IO.FileMode]::Create)
    $archive = [System.IO.Compression.ZipArchive]::new(
        $stream,
        [System.IO.Compression.ZipArchiveMode]::Create,
        $false
    )
    try {
        $directoryEntry = $archive.CreateEntry((".." + "/escape/"))
        $fileEntry = $archive.CreateEntry("loom.exe")
        $writer = [System.IO.StreamWriter]::new($fileEntry.Open())
        try {
            $writer.Write("fixture")
        }
        finally {
            $writer.Dispose()
        }
    }
    finally {
        $archive.Dispose()
        $stream.Dispose()
    }
}

function Write-PackageChecksums {
    param([string]$PackageDir)
    $lines = @()
    foreach ($entry in (Get-ChildItem -LiteralPath $PackageDir -Recurse -File | Sort-Object FullName)) {
        $relative = $entry.FullName.Substring($PackageDir.Length + 1).Replace("/", "\")
        if ($relative -ieq "checksums.sha256") { continue }
        $hash = (Get-FileHash -LiteralPath $entry.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $lines += "$hash  $relative"
    }
    Write-Ascii -Path (Join-Path $PackageDir "checksums.sha256") -Value (($lines -join [Environment]::NewLine) + [Environment]::NewLine)
}

function New-IntegrityFixture {
    param(
        [string]$Name,
        [switch]$ExtraRootExecutable,
        [switch]$ExtraCliEntry,
        [switch]$CliMetadataMismatch,
        [switch]$ArtifactNamingMismatch,
        [ValidateSet("valid", "desktop-wrong", "cli-wrong")]
        [string]$SidecarMode = "valid"
    )

    $packageDir = Join-Path $tempRoot $Name
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    Write-FixtureFile -PackageDir $packageDir -RelativePath "Loom.exe" -Content "desktop-fixture"
    Write-FixtureFile -PackageDir $packageDir -RelativePath "runtime/loom-daemon.exe" -Content "daemon-fixture"
    Write-FixtureFile -PackageDir $packageDir -RelativePath "BUILD_INFO.txt" -Content ("Loom Windows release artifact" + [Environment]::NewLine)
    if ($ExtraRootExecutable) {
        Write-FixtureFile -PackageDir $packageDir -RelativePath "loom-desktop.exe" -Content "stale-desktop-fixture"
    }

    $desktopZipRelative = "packages/Loom-integrity-fixture-windows-x64.zip"
    $cliZipRelative = "packages/Loom-CLI-integrity-fixture-windows-x64.zip"
    if ($ArtifactNamingMismatch) {
        $desktopZipRelative = "packages/Loom-wrong-version-windows-x64.zip"
        $cliZipRelative = "packages/Loom-CLI-wrong-version-windows-x64.zip"
    }
    New-FixtureZip -PackageDir $packageDir -RelativeZipPath $desktopZipRelative -Entries @(
        @{ path = "Loom.exe"; content = "desktop-fixture" },
        @{ path = "runtime/loom-daemon.exe"; content = "daemon-fixture" }
    )
    $cliEntries = @(@{ path = "loom.exe"; content = "cli-fixture" })
    if ($ExtraCliEntry) { $cliEntries += @{ path = "extra.txt"; content = "unexpected-entry" } }
    New-FixtureZip -PackageDir $packageDir -RelativeZipPath $cliZipRelative -Entries $cliEntries

    $desktopZipPath = Join-Path $packageDir $desktopZipRelative
    $cliZipPath = Join-Path $packageDir $cliZipRelative
    $desktopZipHash = (Get-FileHash -LiteralPath $desktopZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $cliZipHash = (Get-FileHash -LiteralPath $cliZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $desktopZipName = Split-Path -Leaf $desktopZipPath
    $cliZipName = Split-Path -Leaf $cliZipPath

    $desktopSidecarLine = "$desktopZipHash  $desktopZipName"
    $cliSidecarLine = "$cliZipHash  $cliZipName"
    if ($SidecarMode -eq "desktop-wrong") { $desktopSidecarLine = ("0" * 64) + "  " + $desktopZipName }
    if ($SidecarMode -eq "cli-wrong") { $cliSidecarLine = ("0" * 64) + "  " + $cliZipName }
    Write-Ascii -Path "$desktopZipPath.sha256" -Value ($desktopSidecarLine + [Environment]::NewLine)
    Write-Ascii -Path "$cliZipPath.sha256" -Value ($cliSidecarLine + [Environment]::NewLine)

    $desktopRecord = Get-Record -PackageDir $packageDir -Kind "desktop-zip" -RelativePath $desktopZipRelative
    $desktopSidecarRecord = Get-Record -PackageDir $packageDir -Kind "zip-sha256" -RelativePath "$desktopZipRelative.sha256"
    $cliRecord = Get-Record -PackageDir $packageDir -Kind "cli-zip" -RelativePath $cliZipRelative
    $cliSidecarRecord = Get-Record -PackageDir $packageDir -Kind "cli-zip-sha256" -RelativePath "$cliZipRelative.sha256"

    $cliArtifact = [ordered]@{
        name = "loom-cli"
        entryName = "loom.exe"
        zipName = $cliZipName
        path = $cliZipRelative.Replace("/", "\")
        bytes = $cliRecord.bytes
        sha256 = $cliRecord.sha256
    }
    if ($CliMetadataMismatch) {
        $cliArtifact.bytes = [int64]$cliArtifact.bytes + 1
        $cliArtifact.sha256 = "a" * 64
    }

    $manifest = [ordered]@{
        schemaVersion = 1
        app = "Loom"
        sourceProject = "Loom"
        versionId = "integrity-fixture"
        builtAt = "2026-01-01T00:00:00.0000000Z"
        gitHead = "integrity-fixture"
        gitShortSha = "integrity"
        gitDirty = $false
        profile = "release"
        target = "windows-x64"
        repoRoot = "."
        outputRoot = "."
        destination = "integrity-fixture"
        commands = @(
            [ordered]@{ display = "fixture build"; workingDirectory = "."; logPath = "logs/build.log" },
            [ordered]@{ display = "fixture package"; workingDirectory = "."; logPath = "logs/package.log" }
        )
        exes = @(
            (Get-Record -PackageDir $packageDir -Kind "exe" -RelativePath "Loom.exe"),
            (Get-Record -PackageDir $packageDir -Kind "exe" -RelativePath "runtime/loom-daemon.exe")
        )
        supportFiles = @()
        cliArtifact = $cliArtifact
        buildInfo = (Get-Record -PackageDir $packageDir -Kind "build-info" -RelativePath "BUILD_INFO.txt")
        artifacts = @($desktopRecord, $desktopSidecarRecord, $cliRecord, $cliSidecarRecord)
        checksums = "checksums.sha256"
        sourceGitDirty = $false
        sourcePaths = @(".")
    }

    Write-Utf8NoBom -Path (Join-Path $packageDir "manifest.json") -Value (($manifest | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
    Write-PackageChecksums -PackageDir $packageDir
    return $packageDir
}

function Invoke-VerifierProcess {
    param([string]$PackageDir)

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = "powershell.exe"
    $startInfo.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$verifyPath`" -PackageDir `"$PackageDir`""
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        [void]$process.Start()
        $stdout = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        return [pscustomobject]@{
            exitCode = $process.ExitCode
            output = ($stdout + $stderr).Trim()
        }
    }
    finally {
        $process.Dispose()
    }
}

function Invoke-VerifierSuccess {
    param([string]$PackageDir)
    $result = Invoke-VerifierProcess -PackageDir $PackageDir
    Assert-Equal 0 $result.exitCode "Valid integrity fixture must pass: $($result.output)"
}

function Invoke-VerifierFailure {
    param([string]$PackageDir, [string]$ExpectedMessage)
    $result = Invoke-VerifierProcess -PackageDir $PackageDir
    Assert-True ($result.exitCode -ne 0) "Tampered fixture unexpectedly passed."
    Assert-True ($result.output.Contains($ExpectedMessage)) "Expected failure text was not reported: $ExpectedMessage$([Environment]::NewLine)$($result.output)"
}

New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
try {
    $valid = New-IntegrityFixture -Name "valid"
    Invoke-VerifierSuccess -PackageDir $valid

    $extraRoot = New-IntegrityFixture -Name "extra-root" -ExtraRootExecutable
    Invoke-VerifierFailure -PackageDir $extraRoot -ExpectedMessage "Loom desktop package root must contain exactly one executable named Loom.exe."

    $extraCli = New-IntegrityFixture -Name "extra-cli" -ExtraCliEntry
    Invoke-VerifierFailure -PackageDir $extraCli -ExpectedMessage "Loom CLI ZIP must contain exactly one loom.exe entry."

    . $layoutPath
    $layoutError = $null
    try {
        [void](Get-LoomReleaseLayout -PackageDir $extraCli -CliExtractRoot (Join-Path $extraCli "extracted"))
        throw "Malformed CLI ZIP unexpectedly passed shared layout validation."
    }
    catch {
        $layoutError = $_.Exception.Message
    }
    Assert-True ($layoutError.Contains("Loom CLI ZIP must contain exactly one loom.exe entry.")) "Shared layout helper did not reject malformed CLI ZIP: $layoutError"

    $traversalZip = Join-Path $tempRoot "traversal.zip"
    New-TraversalZip -ZipPath $traversalZip
    $traversalError = $null
    try {
        [void](Get-LoomArchiveFileEntries -ZipPath $traversalZip)
        throw "Traversal archive unexpectedly passed shared entry validation."
    }
    catch {
        $traversalError = $_.Exception.Message
    }
    Assert-True ($traversalError.Contains("Invalid Loom archive entry: ..\escape\")) "Shared layout helper did not reject directory traversal: $traversalError"

    $metadata = New-IntegrityFixture -Name "metadata-mismatch" -CliMetadataMismatch
    Invoke-VerifierFailure -PackageDir $metadata -ExpectedMessage "Loom CLI artifact metadata mismatch."

    $artifactNaming = New-IntegrityFixture -Name "artifact-naming" -ArtifactNamingMismatch
    Invoke-VerifierFailure -PackageDir $artifactNaming -ExpectedMessage "Desktop ZIP name does not match the manifest version."

    $desktopSidecar = New-IntegrityFixture -Name "desktop-sidecar" -SidecarMode desktop-wrong
    Invoke-VerifierFailure -PackageDir $desktopSidecar -ExpectedMessage "ZIP checksum sidecar content mismatch for Loom-integrity-fixture-windows-x64.zip."

    $cliSidecar = New-IntegrityFixture -Name "cli-sidecar" -SidecarMode cli-wrong
    Invoke-VerifierFailure -PackageDir $cliSidecar -ExpectedMessage "ZIP checksum sidecar content mismatch for Loom-CLI-integrity-fixture-windows-x64.zip."

    Write-Output "Loom release integrity tamper contract passed."
}
finally {
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force }
}
