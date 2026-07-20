[CmdletBinding()]
param(
    [string]$VersionId = "",
    [string]$OutputRoot = ".\release\Loom",
    [switch]$NoZip,
    [switch]$DryRun
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
    $ocrRoot = Join-Path $repoRoot "resources\ocr"
    $pythonEmbedRoot = Join-Path $repoRoot "resources\python-embed"
    $pythonRoot = Join-Path $repoRoot "resources\python"

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
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "python.exe") -DestinationRelativePath "runtime\bin\python-embed\python.exe"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "pythonw.exe") -DestinationRelativePath "runtime\bin\python-embed\pythonw.exe"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "python3.dll") -DestinationRelativePath "runtime\bin\python-embed\python3.dll"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "python312.dll") -DestinationRelativePath "runtime\bin\python-embed\python312.dll"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "python312.zip") -DestinationRelativePath "runtime\bin\python-embed\python312.zip"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "python312._pth") -DestinationRelativePath "runtime\bin\python-embed\python312._pth"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "LICENSE.txt") -DestinationRelativePath "runtime\bin\python-embed\LICENSE.txt"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "vcruntime140.dll") -DestinationRelativePath "runtime\bin\python-embed\vcruntime140.dll"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "vcruntime140_1.dll") -DestinationRelativePath "runtime\bin\python-embed\vcruntime140_1.dll"
        New-SupportSpec -Source (Join-Path $pythonEmbedRoot "site-packages\.loom-keep") -DestinationRelativePath "runtime\bin\python-embed\site-packages\.loom-keep"
        New-SupportSpec -Source (Join-Path $pythonRoot "Launcher.py") -DestinationRelativePath "runtime\python\Launcher.py"
        New-SupportSpec -Source (Join-Path $pythonRoot "Arts\Art_LoomEcho\art.json") -DestinationRelativePath "runtime\python\Arts\Art_LoomEcho\art.json"
        New-SupportSpec -Source (Join-Path $pythonRoot "Arts\Art_LoomEcho\main.py") -DestinationRelativePath "runtime\python\Arts\Art_LoomEcho\main.py"
    )

    $cliArtifact = [ordered]@{
        name = "loom-cli"
        source = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "target\release\loom.exe"))
        entryName = "loom.exe"
        zipNamePattern = "Loom-CLI-{versionId}-windows-x64.zip"
    }

    return [ordered]@{
        app = "Loom"
        sourceProject = "Loom"
        sourcePaths = @(".")
        exes = $exes
        supportFiles = $support
        cliArtifact = $cliArtifact
        commands = @(
            New-CommandSpec -Executable "cargo" `
                -Arguments @("build", "--locked", "--release", "-p", "loom-daemon", "-p", "loom-cli") `
                -WorkingDirectory $repoRoot `
                -Display "cargo build --locked --release -p loom-daemon -p loom-cli" `
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

$resolvedVersionId = Resolve-VersionId -ExplicitVersionId $VersionId
$resolvedOutputRoot = Resolve-OutputRoot -Value $OutputRoot
$destination = Join-Path $resolvedOutputRoot $resolvedVersionId
$catalog = Get-LoomCatalog

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

$gitHead = Get-GitText -Arguments @("rev-parse", "HEAD")
if ([string]::IsNullOrWhiteSpace($gitHead)) {
    $gitHead = "unknown"
}
$gitShortSha = Get-GitText -Arguments @("rev-parse", "--short=8", "HEAD")
if ([string]::IsNullOrWhiteSpace($gitShortSha)) {
    $gitShortSha = "nogit"
}
$gitDirty = Get-GitDirty

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

$payloadRecords = @($exeRecords + $supportRecords)
$artifactRecords = @()
$cliArtifactManifest = $null
if (-not $NoZip) {
    $desktopArtifactRecords = @(New-PayloadZip -Destination $destination -ResolvedVersionId $resolvedVersionId -PayloadRecords $payloadRecords)
    $cliArtifactRecords = @(New-CliZip -Destination $destination -ResolvedVersionId $resolvedVersionId -CliArtifact $catalog.cliArtifact)
    $artifactRecords = @($desktopArtifactRecords + $cliArtifactRecords)
    $cliZipRecord = @($cliArtifactRecords | Where-Object { [string]$_.kind -eq "cli-zip" })[0]
    $cliArtifactManifest = [ordered]@{
        name = $catalog.cliArtifact.name
        entryName = $catalog.cliArtifact.entryName
        zipName = $cliZipRecord.name
        path = $cliZipRecord.path
        bytes = $cliZipRecord.bytes
        sha256 = $cliZipRecord.sha256
    }
}

$manifest = [ordered]@{
    schemaVersion = 1
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
    artifacts = $artifactRecords
}
Write-Output ($result | ConvertTo-Json -Depth 20)
