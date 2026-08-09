[CmdletBinding()]
param(
    [string]$VersionId = "",
    [string]$OutputRoot = ".\release\Loom",
    [switch]$NoZip,
    [switch]$DryRun,
    [switch]$RequireCleanSource
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$targetName = "windows-x64"

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Value
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Value, $encoding)
}

function Write-Ascii {
    param(
        [string]$Path,
        [string]$Value
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $Value, [System.Text.ASCIIEncoding]::new())
}

function Get-GitText {
    param([string[]]$Arguments)

    try {
        $output = & git -C $repoRoot @Arguments 2>$null
        if ($LASTEXITCODE -eq 0) {
            return (($output | ForEach-Object { $_.ToString() }) -join "`n").Trim()
        }
    }
    catch {
        return ""
    }
    return ""
}

function Get-GitDirty {
    try {
        $output = & git -C $repoRoot status --porcelain --untracked-files=all 2>$null
        if ($LASTEXITCODE -ne 0) {
            return $null
        }
        return (@($output | Where-Object { -not [string]::IsNullOrWhiteSpace($_.ToString()) }).Count -gt 0)
    }
    catch {
        return $null
    }
}

function Resolve-VersionId {
    param([string]$ExplicitVersionId)

    $value = $ExplicitVersionId
    if ([string]::IsNullOrWhiteSpace($value)) {
        $shortSha = Get-GitText -Arguments @("rev-parse", "--short=8", "HEAD")
        if ([string]::IsNullOrWhiteSpace($shortSha)) {
            $shortSha = "nogit"
        }
        $value = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$shortSha"
    }
    if ($value -notmatch '^[A-Za-z0-9._-]+$') {
        throw "Invalid VersionId '$value'. Use only letters, numbers, dot, underscore, and dash."
    }
    return $value
}

function Resolve-OutputRoot {
    param([string]$Value)

    if ([System.IO.Path]::IsPathRooted($Value)) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Value))
}

function Get-RepoRelativeOrExternal {
    param([string]$Path)

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $root = $repoRoot.TrimEnd("\", "/")
    if ($fullPath -eq $root) {
        return "."
    }
    if ($fullPath.StartsWith($root + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $fullPath.Substring($root.Length + 1).Replace("\", "/")
    }
    return "<external-output>"
}

function New-CommandSpec {
    param(
        [string]$Executable,
        [string[]]$Arguments,
        [string]$WorkingDirectory,
        [string]$Display,
        [string]$LogName
    )

    return [ordered]@{
        executable = $Executable
        arguments = @($Arguments)
        workingDirectory = [System.IO.Path]::GetFullPath($WorkingDirectory)
        display = $Display
        logName = $LogName
    }
}

function New-ExeSpec {
    param(
        [string]$Name,
        [string]$Source,
        [string]$DestinationRelativePath = ""
    )

    if ([string]::IsNullOrWhiteSpace($DestinationRelativePath)) {
        $DestinationRelativePath = $Name
    }

    return [ordered]@{
        name = $Name
        source = [System.IO.Path]::GetFullPath($Source)
        destinationRelativePath = $DestinationRelativePath
    }
}

function New-SupportSpec {
    param(
        [string]$Source,
        [string]$DestinationRelativePath
    )

    return [ordered]@{
        source = [System.IO.Path]::GetFullPath($Source)
        destinationRelativePath = $DestinationRelativePath
    }
}

function Get-LoomCatalog {
    param(
        [string]$FrameworkPackageOutputRoot,
        [string]$SampleArtPackageOutputRoot
    )

    $ocrRoot = Join-Path $repoRoot "resources\ocr"

    $exes = @(
        New-ExeSpec -Name "Loom.exe" -Source (Join-Path $repoRoot "apps\desktop\src-tauri\target\release\loom-desktop.exe")
        New-ExeSpec -Name "loom-daemon.exe" -Source (Join-Path $repoRoot "target\release\loom-daemon.exe") -DestinationRelativePath "runtime\loom-daemon.exe"
    )

    $support = @(
        New-SupportSpec -Source (Join-Path $ocrRoot "README.txt") -DestinationRelativePath "runtime\resources\ocr\README.txt"
        New-SupportSpec -Source (Join-Path $ocrRoot "ch_PP-OCRv4_det_infer.onnx") -DestinationRelativePath "runtime\resources\ocr\ch_PP-OCRv4_det_infer.onnx"
        New-SupportSpec -Source (Join-Path $ocrRoot "ch_ppocr_mobile_v2.0_cls_infer.onnx") -DestinationRelativePath "runtime\resources\ocr\ch_ppocr_mobile_v2.0_cls_infer.onnx"
        New-SupportSpec -Source (Join-Path $ocrRoot "ch_PP-OCRv4_rec_infer.onnx") -DestinationRelativePath "runtime\resources\ocr\ch_PP-OCRv4_rec_infer.onnx"
        New-SupportSpec -Source (Join-Path $ocrRoot "ch_PP-OCRv5_rec_mobile_infer.onnx") -DestinationRelativePath "runtime\resources\ocr\ch_PP-OCRv5_rec_mobile_infer.onnx"
        New-SupportSpec -Source (Join-Path $ocrRoot "fixtures\test_1.png") -DestinationRelativePath "runtime\resources\ocr\fixtures\test_1.png"
        New-SupportSpec -Source (Join-Path $ocrRoot "onnxruntime.dll") -DestinationRelativePath "runtime\resources\ocr\onnxruntime.dll"
        New-SupportSpec -Source (Join-Path $ocrRoot "onnxruntime_providers_shared.dll") -DestinationRelativePath "runtime\resources\ocr\onnxruntime_providers_shared.dll"
    )

    $cliArtifact = [ordered]@{
        name = "loom-cli"
        source = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "target\release\loom.exe"))
        entryName = "loom.exe"
        zipNamePattern = "Loom-CLI-{versionId}-windows-x64.zip"
    }

    $pluginSdkArtifact = [ordered]@{
        name = "loom-plugin-sdk"
        pluginCliSource = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "target\release\loom-plugin.exe"))
        pluginCliEntryName = "loom-plugin.exe"
        zipNamePattern = "Loom-Plugin-SDK-{versionId}-windows-x64.zip"
        files = @(
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\README.md") -DestinationRelativePath "protocol\README.md"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\framework-manifest.v1.schema.json") -DestinationRelativePath "protocol\schemas\framework-manifest.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\framework-execute-request.v1.schema.json") -DestinationRelativePath "protocol\schemas\framework-execute-request.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\framework-execute-response.v1.schema.json") -DestinationRelativePath "protocol\schemas\framework-execute-response.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\framework-authoring.v1.schema.json") -DestinationRelativePath "protocol\schemas\framework-authoring.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\art-runtime.v1.schema.json") -DestinationRelativePath "protocol\schemas\art-runtime.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-development.md") -DestinationRelativePath "docs\plugin-development.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-security.md") -DestinationRelativePath "docs\plugin-security.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-permissions.md") -DestinationRelativePath "docs\plugin-permissions.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-signing-and-trust.md") -DestinationRelativePath "docs\plugin-signing-and-trust.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-migration.md") -DestinationRelativePath "docs\plugin-migration.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\release-provenance.md") -DestinationRelativePath "docs\release-provenance.md"
        )
    }

    $frameworkPackageCatalog = [ordered]@{
        outputRoot = [System.IO.Path]::GetFullPath($FrameworkPackageOutputRoot)
        expectedIds = @(
            "process",
            "cloud_api",
            "mcp",
            "workflow"
        )
    }

    $sampleArtPackageCatalog = [ordered]@{
        outputRoot = [System.IO.Path]::GetFullPath($SampleArtPackageOutputRoot)
        expected = @(
            [ordered]@{ id = "custom-1770146354922"; framework = "process" }
            [ordered]@{ id = "custom-remove-bg-cloud"; framework = "cloud_api" }
            [ordered]@{ id = "custom-image-search"; framework = "mcp" }
            [ordered]@{ id = "custom-1770131241684"; framework = "process" }
        )
    }

    return [ordered]@{
        app = "Loom"
        sourceProject = "Loom"
        sourcePaths = @(".")
        exes = $exes
        supportFiles = $support
        cliArtifact = $cliArtifact
        pluginSdkArtifact = $pluginSdkArtifact
        frameworkPackageCatalog = $frameworkPackageCatalog
        sampleArtPackageCatalog = $sampleArtPackageCatalog
        commands = @(
            New-CommandSpec -Executable "cargo" `
                -Arguments @("build", "--locked", "--release", "-p", "loom-daemon", "-p", "loom-cli", "-p", "loom-plugin-cli") `
                -WorkingDirectory $repoRoot `
                -Display "cargo build --locked --release -p loom-daemon -p loom-cli -p loom-plugin-cli" `
                -LogName "build-01.log"
            New-CommandSpec -Executable "cmd.exe" `
                -Arguments @("/d", "/c", "npm ci") `
                -WorkingDirectory (Join-Path $repoRoot "apps\desktop") `
                -Display "npm ci" `
                -LogName "build-02.log"
            New-CommandSpec -Executable "cmd.exe" `
                -Arguments @("/d", "/c", "npm run tauri build -- --no-bundle") `
                -WorkingDirectory (Join-Path $repoRoot "apps\desktop") `
                -Display "npm run tauri build -- --no-bundle" `
                -LogName "build-03.log"
            New-CommandSpec -Executable "powershell.exe" `
                -Arguments @(
                    "-NoProfile",
                    "-ExecutionPolicy", "Bypass",
                    "-File", (Join-Path $repoRoot "scripts\Build-LoomArtFrameworkPackages.ps1"),
                    "-OutputRoot", $frameworkPackageCatalog.outputRoot,
                    "-Configuration", "Release"
                ) `
                -WorkingDirectory $repoRoot `
                -Display "Build-LoomArtFrameworkPackages.ps1 -OutputRoot packages\frameworks -Configuration Release" `
                -LogName "build-04.log"
            New-CommandSpec -Executable "powershell.exe" `
                -Arguments @(
                    "-NoProfile",
                    "-ExecutionPolicy", "Bypass",
                    "-File", (Join-Path $repoRoot "scripts\Build-LoomSampleArtPackages.ps1"),
                    "-OutputRoot", $sampleArtPackageCatalog.outputRoot,
                    "-Configuration", "Release"
                ) `
                -WorkingDirectory $repoRoot `
                -Display "Build-LoomSampleArtPackages.ps1 -OutputRoot packages\arts -Configuration Release" `
                -LogName "build-05.log"
        )
    }
}

function New-Plan {
    param(
        [System.Collections.Specialized.OrderedDictionary]$Catalog,
        [string]$ResolvedVersionId,
        [string]$ResolvedOutputRoot,
        [string]$Destination
    )

    return [ordered]@{
        schemaVersion = 1
        mode = "dry-run"
        app = "Loom"
        versionId = $ResolvedVersionId
        outputRoot = $ResolvedOutputRoot
        destination = $Destination
        target = $targetName
        sourcePaths = @($Catalog.sourcePaths)
        commands = @($Catalog.commands | ForEach-Object {
            [ordered]@{
                executable = $_.executable
                arguments = @($_.arguments)
                workingDirectory = $_.workingDirectory
                display = $_.display
                logName = $_.logName
            }
        })
        exes = @($Catalog.exes | ForEach-Object {
            [ordered]@{
                name = $_.name
                source = $_.source
                destinationRelativePath = $_.destinationRelativePath
            }
        })
        supportFiles = @($Catalog.supportFiles | ForEach-Object {
            [ordered]@{
                source = $_.source
                destinationRelativePath = $_.destinationRelativePath
            }
        })
        cliArtifact = [ordered]@{
            name = $Catalog.cliArtifact.name
            entryName = $Catalog.cliArtifact.entryName
            source = $Catalog.cliArtifact.source
            zipNamePattern = $Catalog.cliArtifact.zipNamePattern
        }
        pluginSdkArtifact = [ordered]@{
            name = $Catalog.pluginSdkArtifact.name
            pluginCliSource = $Catalog.pluginSdkArtifact.pluginCliSource
            pluginCliEntryName = $Catalog.pluginSdkArtifact.pluginCliEntryName
            zipNamePattern = $Catalog.pluginSdkArtifact.zipNamePattern
            files = @($Catalog.pluginSdkArtifact.files | ForEach-Object {
                [ordered]@{
                    source = $_.source
                    destinationRelativePath = $_.destinationRelativePath
                }
            })
        }
        frameworkPackageCatalog = [ordered]@{
            outputRoot = $Catalog.frameworkPackageCatalog.outputRoot
            expectedIds = @($Catalog.frameworkPackageCatalog.expectedIds)
        }
        sampleArtPackageCatalog = [ordered]@{
            outputRoot = $Catalog.sampleArtPackageCatalog.outputRoot
            expected = @($Catalog.sampleArtPackageCatalog.expected)
        }
        requireCleanSource = [bool]$RequireCleanSource
        zip = (-not $NoZip)
    }
}

function Invoke-CommandToLog {
    param(
        [System.Collections.Specialized.OrderedDictionary]$Command,
        [string]$LogPath
    )

    $header = @(
        "Command: $($Command.display)"
        "Executable: $($Command.executable)"
        "Arguments: $($Command.arguments -join ' ')"
        "Working directory: $($Command.workingDirectory)"
        "Started at: $(Get-Date -Format o)"
        ""
    ) -join [Environment]::NewLine
    Write-Utf8NoBom -Path $LogPath -Value $header

    Push-Location -LiteralPath $Command.workingDirectory
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = & $Command.executable @($Command.arguments) 2>&1
        $exitCode = $LASTEXITCODE
    }
    catch {
        $output = @($_.Exception.ToString())
        $exitCode = 1
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
        Pop-Location
    }

    $body = ($output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
    [System.IO.File]::AppendAllText(
        $LogPath,
        $body + [Environment]::NewLine,
        [System.Text.UTF8Encoding]::new($false)
    )
    if ($exitCode -ne 0) {
        throw "Build command failed with exit code ${exitCode}: $($Command.display). See $LogPath"
    }
}

function Copy-PayloadFile {
    param(
        [string]$Source,
        [string]$Destination,
        [string]$RelativePath,
        [string]$Kind
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "Required Loom build input is missing: $Source"
    }
    $parent = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
    $file = Get-Item -LiteralPath $Destination
    return [ordered]@{
        kind = $Kind
        name = $file.Name
        path = $RelativePath.Replace("/", "\")
        bytes = [int64]$file.Length
        sha256 = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function Get-FrameworkPackageArtifacts {
    param([System.Collections.Specialized.OrderedDictionary]$FrameworkCatalog)

    $catalogRoot = [string]$FrameworkCatalog.outputRoot
    $summaryPath = Join-Path $catalogRoot "summary.json"
    if (-not (Test-Path -LiteralPath $summaryPath -PathType Leaf)) {
        throw "Framework package catalog summary is missing: $summaryPath"
    }
    $summary = Get-Content -Raw -Encoding UTF8 -LiteralPath $summaryPath | ConvertFrom-Json
    $expectedIds = @($FrameworkCatalog.expectedIds | ForEach-Object { [string]$_ })
    $summaryEntries = @($summary.frameworks)
    $actualIds = @($summaryEntries | ForEach-Object { [string]$_.id })
    $actualIdSet = (@($actualIds | Sort-Object) -join "`n")
    $expectedIdSet = (@($expectedIds | Sort-Object) -join "`n")
    if (-not [string]::Equals($actualIdSet, $expectedIdSet, [System.StringComparison]::Ordinal)) {
        throw "Framework package catalog ids do not match the release contract."
    }

    $packageRecords = @()
    $artifactRecords = @()
    $payloadRecords = @()
    foreach ($id in $expectedIds) {
        $entry = @($summaryEntries | Where-Object { [string]$_.id -eq $id })
        if ($entry.Count -ne 1) {
            throw "Framework package catalog must contain exactly one entry for ${id}."
        }
        $zipPath = Join-Path $catalogRoot "$id.zip"
        $sidecarPath = "$zipPath.sha256"
        foreach ($required in @($zipPath, $sidecarPath)) {
            if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
                throw "Framework package artifact is missing: $required"
            }
        }
        $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $sidecarFields = @((Get-Content -Raw -Encoding UTF8 -LiteralPath $sidecarPath).Trim() -split '\s+')
        if (
            $sidecarFields.Count -ne 2 -or
            -not [string]::Equals($sidecarFields[0], $zipHash, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not [string]::Equals($sidecarFields[1], "$id.zip", [System.StringComparison]::Ordinal)
        ) {
            throw "Framework package checksum sidecar is invalid: $sidecarPath"
        }
        if (-not [string]::Equals([string]$entry[0].sha256, $zipHash, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Framework package summary hash mismatch: $id"
        }

        $zipFile = Get-Item -LiteralPath $zipPath
        $zipRelative = "packages\frameworks\$id.zip"
        $zipRecord = [ordered]@{
            kind = "framework-package-zip"
            role = "framework"
            id = $id
            version = [string]$entry[0].version
            protocolVersion = [string]$entry[0].protocolVersion
            name = $zipFile.Name
            path = $zipRelative
            bytes = [int64]$zipFile.Length
            sha256 = $zipHash
        }
        $sidecarFile = Get-Item -LiteralPath $sidecarPath
        $sidecarRelative = "$zipRelative.sha256"
        $sidecarRecord = [ordered]@{
            kind = "framework-package-zip-sha256"
            role = "framework"
            id = $id
            name = $sidecarFile.Name
            path = $sidecarRelative
            bytes = [int64]$sidecarFile.Length
            sha256 = (Get-FileHash -LiteralPath $sidecarPath -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        $packageRecords += $zipRecord
        $artifactRecords += @($zipRecord, $sidecarRecord)
        $payloadRecords += @($zipRecord, $sidecarRecord)
    }

    $summaryFile = Get-Item -LiteralPath $summaryPath
    $summaryRecord = [ordered]@{
        kind = "framework-package-catalog"
        name = $summaryFile.Name
        path = "packages\frameworks\summary.json"
        bytes = [int64]$summaryFile.Length
        sha256 = (Get-FileHash -LiteralPath $summaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $artifactRecords += $summaryRecord
    $payloadRecords += $summaryRecord
    return [pscustomobject]@{
        packages = @($packageRecords)
        catalog = $summaryRecord
        artifacts = @($artifactRecords)
        payload = @($payloadRecords)
    }
}

function Get-SampleArtPackageArtifacts {
    param([System.Collections.Specialized.OrderedDictionary]$ArtCatalog)

    $catalogRoot = [string]$ArtCatalog.outputRoot
    $summaryPath = Join-Path $catalogRoot "summary.json"
    if (-not (Test-Path -LiteralPath $summaryPath -PathType Leaf)) {
        throw "Sample Art package catalog summary is missing: $summaryPath"
    }
    $summary = Get-Content -Raw -Encoding UTF8 -LiteralPath $summaryPath | ConvertFrom-Json
    $expectedEntries = @($ArtCatalog.expected)
    $summaryEntries = @($summary.packages)
    $expectedIds = @($expectedEntries | ForEach-Object { [string]$_.id })
    $actualIds = @($summaryEntries | ForEach-Object { [string]$_.id })
    if (-not [string]::Equals(
        (@($expectedIds | Sort-Object) -join "`n"),
        (@($actualIds | Sort-Object) -join "`n"),
        [System.StringComparison]::Ordinal
    )) {
        throw "Sample Art package catalog ids do not match the release contract."
    }

    $packageRecords = @()
    $artifactRecords = @()
    $payloadRecords = @()
    foreach ($expected in $expectedEntries) {
        $id = [string]$expected.id
        $framework = [string]$expected.framework
        $entry = @($summaryEntries | Where-Object { [string]$_.id -eq $id })
        if ($entry.Count -ne 1) {
            throw "Sample Art package catalog must contain exactly one entry for ${id}."
        }
        if (-not [string]::Equals([string]$entry[0].framework, $framework, [System.StringComparison]::Ordinal)) {
            throw "Sample Art framework mismatch in catalog: $id"
        }
        if (-not [string]::Equals([string]$entry[0].zip, "$id.zip", [System.StringComparison]::Ordinal)) {
            throw "Sample Art ZIP name mismatch in catalog: $id"
        }

        $zipPath = Join-Path $catalogRoot "$id.zip"
        $sidecarPath = "$zipPath.sha256"
        foreach ($required in @($zipPath, $sidecarPath)) {
            if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
                throw "Sample Art package artifact is missing: $required"
            }
        }
        $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $sidecarFields = @((Get-Content -Raw -Encoding UTF8 -LiteralPath $sidecarPath).Trim() -split '\s+')
        if (
            $sidecarFields.Count -ne 2 -or
            -not [string]::Equals($sidecarFields[0], $zipHash, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not [string]::Equals($sidecarFields[1], "$id.zip", [System.StringComparison]::Ordinal)
        ) {
            throw "Sample Art package checksum sidecar is invalid: $sidecarPath"
        }
        if (-not [string]::Equals([string]$entry[0].sha256, $zipHash, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Sample Art package summary hash mismatch: $id"
        }

        $zipFile = Get-Item -LiteralPath $zipPath
        $zipRelative = "packages\arts\$id.zip"
        $zipRecord = [ordered]@{
            kind = "sample-art-package-zip"
            role = "sample-art"
            id = $id
            framework = $framework
            name = $zipFile.Name
            path = $zipRelative
            bytes = [int64]$zipFile.Length
            sha256 = $zipHash
        }
        $sidecarFile = Get-Item -LiteralPath $sidecarPath
        $sidecarRecord = [ordered]@{
            kind = "sample-art-package-zip-sha256"
            role = "sample-art"
            id = $id
            name = $sidecarFile.Name
            path = "$zipRelative.sha256"
            bytes = [int64]$sidecarFile.Length
            sha256 = (Get-FileHash -LiteralPath $sidecarPath -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        $packageRecords += $zipRecord
        $artifactRecords += @($zipRecord, $sidecarRecord)
        $payloadRecords += @($zipRecord, $sidecarRecord)
    }

    $summaryFile = Get-Item -LiteralPath $summaryPath
    $summaryRecord = [ordered]@{
        kind = "sample-art-package-catalog"
        name = $summaryFile.Name
        path = "packages\arts\summary.json"
        bytes = [int64]$summaryFile.Length
        sha256 = (Get-FileHash -LiteralPath $summaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $artifactRecords += $summaryRecord
    $payloadRecords += $summaryRecord
    return [pscustomobject]@{
        packages = @($packageRecords)
        catalog = $summaryRecord
        artifacts = @($artifactRecords)
        payload = @($payloadRecords)
    }
}

function New-BuildInfo {
    param(
        [string]$ResolvedVersionId,
        [string]$ResolvedOutputRoot,
        [System.Collections.Specialized.OrderedDictionary]$Catalog,
        [string]$GitHead,
        [object]$GitDirty
    )

    $lines = @(
        "Loom Windows release artifact"
        "versionId=$ResolvedVersionId"
        "target=$targetName"
        "repository=https://github.com/aiaimimi0920/Loom"
        "sourcePaths=."
        "gitHead=$GitHead"
        "gitDirty=$GitDirty"
        "outputRoot=$ResolvedOutputRoot"
        ""
        "Commands:"
    )
    foreach ($command in $Catalog.commands) {
        $lines += "- $($command.display)"
    }
    return ($lines -join [Environment]::NewLine) + [Environment]::NewLine
}

function Get-RelativeFiles {
    param([string]$BasePath)

    $base = [System.IO.Path]::GetFullPath($BasePath).TrimEnd("\", "/")
    return @(Get-ChildItem -LiteralPath $base -Recurse -File | ForEach-Object {
        $relative = $_.FullName.Substring($base.Length + 1).Replace("\", "/")
        [ordered]@{ file = $_; relative = $relative }
    })
}

function Write-Checksums {
    param([string]$Destination)

    $checksumPath = Join-Path $Destination "checksums.sha256"
    $lines = @()
    foreach ($entry in (Get-RelativeFiles -BasePath $Destination)) {
        if ($entry.relative -ieq "checksums.sha256") {
            continue
        }
        $hash = (Get-FileHash -LiteralPath $entry.file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $lines += "$hash  $($entry.relative)"
    }
    Write-Ascii -Path $checksumPath -Value (($lines -join "`r`n") + "`r`n")
    return [ordered]@{
        path = "checksums.sha256"
        entries = $lines.Count
        sha256 = (Get-FileHash -LiteralPath $checksumPath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function New-PayloadZip {
    param(
        [string]$Destination,
        [string]$ResolvedVersionId,
        [object[]]$PayloadRecords
    )

    $packageDir = Join-Path $Destination "packages"
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    $stage = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-package-" + [Guid]::NewGuid().ToString("N"))
    try {
        New-Item -ItemType Directory -Path $stage -Force | Out-Null
        foreach ($record in $PayloadRecords) {
            $source = Join-Path $Destination ([string]$record.path)
            $target = Join-Path $stage ([string]$record.path)
            $parent = Split-Path -Parent $target
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
            Copy-Item -LiteralPath $source -Destination $target -Force
        }
        $zipName = "Loom-$ResolvedVersionId-$targetName.zip"
        $zipPath = Join-Path $packageDir $zipName
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
        $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $zipShaPath = "$zipPath.sha256"
        Write-Ascii -Path $zipShaPath -Value "$zipHash  $zipName`r`n"
        return @(
            [ordered]@{
                kind = "desktop-zip"
                role = "desktop"
                name = $zipName
                path = "packages\$zipName"
                bytes = [int64](Get-Item -LiteralPath $zipPath).Length
                sha256 = $zipHash
            }
            [ordered]@{
                kind = "zip-sha256"
                name = "$zipName.sha256"
                path = "packages\$zipName.sha256"
                bytes = [int64](Get-Item -LiteralPath $zipShaPath).Length
                sha256 = (Get-FileHash -LiteralPath $zipShaPath -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        )
    }
    finally {
        if (Test-Path -LiteralPath $stage) {
            Remove-Item -LiteralPath $stage -Recurse -Force
        }
    }
}

function New-CliZip {
    param(
        [string]$Destination,
        [string]$ResolvedVersionId,
        [object]$CliArtifact
    )

    $packageDir = Join-Path $Destination "packages"
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    $stage = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-cli-package-" + [Guid]::NewGuid().ToString("N"))
    try {
        New-Item -ItemType Directory -Path $stage -Force | Out-Null
        if (-not (Test-Path -LiteralPath $CliArtifact.source -PathType Leaf)) {
            throw "Required Loom CLI build input is missing: $($CliArtifact.source)"
        }
        Copy-Item -LiteralPath $CliArtifact.source -Destination (Join-Path $stage $CliArtifact.entryName) -Force
        $zipName = "Loom-CLI-$ResolvedVersionId-$targetName.zip"
        $zipPath = Join-Path $packageDir $zipName
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
        $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $zipShaPath = "$zipPath.sha256"
        Write-Ascii -Path $zipShaPath -Value "$zipHash  $zipName`r`n"
        return @(
            [ordered]@{
                kind = "cli-zip"
                role = "cli"
                name = $zipName
                path = "packages\$zipName"
                bytes = [int64](Get-Item -LiteralPath $zipPath).Length
                sha256 = $zipHash
            }
            [ordered]@{
                kind = "cli-zip-sha256"
                role = "cli"
                name = "$zipName.sha256"
                path = "packages\$zipName.sha256"
                bytes = [int64](Get-Item -LiteralPath $zipShaPath).Length
                sha256 = (Get-FileHash -LiteralPath $zipShaPath -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        )
    }
    finally {
        if (Test-Path -LiteralPath $stage) {
            Remove-Item -LiteralPath $stage -Recurse -Force
        }
    }
}

function New-PluginSdkZip {
    param(
        [string]$Destination,
        [string]$ResolvedVersionId,
        [object]$PluginSdkArtifact
    )

    $packageDir = Join-Path $Destination "packages"
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    $stage = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-plugin-sdk-" + [Guid]::NewGuid().ToString("N"))
    try {
        New-Item -ItemType Directory -Path $stage -Force | Out-Null
        if (-not (Test-Path -LiteralPath $PluginSdkArtifact.pluginCliSource -PathType Leaf)) {
            throw "Required Loom plugin CLI build input is missing: $($PluginSdkArtifact.pluginCliSource)"
        }
        Copy-Item -LiteralPath $PluginSdkArtifact.pluginCliSource -Destination (Join-Path $stage $PluginSdkArtifact.pluginCliEntryName) -Force
        foreach ($file in $PluginSdkArtifact.files) {
            if (-not (Test-Path -LiteralPath $file.source -PathType Leaf)) {
                throw "Required Loom plugin SDK file is missing: $($file.source)"
            }
            $target = Join-Path $stage ([string]$file.destinationRelativePath)
            $parent = Split-Path -Parent $target
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
            Copy-Item -LiteralPath $file.source -Destination $target -Force
        }
        $zipName = "Loom-Plugin-SDK-$ResolvedVersionId-$targetName.zip"
        $zipPath = Join-Path $packageDir $zipName
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
        $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $zipShaPath = "$zipPath.sha256"
        Write-Ascii -Path $zipShaPath -Value "$zipHash  $zipName`r`n"
        return @(
            [ordered]@{
                kind = "plugin-sdk-zip"
                role = "plugin-sdk"
                name = $zipName
                path = "packages\$zipName"
                bytes = [int64](Get-Item -LiteralPath $zipPath).Length
                sha256 = $zipHash
            }
            [ordered]@{
                kind = "plugin-sdk-zip-sha256"
                role = "plugin-sdk"
                name = "$zipName.sha256"
                path = "packages\$zipName.sha256"
                bytes = [int64](Get-Item -LiteralPath $zipShaPath).Length
                sha256 = (Get-FileHash -LiteralPath $zipShaPath -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        )
    }
    finally {
        if (Test-Path -LiteralPath $stage) {
            Remove-Item -LiteralPath $stage -Recurse -Force
        }
    }
}

$resolvedVersionId = Resolve-VersionId -ExplicitVersionId $VersionId
$resolvedOutputRoot = Resolve-OutputRoot -Value $OutputRoot
$destination = Join-Path $resolvedOutputRoot $resolvedVersionId
$catalog = Get-LoomCatalog `
    -FrameworkPackageOutputRoot (Join-Path $destination "packages\frameworks") `
    -SampleArtPackageOutputRoot (Join-Path $destination "packages\arts")
$sourceGitDirty = Get-GitDirty
if ($RequireCleanSource -and $sourceGitDirty -ne $false) {
    throw "Formal Loom release requires a clean, readable Git worktree. gitDirty=$sourceGitDirty"
}

if ($DryRun) {
    $plan = New-Plan -Catalog $catalog -ResolvedVersionId $resolvedVersionId -ResolvedOutputRoot $resolvedOutputRoot -Destination $destination
    Write-Output ($plan | ConvertTo-Json -Depth 20)
    exit 0
}

if (Test-Path -LiteralPath $destination) {
    throw "Release destination already exists: $destination"
}

New-Item -ItemType Directory -Path $destination -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $destination "logs") -Force | Out-Null

$commandRecords = @()
for ($index = 0; $index -lt @($catalog.commands).Count; $index++) {
    $command = $catalog.commands[$index]
    $logPath = Join-Path $destination (Join-Path "logs" $command.logName)
    Invoke-CommandToLog -Command $command -LogPath $logPath
    $commandRecords += [ordered]@{
        display = $command.display
        workingDirectory = Get-RepoRelativeOrExternal -Path $command.workingDirectory
        logPath = "logs\$($command.logName)"
    }
}

$exeRecords = @()
foreach ($exe in $catalog.exes) {
    $relative = [string]$exe.destinationRelativePath
    $destinationPath = Join-Path $destination $relative
    $exeRecords += Copy-PayloadFile `
        -Source $exe.source `
        -Destination $destinationPath `
        -RelativePath $relative `
        -Kind "exe"
}

$supportRecords = @()
foreach ($support in $catalog.supportFiles) {
    $relative = [string]$support.destinationRelativePath
    $destinationPath = Join-Path $destination $relative
    $supportRecords += Copy-PayloadFile `
        -Source $support.source `
        -Destination $destinationPath `
        -RelativePath $relative `
        -Kind "support-file"
}

$frameworkArtifacts = Get-FrameworkPackageArtifacts -FrameworkCatalog $catalog.frameworkPackageCatalog
$frameworkPackageRecords = @($frameworkArtifacts.packages)
$frameworkCatalogRecord = $frameworkArtifacts.catalog
$frameworkArtifactRecords = @($frameworkArtifacts.artifacts)
$frameworkPayloadRecords = @($frameworkArtifacts.payload)
$sampleArtArtifacts = Get-SampleArtPackageArtifacts -ArtCatalog $catalog.sampleArtPackageCatalog
$sampleArtPackageRecords = @($sampleArtArtifacts.packages)
$sampleArtCatalogRecord = $sampleArtArtifacts.catalog
$sampleArtArtifactRecords = @($sampleArtArtifacts.artifacts)
$sampleArtPayloadRecords = @($sampleArtArtifacts.payload)

$gitHead = Get-GitText -Arguments @("rev-parse", "HEAD")
if ([string]::IsNullOrWhiteSpace($gitHead)) {
    $gitHead = "unknown"
}
$gitShortSha = Get-GitText -Arguments @("rev-parse", "--short=8", "HEAD")
if ([string]::IsNullOrWhiteSpace($gitShortSha)) {
    $gitShortSha = "nogit"
}
$gitDirty = $sourceGitDirty

$buildInfoPath = Join-Path $destination "BUILD_INFO.txt"
Write-Utf8NoBom -Path $buildInfoPath -Value (New-BuildInfo `
    -ResolvedVersionId $resolvedVersionId `
    -ResolvedOutputRoot (Get-RepoRelativeOrExternal -Path $resolvedOutputRoot) `
    -Catalog $catalog `
    -GitHead $gitHead `
    -GitDirty $gitDirty)
$buildInfo = [ordered]@{
    kind = "build-info"
    name = "BUILD_INFO.txt"
    path = "BUILD_INFO.txt"
    bytes = [int64](Get-Item -LiteralPath $buildInfoPath).Length
    sha256 = (Get-FileHash -LiteralPath $buildInfoPath -Algorithm SHA256).Hash.ToLowerInvariant()
}

$payloadRecords = @($exeRecords + $supportRecords + $frameworkPayloadRecords + $sampleArtPayloadRecords)
$artifactRecords = @($frameworkArtifactRecords + $sampleArtArtifactRecords)
$cliArtifactManifest = $null
$pluginSdkArtifactManifest = $null
if (-not $NoZip) {
    $desktopArtifactRecords = @(New-PayloadZip -Destination $destination -ResolvedVersionId $resolvedVersionId -PayloadRecords $payloadRecords)
    $cliArtifactRecords = @(New-CliZip -Destination $destination -ResolvedVersionId $resolvedVersionId -CliArtifact $catalog.cliArtifact)
    $pluginSdkArtifactRecords = @(New-PluginSdkZip -Destination $destination -ResolvedVersionId $resolvedVersionId -PluginSdkArtifact $catalog.pluginSdkArtifact)
    $artifactRecords = @(
        $desktopArtifactRecords +
        $cliArtifactRecords +
        $pluginSdkArtifactRecords +
        $frameworkArtifactRecords +
        $sampleArtArtifactRecords
    )
    $cliZipRecord = @($cliArtifactRecords | Where-Object { [string]$_.kind -eq "cli-zip" })[0]
    $cliArtifactManifest = [ordered]@{
        name = $catalog.cliArtifact.name
        entryName = $catalog.cliArtifact.entryName
        zipName = $cliZipRecord.name
        path = $cliZipRecord.path
        bytes = $cliZipRecord.bytes
        sha256 = $cliZipRecord.sha256
    }
    $pluginSdkZipRecord = @($pluginSdkArtifactRecords | Where-Object { [string]$_.kind -eq "plugin-sdk-zip" })[0]
    $pluginSdkArtifactManifest = [ordered]@{
        name = $catalog.pluginSdkArtifact.name
        entryName = $catalog.pluginSdkArtifact.pluginCliEntryName
        zipName = $pluginSdkZipRecord.name
        path = $pluginSdkZipRecord.path
        bytes = $pluginSdkZipRecord.bytes
        sha256 = $pluginSdkZipRecord.sha256
        protocolVersion = "loom.framework.v1"
        schemaCount = 5
    }
}

$sbomDir = Join-Path $destination "sbom"
$sbomOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "New-LoomSbom.ps1") `
    -OutputDirectory $sbomDir `
    -Version $resolvedVersionId
if ($LASTEXITCODE -ne 0) {
    throw "SBOM generation failed: $($sbomOutput -join [Environment]::NewLine)"
}
$sbomRecords = @(Get-ChildItem -LiteralPath $sbomDir -File | Sort-Object Name | ForEach-Object {
    [ordered]@{
        kind = "sbom"
        name = $_.Name
        path = "sbom\$($_.Name)"
        bytes = [int64]$_.Length
        sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
})

$provenanceDir = Join-Path $destination "provenance"
$provenancePath = Join-Path $provenanceDir "build-provenance.json"
$provenance = [ordered]@{
    schemaVersion = 1
    builder = "Loom scripts/build-release.ps1"
    versionId = $resolvedVersionId
    target = $targetName
    gitHead = $gitHead
    gitDirty = $gitDirty
    sourcePaths = @(".")
    commands = @($commandRecords)
    subjects = @($artifactRecords | Where-Object { ([string]$_.kind).EndsWith("-zip") } | ForEach-Object {
        [ordered]@{ name = $_.name; sha256 = $_.sha256; bytes = $_.bytes }
    })
}
Write-Utf8NoBom -Path $provenancePath -Value (($provenance | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
$provenanceRecord = [ordered]@{
    kind = "provenance"
    name = "build-provenance.json"
    path = "provenance\build-provenance.json"
    bytes = [int64](Get-Item -LiteralPath $provenancePath).Length
    sha256 = (Get-FileHash -LiteralPath $provenancePath -Algorithm SHA256).Hash.ToLowerInvariant()
}

$manifest = [ordered]@{
    schemaVersion = 2
    app = "Loom"
    sourceProject = "Loom"
    versionId = $resolvedVersionId
    builtAt = (Get-Date).ToString("o")
    gitHead = $gitHead
    gitShortSha = $gitShortSha
    gitDirty = $gitDirty
    profile = "release"
    target = $targetName
    repoRoot = "."
    outputRoot = Get-RepoRelativeOrExternal -Path $resolvedOutputRoot
    destination = Get-RepoRelativeOrExternal -Path $destination
    commands = $commandRecords
    exes = $exeRecords
    supportFiles = $supportRecords
    cliArtifact = $cliArtifactManifest
    pluginSdkArtifact = $pluginSdkArtifactManifest
    frameworkPackages = $frameworkPackageRecords
    frameworkCatalog = $frameworkCatalogRecord
    sampleArtPackages = $sampleArtPackageRecords
    sampleArtCatalog = $sampleArtCatalogRecord
    sbom = $sbomRecords
    provenance = $provenanceRecord
    buildInfo = $buildInfo
    artifacts = $artifactRecords
    checksums = "checksums.sha256"
    sourceGitDirty = $gitDirty
    sourcePaths = @(".")
}
$manifestPath = Join-Path $destination "manifest.json"
Write-Utf8NoBom -Path $manifestPath -Value (($manifest | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
$checksumRecord = Write-Checksums -Destination $destination

$result = [ordered]@{
    schemaVersion = 1
    mode = "build"
    app = "Loom"
    versionId = $resolvedVersionId
    outputRoot = $resolvedOutputRoot
    destination = $destination
    manifest = $manifestPath
    checksums = (Join-Path $destination "checksums.sha256")
    checksumEntries = $checksumRecord.entries
    zip = (-not $NoZip)
    exes = $exeRecords
    supportFiles = $supportRecords
    cliArtifact = $cliArtifactManifest
    pluginSdkArtifact = $pluginSdkArtifactManifest
    frameworkPackages = $frameworkPackageRecords
    frameworkCatalog = $frameworkCatalogRecord
    sampleArtPackages = $sampleArtPackageRecords
    sampleArtCatalog = $sampleArtCatalogRecord
    sbom = $sbomRecords
    provenance = $provenanceRecord
    artifacts = $artifactRecords
}
Write-Output ($result | ConvertTo-Json -Depth 20)
