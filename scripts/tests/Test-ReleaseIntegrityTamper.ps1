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

function New-WhitespaceEntryZip {
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
        foreach ($entryName in @("loom.exe", " ")) {
            $entry = $archive.CreateEntry($entryName)
            $writer = [System.IO.StreamWriter]::new($entry.Open())
            try {
                $writer.Write("fixture")
            }
            finally {
                $writer.Dispose()
            }
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
        [switch]$CliEntryCaseMismatch,
        [switch]$CliKindCaseMismatch,
        [switch]$ArtifactNamingMismatch,
        [switch]$PluginSdkPathMismatch,
        [switch]$ForwardSlashPaths,
        [ValidateSet("valid", "desktop-wrong", "cli-wrong", "no-newline", "extra-line")]
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
    $pluginSdkZipRelative = "packages/Loom-Plugin-SDK-integrity-fixture-windows-x64.zip"
    if ($PluginSdkPathMismatch) {
        $pluginSdkZipRelative = "sdk/Loom-Plugin-SDK-integrity-fixture-windows-x64.zip"
    }
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
    $pluginSdkEntries = @(
        @{ path = "loom-plugin.exe"; content = "plugin-cli-fixture" },
        @{ path = "protocol/README.md"; content = "protocol" },
        @{ path = "protocol/schemas/framework-manifest.v1.schema.json"; content = "{}" },
        @{ path = "protocol/schemas/framework-execute-request.v1.schema.json"; content = "{}" },
        @{ path = "protocol/schemas/framework-execute-response.v1.schema.json"; content = "{}" },
        @{ path = "protocol/schemas/framework-authoring.v1.schema.json"; content = "{}" },
        @{ path = "protocol/schemas/art-runtime.v1.schema.json"; content = "{}" },
        @{ path = "protocol/schemas/device-session.v1.schema.json"; content = "{}" },
        @{ path = "protocol/schemas/surface-manifest.v1.schema.json"; content = "{}" },
        @{ path = "protocol/schemas/surface-message.v1.schema.json"; content = "{}" },
        @{ path = "protocol/schemas/surface-scene.v1.schema.json"; content = "{}" },
        @{ path = "sdk/surface/README.md"; content = "surface sdk" },
        @{ path = "sdk/surface/neuro-surface.d.ts"; content = "export {};" },
        @{ path = "docs/plugin-development.md"; content = "development" },
        @{ path = "docs/plugin-security.md"; content = "security" },
        @{ path = "docs/plugin-permissions.md"; content = "permissions" },
        @{ path = "docs/plugin-signing-and-trust.md"; content = "signing" },
        @{ path = "docs/plugin-migration.md"; content = "migration" },
        @{ path = "docs/release-provenance.md"; content = "provenance" }
    )
    New-FixtureZip -PackageDir $packageDir -RelativeZipPath $pluginSdkZipRelative -Entries $pluginSdkEntries

    $desktopZipPath = Join-Path $packageDir $desktopZipRelative
    $cliZipPath = Join-Path $packageDir $cliZipRelative
    $pluginSdkZipPath = Join-Path $packageDir $pluginSdkZipRelative
    $desktopZipHash = (Get-FileHash -LiteralPath $desktopZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $cliZipHash = (Get-FileHash -LiteralPath $cliZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $pluginSdkZipHash = (Get-FileHash -LiteralPath $pluginSdkZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $desktopZipName = Split-Path -Leaf $desktopZipPath
    $cliZipName = Split-Path -Leaf $cliZipPath
    $pluginSdkZipName = Split-Path -Leaf $pluginSdkZipPath

    $desktopSidecarLine = "$desktopZipHash  $desktopZipName"
    $cliSidecarLine = "$cliZipHash  $cliZipName"
    if ($SidecarMode -eq "desktop-wrong") { $desktopSidecarLine = ("0" * 64) + "  " + $desktopZipName }
    if ($SidecarMode -eq "cli-wrong") { $cliSidecarLine = ("0" * 64) + "  " + $cliZipName }
    $sidecarSuffix = if ($SidecarMode -eq "no-newline") {
        ""
    }
    elseif ($SidecarMode -eq "extra-line") {
        "`r`nunexpected`r`n"
    }
    else {
        [Environment]::NewLine
    }
    Write-Ascii -Path "$desktopZipPath.sha256" -Value ($desktopSidecarLine + $sidecarSuffix)
    Write-Ascii -Path "$cliZipPath.sha256" -Value ($cliSidecarLine + $sidecarSuffix)
    Write-Ascii -Path "$pluginSdkZipPath.sha256" -Value ("$pluginSdkZipHash  $pluginSdkZipName" + [Environment]::NewLine)

    Write-FixtureFile -PackageDir $packageDir -RelativePath "sbom/Loom-integrity-fixture.cdx.json" -Content '{"bomFormat":"CycloneDX","specVersion":"1.6","components":[]}'
    Write-FixtureFile -PackageDir $packageDir -RelativePath "sbom/Loom-integrity-fixture.spdx.json" -Content '{"spdxVersion":"SPDX-2.3","packages":[]}'
    Write-FixtureFile -PackageDir $packageDir -RelativePath "provenance/build-provenance.json" -Content '{"schemaVersion":1,"gitHead":"integrity-fixture","gitDirty":false,"subjects":[]}'

    $desktopRecord = Get-Record -PackageDir $packageDir -Kind "desktop-zip" -RelativePath $desktopZipRelative
    $desktopSidecarRecord = Get-Record -PackageDir $packageDir -Kind "zip-sha256" -RelativePath "$desktopZipRelative.sha256"
    $cliRecord = Get-Record -PackageDir $packageDir -Kind "cli-zip" -RelativePath $cliZipRelative
    $cliSidecarRecord = Get-Record -PackageDir $packageDir -Kind "cli-zip-sha256" -RelativePath "$cliZipRelative.sha256"
    $pluginSdkRecord = Get-Record -PackageDir $packageDir -Kind "plugin-sdk-zip" -RelativePath $pluginSdkZipRelative
    $pluginSdkSidecarRecord = Get-Record -PackageDir $packageDir -Kind "plugin-sdk-zip-sha256" -RelativePath "$pluginSdkZipRelative.sha256"
    if ($CliKindCaseMismatch) { $cliRecord.kind = "CLI-ZIP" }

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
    if ($CliEntryCaseMismatch) { $cliArtifact.entryName = "LOOM.EXE" }

    $pluginSdkArtifact = [ordered]@{
        name = "loom-plugin-sdk"
        entryName = "loom-plugin.exe"
        zipName = $pluginSdkZipName
        path = $pluginSdkZipRelative.Replace("/", "\")
        bytes = $pluginSdkRecord.bytes
        sha256 = $pluginSdkRecord.sha256
        protocolVersion = "loom.framework.v1"
        schemaCount = 9
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
        pluginSdkArtifact = $pluginSdkArtifact
        buildInfo = (Get-Record -PackageDir $packageDir -Kind "build-info" -RelativePath "BUILD_INFO.txt")
        artifacts = @($desktopRecord, $desktopSidecarRecord, $cliRecord, $cliSidecarRecord, $pluginSdkRecord, $pluginSdkSidecarRecord)
        sbom = @(
            (Get-Record -PackageDir $packageDir -Kind "sbom" -RelativePath "sbom/Loom-integrity-fixture.cdx.json"),
            (Get-Record -PackageDir $packageDir -Kind "sbom" -RelativePath "sbom/Loom-integrity-fixture.spdx.json")
        )
        provenance = (Get-Record -PackageDir $packageDir -Kind "provenance" -RelativePath "provenance/build-provenance.json")
        checksums = "checksums.sha256"
        sourceGitDirty = $false
        sourcePaths = @(".")
    }

    if ($ForwardSlashPaths) {
        foreach ($record in @($manifest.exes + $manifest.artifacts + @($manifest.buildInfo))) {
            $record.path = ([string]$record.path).Replace("\", "/")
        }
        $manifest.cliArtifact.path = ([string]$manifest.cliArtifact.path).Replace("\", "/")
        $manifest.pluginSdkArtifact.path = ([string]$manifest.pluginSdkArtifact.path).Replace("\", "/")
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

    $noNewline = New-IntegrityFixture -Name "sidecar-no-newline" -SidecarMode no-newline
    Invoke-VerifierSuccess -PackageDir $noNewline

    $forwardSlash = New-IntegrityFixture -Name "forward-slash-paths" -ForwardSlashPaths
    Invoke-VerifierSuccess -PackageDir $forwardSlash

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

    $whitespaceZip = Join-Path $tempRoot "whitespace-entry.zip"
    New-WhitespaceEntryZip -ZipPath $whitespaceZip
    $whitespaceError = $null
    try {
        [void](Get-LoomArchiveFileEntries -ZipPath $whitespaceZip)
        throw "Whitespace-named archive entry unexpectedly passed validation."
    }
    catch {
        $whitespaceError = $_.Exception.Message
    }
    Assert-True ($whitespaceError.Contains("Invalid Loom archive entry:")) "Whitespace-named ZIP file was discarded instead of rejected: $whitespaceError"

    $validExtractRoot = Join-Path $tempRoot "valid-extraction"
    $validLayout = Get-LoomReleaseLayout -PackageDir $valid -CliExtractRoot $validExtractRoot
    $extractedFiles = @(Get-ChildItem -LiteralPath $validExtractRoot -Recurse -File)
    Assert-Equal 1 $extractedFiles.Count "Valid CLI extraction must produce exactly one file."
    Assert-True ([string]::Equals($extractedFiles[0].Name, "loom.exe", [System.StringComparison]::Ordinal)) "Valid CLI extraction produced an unexpected file name."
    Assert-True (Test-Path -LiteralPath $validLayout.cliExe -PathType Leaf) "Valid CLI extraction did not return the loom.exe path."

    $staleExtractRoot = Join-Path $tempRoot "stale-extraction"
    New-Item -ItemType Directory -Path $staleExtractRoot -Force | Out-Null
    Write-FixtureFile -PackageDir $staleExtractRoot -RelativePath "stale.txt" -Content "stale"
    $staleExtractionError = $null
    try {
        [void](Get-LoomReleaseLayout -PackageDir $valid -CliExtractRoot $staleExtractRoot)
        throw "Non-empty CLI extraction destination unexpectedly passed validation."
    }
    catch {
        $staleExtractionError = $_.Exception.Message
    }
    Assert-True ($staleExtractionError.Contains("Loom CLI extraction destination must be empty:")) "Non-empty CLI extraction destination was not rejected: $staleExtractionError"

    $metadata = New-IntegrityFixture -Name "metadata-mismatch" -CliMetadataMismatch
    Invoke-VerifierFailure -PackageDir $metadata -ExpectedMessage "Loom CLI artifact metadata mismatch."

    $entryCase = New-IntegrityFixture -Name "entry-case-mismatch" -CliEntryCaseMismatch
    Invoke-VerifierFailure -PackageDir $entryCase -ExpectedMessage "Loom CLI artifact entry must be loom.exe."

    $kindCase = New-IntegrityFixture -Name "kind-case-mismatch" -CliKindCaseMismatch
    Invoke-VerifierFailure -PackageDir $kindCase -ExpectedMessage "Loom CLI artifact metadata mismatch."

    $artifactNaming = New-IntegrityFixture -Name "artifact-naming" -ArtifactNamingMismatch
    Invoke-VerifierFailure -PackageDir $artifactNaming -ExpectedMessage "Desktop ZIP name does not match the manifest version."

    $pluginSdkPath = New-IntegrityFixture -Name "plugin-sdk-path" -PluginSdkPathMismatch
    Invoke-VerifierFailure -PackageDir $pluginSdkPath -ExpectedMessage "Plugin SDK ZIP path does not match its name."

    $desktopSidecar = New-IntegrityFixture -Name "desktop-sidecar" -SidecarMode desktop-wrong
    Invoke-VerifierFailure -PackageDir $desktopSidecar -ExpectedMessage "ZIP checksum sidecar content mismatch for Loom-integrity-fixture-windows-x64.zip."

    $cliSidecar = New-IntegrityFixture -Name "cli-sidecar" -SidecarMode cli-wrong
    Invoke-VerifierFailure -PackageDir $cliSidecar -ExpectedMessage "ZIP checksum sidecar content mismatch for Loom-CLI-integrity-fixture-windows-x64.zip."

    $extraSidecarLine = New-IntegrityFixture -Name "sidecar-extra-line" -SidecarMode extra-line
    Invoke-VerifierFailure -PackageDir $extraSidecarLine -ExpectedMessage "ZIP checksum sidecar content mismatch for Loom-integrity-fixture-windows-x64.zip."

    Write-Output "Loom release integrity tamper contract passed."
}
finally {
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force }
}
