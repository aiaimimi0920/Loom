[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
. (Join-Path $repoRoot "scripts\LoomReleaseLayout.ps1")
. (Join-Path $repoRoot "scripts\build-release\Common.ps1")
. (Join-Path $repoRoot "scripts\build-release\Archives.ps1")
. (Join-Path $repoRoot "scripts\verify-release\Common.ps1")

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$ExpectedText)

    try {
        & $Action
    }
    catch {
        if (-not $_.Exception.Message.Contains($ExpectedText)) {
            throw "Unexpected error. Expected=[$ExpectedText] Actual=[$($_.Exception.Message)]"
        }
        return
    }
    throw "Expected action to fail with: $ExpectedText"
}

function New-TestZip {
    param([string]$Path, [object[]]$Entries)

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::Open($Path, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($entryRecord in $Entries) {
            $entry = $archive.CreateEntry([string]$entryRecord.name)
            $stream = $entry.Open()
            try {
                $bytes = [System.Text.ASCIIEncoding]::new().GetBytes([string]$entryRecord.value)
                $stream.Write($bytes, 0, $bytes.Length)
            }
            finally {
                $stream.Dispose()
            }
        }
    }
    finally {
        $archive.Dispose()
    }
}

Assert-Equal -Expected "packages\Loom.zip" -Actual (Assert-LoomSafeRelativePath -RelativePath "packages/Loom.zip") -Message "Safe package path normalization failed."
foreach ($invalidPath in @(
    ".",
    "..",
    "packages\..\outside",
    "packages\\double",
    "name:stream",
    "NUL.txt",
    "trailing."
)) {
    Assert-Throws -ExpectedText "Invalid Loom package-relative path" -Action {
        [void](Assert-LoomSafeRelativePath -RelativePath $invalidPath)
    }
}
$controlPath = "bad" + [char]1 + "name"
Assert-Throws -ExpectedText "Invalid Loom package-relative path" -Action {
    [void](Assert-LoomSafeRelativePath -RelativePath $controlPath)
}

$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-release-path-safety-" + [Guid]::NewGuid().ToString("N"))
$packageRoot = Join-Path $fixtureRoot "package"
$outsideRoot = Join-Path $fixtureRoot "outside"
$linkPath = Join-Path $packageRoot "linked"
New-Item -ItemType Directory -Path $packageRoot | Out-Null
New-Item -ItemType Directory -Path $outsideRoot | Out-Null
[System.IO.File]::WriteAllText((Join-Path $outsideRoot "outside.txt"), "outside", [System.Text.ASCIIEncoding]::new())
try {
    $junctionOutput = @(& cmd.exe /d /c "mklink /J `"$linkPath`" `"$outsideRoot`"" 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to create release path safety junction: $($junctionOutput -join ' ')"
    }

    Assert-Throws -ExpectedText "reparse points" -Action {
        [void]@(Get-LoomSafeDescendantFiles -RootPath $packageRoot)
    }
    Assert-Throws -ExpectedText "reparse points" -Action {
        [void](Resolve-PackageRelativePath -BasePath $packageRoot -RelativePath "linked\outside.txt")
    }
    Assert-Throws -ExpectedText "reparse points" -Action {
        Assert-LoomBuildOutputRoot -OutputRoot $linkPath
    }

    $boundedPath = Join-Path $packageRoot "bounded.txt"
    [System.IO.File]::WriteAllBytes($boundedPath, (New-Object byte[] 33))
    Assert-Throws -ExpectedText "32-byte limit" -Action {
        [void](Read-LoomBoundedFileBytes -Path $boundedPath -MaxBytes 32)
    }
    [System.IO.File]::WriteAllBytes($boundedPath, (New-Object byte[] 32))
    Assert-Equal -Expected 32 -Actual (Read-LoomBoundedFileBytes -Path $boundedPath -MaxBytes 32).Length -Message "Exact bounded-file limit must be accepted."
    Assert-Throws -ExpectedText "must not be negative" -Action {
        [void](Read-LoomBoundedFileBytes -Path $boundedPath -MaxBytes -1)
    }

    $digestPath = Join-Path $packageRoot "digest.txt"
    [System.IO.File]::WriteAllText($digestPath, "aaaa", [System.Text.ASCIIEncoding]::new())
    $script:LoomVerifiedFileDigests = @{}
    $originalWriteTime = (Get-Item -LiteralPath $digestPath).LastWriteTimeUtc
    [void](Get-LoomVerifiedFileDigest -Path $digestPath)
    [System.IO.File]::WriteAllText($digestPath, "bbbb", [System.Text.ASCIIEncoding]::new())
    (Get-Item -LiteralPath $digestPath).LastWriteTimeUtc = $originalWriteTime
    Assert-Throws -ExpectedText "changed during verification" -Action {
        [void](Get-LoomVerifiedFileDigest -Path $digestPath)
    }

    $invalidZip = Join-Path $packageRoot "invalid.zip"
    New-TestZip -Path $invalidZip -Entries @([pscustomobject]@{ name = "NUL.txt"; value = "x" })
    Assert-Throws -ExpectedText "Invalid Loom archive entry" -Action {
        [void]@(Get-LoomArchiveFileEntries -ZipPath $invalidZip)
    }
    Remove-Item -LiteralPath $invalidZip -Force

    $duplicateZip = Join-Path $packageRoot "duplicate.zip"
    New-TestZip -Path $duplicateZip -Entries @(
        [pscustomobject]@{ name = "safe.txt"; value = "x" },
        [pscustomobject]@{ name = "SAFE.txt"; value = "y" }
    )
    Assert-Throws -ExpectedText "Duplicate Loom archive entry" -Action {
        [void]@(Get-LoomArchiveFileEntries -ZipPath $duplicateZip)
    }
    Assert-Throws -ExpectedText "1-entry limit" -Action {
        [void]@(Get-LoomArchiveFileEntries -ZipPath $duplicateZip -MaxEntries 1)
    }

    $boundedZip = Join-Path $packageRoot "bounded.zip"
    New-TestZip -Path $boundedZip -Entries @([pscustomobject]@{ name = "payload.bin"; value = "xx" })
    Assert-Throws -ExpectedText "uncompressed limit" -Action {
        [void]@(Get-LoomArchiveFileEntries -ZipPath $boundedZip -MaxUncompressedBytes 1)
    }

    $archiveStage = Join-Path $fixtureRoot "archive-stage"
    $archiveStageChild = Join-Path $archiveStage "nested"
    New-Item -ItemType Directory -Path $archiveStageChild | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $archiveStageChild "file.txt"), "x", [System.Text.ASCIIEncoding]::new())
    Remove-LoomArchiveStage -Stage $archiveStage
    Assert-Equal -Expected $false -Actual (Test-Path -LiteralPath $archiveStage) -Message "Archive stage cleanup must delete ordinary descendants."
}
finally {
    if (Test-Path -LiteralPath $linkPath) {
        [System.IO.Directory]::Delete($linkPath)
    }
    if (Test-Path -LiteralPath $fixtureRoot) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
    }
}

Write-Output "Loom release path safety contract passed."
