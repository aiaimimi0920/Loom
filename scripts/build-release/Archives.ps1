<# Owns one release-script responsibility. #>

function New-LoomArchiveStage {
    param([Parameter(Mandatory = $true)][string]$Prefix)

    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd("\", "/")
    Assert-LoomPathHasNoReparsePoints -RootPath $tempRoot -Path $tempRoot
    $stage = Join-Path $tempRoot ($Prefix + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $stage | Out-Null
    Assert-LoomPathHasNoReparsePoints -RootPath $tempRoot -Path $stage
    return $stage
}

function Remove-LoomArchiveStage {
    param([Parameter(Mandatory = $true)][string]$Stage)

    if (Test-Path -LiteralPath $Stage) {
        $stageFullPath = [System.IO.Path]::GetFullPath($Stage)
        Assert-LoomPathHasNoReparsePoints -RootPath $stageFullPath -Path $stageFullPath
        $pending = [System.Collections.Generic.Stack[string]]::new()
        $directories = [System.Collections.Generic.List[string]]::new()
        $pending.Push($stageFullPath)
        while ($pending.Count -gt 0) {
            $directory = $pending.Pop()
            $directories.Add($directory)
            foreach ($item in @(Get-ChildItem -LiteralPath $directory -Force)) {
                if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                    throw "Loom release paths must not contain reparse points: $($item.FullName)"
                }
                if ($item.PSIsContainer) {
                    $pending.Push($item.FullName)
                }
                else {
                    Assert-LoomPathHasNoReparsePoints -RootPath $stageFullPath -Path $item.FullName
                    ([System.IO.FileInfo]$item).Delete()
                }
            }
        }
        foreach ($directory in @($directories | Sort-Object { $_.Length } -Descending)) {
            Assert-LoomPathHasNoReparsePoints -RootPath $stageFullPath -Path $directory
            [System.IO.Directory]::Delete($directory, $false)
        }
    }
}

function Copy-LoomArchiveInput {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Stage,
        [Parameter(Mandatory = $true)][string]$RelativePath
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "Required Loom archive input is missing: $Source"
    }
    $target = Resolve-LoomPackageRelativePath -PackageDir $Stage -RelativePath $RelativePath
    $parent = Split-Path -Parent $target
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    Assert-LoomPathHasNoReparsePoints -RootPath $Stage -Path $parent
    Copy-LoomLockedFile -Source $Source -Destination $target
    Assert-LoomPathHasNoReparsePoints -RootPath $Stage -Path $target
}

function New-PayloadZip {
    param(
        [string]$Destination,
        [string]$ResolvedVersionId,
        [object[]]$PayloadRecords
    )

    $packageDir = Resolve-LoomPackageRelativePath -PackageDir $Destination -RelativePath "packages"
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    Assert-LoomPathHasNoReparsePoints -RootPath $Destination -Path $packageDir
    $stage = New-LoomArchiveStage -Prefix "loom-package-"
    try {
        foreach ($record in $PayloadRecords) {
            $relativePath = Assert-LoomSafeRelativePath -RelativePath ([string]$record.path)
            $source = Resolve-LoomPackageRelativePath -PackageDir $Destination -RelativePath $relativePath
            Assert-LoomPathHasNoReparsePoints -RootPath $Destination -Path $source
            Copy-LoomArchiveInput -Source $source -Stage $stage -RelativePath $relativePath
        }
        $zipName = "Loom-$ResolvedVersionId-$targetName.zip"
        $zipPath = Resolve-LoomPackageRelativePath -PackageDir $packageDir -RelativePath $zipName
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
        $zipDigest = Get-LoomFileDigest -Path $zipPath
        $zipHash = $zipDigest.sha256
        $zipShaPath = "$zipPath.sha256"
        Write-Ascii -Path $zipShaPath -Value "$zipHash  $zipName`r`n"
        $sidecarDigest = Get-LoomFileDigest -Path $zipShaPath
        return @(
            [ordered]@{
                kind = "desktop-zip"
                role = "desktop"
                name = $zipName
                path = "packages\$zipName"
                bytes = [int64]$zipDigest.bytes
                sha256 = $zipHash
            }
            [ordered]@{
                kind = "zip-sha256"
                name = "$zipName.sha256"
                path = "packages\$zipName.sha256"
                bytes = [int64]$sidecarDigest.bytes
                sha256 = $sidecarDigest.sha256
            }
        )
    }
    finally {
        Remove-LoomArchiveStage -Stage $stage
    }
}

function New-CliZip {
    param(
        [string]$Destination,
        [string]$ResolvedVersionId,
        [object]$CliArtifact
    )

    $packageDir = Resolve-LoomPackageRelativePath -PackageDir $Destination -RelativePath "packages"
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    Assert-LoomPathHasNoReparsePoints -RootPath $Destination -Path $packageDir
    $stage = New-LoomArchiveStage -Prefix "loom-cli-package-"
    try {
        if (-not (Test-Path -LiteralPath $CliArtifact.source -PathType Leaf)) {
            throw "Required Loom CLI build input is missing: $($CliArtifact.source)"
        }
        Copy-LoomArchiveInput -Source $CliArtifact.source -Stage $stage -RelativePath ([string]$CliArtifact.entryName)
        $zipName = "Loom-CLI-$ResolvedVersionId-$targetName.zip"
        $zipPath = Resolve-LoomPackageRelativePath -PackageDir $packageDir -RelativePath $zipName
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
        $zipDigest = Get-LoomFileDigest -Path $zipPath
        $zipHash = $zipDigest.sha256
        $zipShaPath = "$zipPath.sha256"
        Write-Ascii -Path $zipShaPath -Value "$zipHash  $zipName`r`n"
        $sidecarDigest = Get-LoomFileDigest -Path $zipShaPath
        return @(
            [ordered]@{
                kind = "cli-zip"
                role = "cli"
                name = $zipName
                path = "packages\$zipName"
                bytes = [int64]$zipDigest.bytes
                sha256 = $zipHash
            }
            [ordered]@{
                kind = "cli-zip-sha256"
                role = "cli"
                name = "$zipName.sha256"
                path = "packages\$zipName.sha256"
                bytes = [int64]$sidecarDigest.bytes
                sha256 = $sidecarDigest.sha256
            }
        )
    }
    finally {
        Remove-LoomArchiveStage -Stage $stage
    }
}

function New-PluginSdkZip {
    param(
        [string]$Destination,
        [string]$ResolvedVersionId,
        [object]$PluginSdkArtifact
    )

    $packageDir = Resolve-LoomPackageRelativePath -PackageDir $Destination -RelativePath "packages"
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    Assert-LoomPathHasNoReparsePoints -RootPath $Destination -Path $packageDir
    $stage = New-LoomArchiveStage -Prefix "loom-plugin-sdk-"
    try {
        if (-not (Test-Path -LiteralPath $PluginSdkArtifact.pluginCliSource -PathType Leaf)) {
            throw "Required Loom plugin CLI build input is missing: $($PluginSdkArtifact.pluginCliSource)"
        }
        Copy-LoomArchiveInput -Source $PluginSdkArtifact.pluginCliSource -Stage $stage -RelativePath ([string]$PluginSdkArtifact.pluginCliEntryName)
        foreach ($file in $PluginSdkArtifact.files) {
            if (-not (Test-Path -LiteralPath $file.source -PathType Leaf)) {
                throw "Required Loom plugin SDK file is missing: $($file.source)"
            }
            Copy-LoomArchiveInput -Source $file.source -Stage $stage -RelativePath ([string]$file.destinationRelativePath)
        }
        $zipName = "Loom-Plugin-SDK-$ResolvedVersionId-$targetName.zip"
        $zipPath = Resolve-LoomPackageRelativePath -PackageDir $packageDir -RelativePath $zipName
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal
        $zipDigest = Get-LoomFileDigest -Path $zipPath
        $zipHash = $zipDigest.sha256
        $zipShaPath = "$zipPath.sha256"
        Write-Ascii -Path $zipShaPath -Value "$zipHash  $zipName`r`n"
        $sidecarDigest = Get-LoomFileDigest -Path $zipShaPath
        return @(
            [ordered]@{
                kind = "plugin-sdk-zip"
                role = "plugin-sdk"
                name = $zipName
                path = "packages\$zipName"
                bytes = [int64]$zipDigest.bytes
                sha256 = $zipHash
            }
            [ordered]@{
                kind = "plugin-sdk-zip-sha256"
                role = "plugin-sdk"
                name = "$zipName.sha256"
                path = "packages\$zipName.sha256"
                bytes = [int64]$sidecarDigest.bytes
                sha256 = $sidecarDigest.sha256
            }
        )
    }
    finally {
        Remove-LoomArchiveStage -Stage $stage
    }
}
