<# Owns one release-script responsibility. #>

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
        $output = @(& git -C $repoRoot status --porcelain --untracked-files=all 2>$null | Select-Object -First 1)
        if ($LASTEXITCODE -ne 0) {
            return $null
        }
        return ($output.Count -gt 0 -and -not [string]::IsNullOrWhiteSpace($output[0].ToString()))
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

function Assert-LoomBuildOutputRoot {
    param([Parameter(Mandatory = $true)][string]$OutputRoot)

    $fullPath = [System.IO.Path]::GetFullPath($OutputRoot).TrimEnd("\", "/")
    if (Test-Path -LiteralPath $fullPath -PathType Leaf) {
        throw "Release output root must be a directory: $fullPath"
    }

    $repository = $repoRoot.TrimEnd("\", "/")
    $repositoryPrefix = $repository + [System.IO.Path]::DirectorySeparatorChar
    if (
        [string]::Equals($fullPath, $repository, [System.StringComparison]::OrdinalIgnoreCase) -or
        $fullPath.StartsWith($repositoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)
    ) {
        $boundary = $repository
    }
    else {
        $boundary = Split-Path -Parent $fullPath
        while (-not [string]::IsNullOrWhiteSpace($boundary) -and -not (Test-Path -LiteralPath $boundary -PathType Container)) {
            $parent = Split-Path -Parent $boundary
            if ([string]::Equals($parent, $boundary, [System.StringComparison]::OrdinalIgnoreCase)) {
                break
            }
            $boundary = $parent
        }
        if ([string]::IsNullOrWhiteSpace($boundary) -or -not (Test-Path -LiteralPath $boundary -PathType Container)) {
            throw "Release output root has no readable directory boundary: $fullPath"
        }
    }
    Assert-LoomPathHasNoReparsePoints -RootPath $boundary -Path $fullPath
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

    $safeLogName = Assert-LoomSafeRelativePath -RelativePath $LogName
    return [ordered]@{
        executable = $Executable
        arguments = @($Arguments)
        workingDirectory = [System.IO.Path]::GetFullPath($WorkingDirectory)
        display = $Display
        logName = $safeLogName
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
    $DestinationRelativePath = Assert-LoomSafeRelativePath -RelativePath $DestinationRelativePath

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

    $DestinationRelativePath = Assert-LoomSafeRelativePath -RelativePath $DestinationRelativePath
    return [ordered]@{
        source = [System.IO.Path]::GetFullPath($Source)
        destinationRelativePath = $DestinationRelativePath
    }
}
