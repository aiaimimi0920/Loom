Set-StrictMode -Version Latest

function Resolve-LoomPackageRelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$PackageDir,
        [Parameter(Mandatory = $true)][string]$RelativePath
    )

    if ([System.IO.Path]::IsPathRooted($RelativePath) -or $RelativePath.Contains("..")) {
        throw "Invalid Loom package-relative path: $RelativePath"
    }
    return [System.IO.Path]::GetFullPath((Join-Path $PackageDir $RelativePath))
}

function Get-LoomReleaseLayout {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$PackageDir,
        [string]$CliExtractRoot = ""
    )

    $packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
    if (-not (Test-Path -LiteralPath $packageFullPath -PathType Container)) {
        throw "Loom package directory is missing: $packageFullPath"
    }

    $manifestPath = Join-Path $packageFullPath "manifest.json"
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "Loom package manifest is missing: $manifestPath"
    }
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json

    $desktopExe = Join-Path $packageFullPath "Loom.exe"
    $runtimeRoot = Join-Path $packageFullPath "runtime"
    $daemonExe = Join-Path $runtimeRoot "loom-daemon.exe"
    foreach ($requiredPath in @($desktopExe, $daemonExe)) {
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw "Loom package executable is missing: $requiredPath"
        }
    }

    $cliArtifact = $manifest.PSObject.Properties["cliArtifact"]
    if ($null -eq $cliArtifact -or $null -eq $cliArtifact.Value) {
        throw "Loom package manifest is missing cliArtifact."
    }
    $cliEntryName = [string]$cliArtifact.Value.entryName
    if ($cliEntryName -ne "loom.exe") {
        throw "Loom CLI artifact entry must be loom.exe. Actual: $cliEntryName"
    }
    $cliZipName = [string]$cliArtifact.Value.zipName
    if (-not $cliZipName.StartsWith("Loom-CLI-")) {
        throw "Loom CLI artifact must use the Loom-CLI- naming contract. Actual: $cliZipName"
    }
    $cliZip = Resolve-LoomPackageRelativePath -PackageDir $packageFullPath -RelativePath ([string]$cliArtifact.Value.path)
    if (-not (Test-Path -LiteralPath $cliZip -PathType Leaf)) {
        throw "Loom CLI artifact is missing: $cliZip"
    }

    $cliExe = $null
    if (-not [string]::IsNullOrWhiteSpace($CliExtractRoot)) {
        $cliExtractFullPath = [System.IO.Path]::GetFullPath($CliExtractRoot)
        New-Item -ItemType Directory -Path $cliExtractFullPath -Force | Out-Null
        Expand-Archive -LiteralPath $cliZip -DestinationPath $cliExtractFullPath -Force
        $cliExe = Join-Path $cliExtractFullPath $cliEntryName
        if (-not (Test-Path -LiteralPath $cliExe -PathType Leaf)) {
            throw "Expanded Loom CLI executable is missing: $cliExe"
        }
    }

    return [pscustomobject]@{
        packageDir = $packageFullPath
        manifest = $manifest
        manifestPath = $manifestPath
        desktopExe = $desktopExe
        runtimeRoot = $runtimeRoot
        daemonExe = $daemonExe
        cliZip = $cliZip
        cliExe = $cliExe
    }
}
