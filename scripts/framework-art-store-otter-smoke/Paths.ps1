# Real-file and real-directory guards for caller-selected smoke paths.
function Test-SmokeReparsePoint {
    param([System.IO.FileSystemInfo]$Item)

    return (($Item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)
}

function Resolve-SmokeRealDirectory {
    param(
        [string]$Path,
        [string]$Label
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if (-not (Test-Path -LiteralPath $fullPath -PathType Container)) {
        throw "$Label directory does not exist: $fullPath"
    }
    $item = Get-Item -LiteralPath $fullPath -Force
    if (Test-SmokeReparsePoint -Item $item) {
        throw "$Label directory must not be a reparse point: $fullPath"
    }
    return $item.FullName
}

function Initialize-SmokeRealDirectory {
    param(
        [string]$Path,
        [string]$Label
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if (-not (Test-Path -LiteralPath $fullPath)) {
        $probe = Split-Path -Parent $fullPath
        while (-not [string]::IsNullOrWhiteSpace($probe) -and -not (Test-Path -LiteralPath $probe)) {
            $probe = Split-Path -Parent $probe
        }
        if ([string]::IsNullOrWhiteSpace($probe)) {
            throw "$Label has no existing parent directory: $fullPath"
        }
        [void](Resolve-SmokeRealDirectory -Path $probe -Label "$Label parent")
        New-Item -ItemType Directory -Path $fullPath -Force | Out-Null
    }
    return Resolve-SmokeRealDirectory -Path $fullPath -Label $Label
}

function Resolve-SmokeRealFile {
    param(
        [string]$Path,
        [string]$Label
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
        throw "$Label file does not exist: $fullPath"
    }
    $item = Get-Item -LiteralPath $fullPath -Force
    if (Test-SmokeReparsePoint -Item $item) {
        throw "$Label file must not be a reparse point: $fullPath"
    }
    return $item.FullName
}

function Assert-SmokePathInsideRoot {
    param(
        [string]$Root,
        [string]$Path,
        [string]$Label
    )

    $trimCharacters = [char[]]@(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    )
    $rootFullPath = [System.IO.Path]::GetFullPath($Root).TrimEnd($trimCharacters)
    $candidateFullPath = [System.IO.Path]::GetFullPath($Path)
    $prefix = $rootFullPath + [System.IO.Path]::DirectorySeparatorChar
    if (
        -not [string]::Equals($candidateFullPath, $rootFullPath, [System.StringComparison]::OrdinalIgnoreCase) -and
        -not $candidateFullPath.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)
    ) {
        throw "$Label must remain inside ${rootFullPath}: $candidateFullPath"
    }
    return $candidateFullPath
}

function Remove-SmokeRealDirectoryTree {
    param(
        [string]$Path,
        [string]$ExpectedRoot
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    try {
        $directory = Resolve-SmokeRealDirectory -Path $Path -Label "cleanup directory"
        [void](Assert-SmokePathInsideRoot -Root $ExpectedRoot -Path $directory -Label "cleanup directory")
        Remove-Item -LiteralPath $directory -Recurse -Force
        return $null
    } catch {
        $message = "Refused unsafe smoke directory cleanup for ${Path}: $($_.Exception.Message)"
        Write-Warning $message
        return $message
    }
}
