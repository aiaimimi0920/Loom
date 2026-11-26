[CmdletBinding()]
param(
    [ValidateSet("api", "web")]
    [string]$Edition,
    [string]$VersionId = "",
    [string]$OutputRoot = ".\release\Loom",
    [switch]$NoZip,
    [switch]$DryRun,
    [switch]$RequireCleanSource,
    [switch]$DevelopmentOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$layoutPath = Join-Path $repoRoot "scripts\LoomReleaseLayout.ps1"
. $layoutPath
$moduleRoot = Join-Path $PSScriptRoot "build-release"
. (Join-Path $moduleRoot "Common.ps1")
. (Join-Path $moduleRoot "Execution.ps1")

$resolvedVersionId = Resolve-VersionId -ExplicitVersionId $VersionId
if ($DevelopmentOnly -and $resolvedVersionId -notmatch '^internal-') {
    throw 'Development-only packages require an internal- VersionId.'
}
$resolvedOutputRoot = Resolve-OutputRoot -Value $OutputRoot
Assert-LoomBuildOutputRoot -OutputRoot $resolvedOutputRoot
$editionDirName = "$resolvedVersionId-$Edition"
$destination = Resolve-LoomPackageRelativePath -PackageDir $resolvedOutputRoot -RelativePath $editionDirName

$sourceGitDirty = Get-GitDirty
if ($null -eq $sourceGitDirty) {
    throw "Loom server edition build requires a readable Git worktree state."
}
if ($RequireCleanSource -and $sourceGitDirty -ne $false) {
    throw "Formal Loom server release requires a clean Git worktree. gitDirty=$sourceGitDirty"
}

$nativeTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repoRoot "target" }
$isWin = ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT)
$daemonExt = if ($isWin) { ".exe" } else { "" }
$targetName = if ($isWin) { "windows-x64" } else { "linux-x64" }

$commands = @()
if ($Edition -eq "web") {
    $npmExe = if ($daemonExt -eq ".exe") { "cmd.exe" } else { "npm" }
    $npmArgs = if ($daemonExt -eq ".exe") { @("/d", "/c", "npm run build:web") } else { @("run", "build:web") }
    $commands += New-CommandSpec `
        -Executable $npmExe `
        -Arguments $npmArgs `
        -WorkingDirectory (Join-Path $repoRoot "apps\desktop") `
        -Display "npm run build:web" `
        -LogName "build-web-ui.log"
    $commands += New-CommandSpec `
        -Executable "cargo" `
        -Arguments @("build", "--locked", "--release", "--no-default-features", "--features", "runtime-defaults,management-ui", "-p", "loom-daemon") `
        -WorkingDirectory $repoRoot `
        -Display "cargo build --locked --release --no-default-features --features runtime-defaults,management-ui -p loom-daemon" `
        -LogName "build-daemon-web.log"
} else {
    $commands += New-CommandSpec `
        -Executable "cargo" `
        -Arguments @("build", "--locked", "--release", "--no-default-features", "--features", "runtime-defaults", "-p", "loom-daemon") `
        -WorkingDirectory $repoRoot `
        -Display "cargo build --locked --release --no-default-features --features runtime-defaults -p loom-daemon" `
        -LogName "build-daemon-api.log"
}
$commands += New-CommandSpec `
    -Executable "cargo" `
    -Arguments @("build", "--locked", "--release", "-p", "loom-cli") `
    -WorkingDirectory $repoRoot `
    -Display "cargo build --locked --release -p loom-cli" `
    -LogName "build-cli.log"

if ($DryRun) {
    $plan = [ordered]@{
        schemaVersion = 1
        mode = "dry-run"
        app = "Loom"
        edition = $Edition
        versionId = $resolvedVersionId
        target = $targetName
        destination = $destination
        commands = @($commands | ForEach-Object { $_.display })
    }
    Write-Output ($plan | ConvertTo-Json -Depth 10)
    exit 0
}

if (Test-Path -LiteralPath $destination) {
    throw "Server edition destination already exists: $destination"
}
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$logRoot = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "logs"
New-Item -ItemType Directory -Path $logRoot | Out-Null

$prevRequirePrebuilt = $env:LOOM_REQUIRE_PREBUILT_WEB_UI
try {
    if ($Edition -eq "web") {
        $env:LOOM_REQUIRE_PREBUILT_WEB_UI = "1"
    }
    $commandRecords = @()
    foreach ($cmd in $commands) {
        $logPath = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath (Join-Path "logs" $cmd.logName)
        Invoke-CommandToLog -Command $cmd -LogPath $logPath
        $commandRecords += [ordered]@{
            display = $cmd.display
            workingDirectory = Get-RepoRelativeOrExternal -Path $cmd.workingDirectory
            logPath = "logs\$($cmd.logName)"
        }
    }
} finally {
    if ($null -eq $prevRequirePrebuilt) {
        Remove-Item Env:LOOM_REQUIRE_PREBUILT_WEB_UI -ErrorAction SilentlyContinue
    } else {
        $env:LOOM_REQUIRE_PREBUILT_WEB_UI = $prevRequirePrebuilt
    }
}

$daemonName = "loom-daemon$daemonExt"
$cliName = "loom$daemonExt"
$exeRecords = @(
    Copy-PayloadFile `
        -PackageRoot $destination `
        -Source (Join-Path $nativeTarget "release\$daemonName") `
        -Destination (Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath $daemonName) `
        -RelativePath $daemonName `
        -Kind "exe"
    Copy-PayloadFile `
        -PackageRoot $destination `
        -Source (Join-Path $nativeTarget "release\$cliName") `
        -Destination (Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath $cliName) `
        -RelativePath $cliName `
        -Kind "exe"
)

$gitHead = Get-GitText -Arguments @("rev-parse", "HEAD")
$manifest = [ordered]@{
    schemaVersion = 1
    app = "Loom"
    edition = $Edition
    versionId = $resolvedVersionId
    target = $targetName
    gitHead = $gitHead
    gitDirty = $sourceGitDirty
    managementUiCompiled = ($Edition -eq "web")
    exes = $exeRecords
    commands = $commandRecords
}
$manifestPath = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "manifest.json"
Write-Utf8NoBom -Path $manifestPath -Value (($manifest | ConvertTo-Json -Depth 15) + [Environment]::NewLine)

$checksumLines = @()
foreach ($file in (Get-LoomSafeDescendantFiles -RootPath $destination | Sort-Object FullName)) {
    $rel = $file.FullName.Substring($destination.Length).TrimStart('\', '/').Replace('\', '/')
    if ($rel -eq "checksums.sha256") { continue }
    $digest = Get-LoomFileDigest -Path $file.FullName
    $checksumLines += "$($digest.sha256)  $rel"
}
$checksumPath = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "checksums.sha256"
Write-Utf8NoBom -Path $checksumPath -Value (($checksumLines -join "`n") + "`n")

Write-Output ([ordered]@{
    schemaVersion = 1
    mode = "build"
    app = "Loom"
    edition = $Edition
    versionId = $resolvedVersionId
    destination = $destination
    manifest = $manifestPath
    checksums = $checksumPath
    exes = $exeRecords
} | ConvertTo-Json -Depth 10)
