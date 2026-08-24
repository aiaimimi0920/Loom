[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:DaemonRequestHeaders = @{}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$moduleRoot = Join-Path $repoRoot "scripts\framework-art-store-hook-smoke"
foreach ($moduleName in @(
    "Assertions.ps1",
    "Paths.ps1",
    "Process.ps1",
    "Http.ps1",
    "HookBridge.ps1",
    "FixtureArchive.ps1"
)) {
    . (Join-Path $moduleRoot $moduleName)
}

function Assert-ThrowsWithText {
    param(
        [scriptblock]$Action,
        [string]$ExpectedText
    )

    try {
        & $Action
    } catch {
        Assert-True `
            -Condition $_.Exception.Message.Contains($ExpectedText) `
            -Message "Exception did not contain expected text [$ExpectedText]: $($_.Exception.Message)"
        return
    }
    throw "Expected action to throw text [$ExpectedText]."
}

$tempRoot = Resolve-SmokeRealDirectory -Path $env:TEMP -Label "test temporary directory"
$testRoot = Initialize-SmokeRealDirectory `
    -Path (Join-Path $tempRoot ("loom-framework-smoke-modules-" + [Guid]::NewGuid().ToString("N"))) `
    -Label "framework smoke module test root"
try {
    Assert-Equal `
        -Expected "runtime/main.ps1" `
        -Actual (ConvertTo-SmokeZipRelativePath -Path "runtime\main.ps1") `
        -Message "ZIP entry normalization mismatch."
    Assert-ThrowsWithText `
        -Action { ConvertTo-SmokeZipRelativePath -Path "../escape.txt" } `
        -ExpectedText "unsafe segment"
    Assert-ThrowsWithText `
        -Action { ConvertTo-SmokeZipRelativePath -Path "runtime//main.ps1" } `
        -ExpectedText "unsafe segment"
    Assert-ThrowsWithText `
        -Action { ConvertTo-SmokeZipRelativePath -Path "runtime/CON.txt" } `
        -ExpectedText "unsafe segment"

    [void](Assert-SmokeLoopbackHttpUri -Uri "http://127.0.0.1:43210/health")
    [void](Assert-SmokeLoopbackHttpUri -Uri "http://localhost:43210/health")
    Assert-ThrowsWithText `
        -Action { Assert-SmokeLoopbackHttpUri -Uri "https://127.0.0.1:43210/health" } `
        -ExpectedText "loopback HTTP"
    Assert-ThrowsWithText `
        -Action { Assert-SmokeLoopbackHttpUri -Uri "http://example.com/health" } `
        -ExpectedText "target loopback"
    Assert-ThrowsWithText `
        -Action { Assert-SmokeLoopbackHttpUri -Uri "http://user:pass@127.0.0.1/health" } `
        -ExpectedText "unauthenticated"

    $secret = "smoke-secret-" + [Guid]::NewGuid().ToString("N")
    $safeError = ConvertTo-SafeSmokeErrorText `
        -Text "token=$secret Authorization: Bearer abc.def-123" `
        -Secrets @($secret)
    Assert-True (-not $safeError.Contains($secret)) "Explicit secret was not redacted."
    Assert-True (-not $safeError.Contains("abc.def-123")) "Bearer token was not redacted."
    Assert-ThrowsWithText `
        -Action { Send-LoomHookBridgeWebSocketJson -Client $null -Json ("x" * (1MB + 1)) } `
        -ExpectedText "1 MiB"
    Assert-ThrowsWithText `
        -Action { Receive-LoomHookBridgeWebSocketJson -Client $null -TimeoutSeconds 0 } `
        -ExpectedText "bounds must be positive"

    $junctionTarget = Initialize-SmokeRealDirectory `
        -Path (Join-Path $testRoot "junction-target") `
        -Label "junction target"
    $junctionPath = Join-Path $testRoot "junction"
    try {
        New-Item -ItemType Junction -Path $junctionPath -Target $junctionTarget | Out-Null
        Assert-ThrowsWithText `
            -Action { Resolve-SmokeRealDirectory -Path $junctionPath -Label "junction test" } `
            -ExpectedText "reparse point"
    } finally {
        if (Test-Path -LiteralPath $junctionPath) {
            Remove-Item -LiteralPath $junctionPath -Force
        }
    }

    $treeScriptPath = Join-Path $testRoot "spawn-process-tree.ps1"
    $childPidPath = Join-Path $testRoot "spawned-child.pid"
    Write-Utf8NoBomFile -Path $treeScriptPath -Content @'
param([string]$ChildPidPath)
$child = Start-Process `
    -FilePath (Join-Path $PSHOME "powershell.exe") `
    -ArgumentList @("-NoLogo", "-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 60") `
    -WindowStyle Hidden `
    -PassThru
[System.IO.File]::WriteAllText($ChildPidPath, [string]$child.Id, [System.Text.Encoding]::ASCII)
Start-Sleep -Seconds 60
'@
    $treeProcess = $null
    try {
        $treeProcess = Start-SmokeProcess `
            -FilePath (Join-Path $PSHOME "powershell.exe") `
            -ArgumentList @(
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy", "Bypass",
                "-File", $treeScriptPath,
                "-ChildPidPath", $childPidPath
            ) `
            -WorkingDirectory $testRoot `
            -StdoutPath (Join-Path $testRoot "tree.stdout.log") `
            -StderrPath (Join-Path $testRoot "tree.stderr.log")
        $treeParentId = $treeProcess.Id
        $pidDeadline = [DateTime]::UtcNow.AddSeconds(10)
        while (-not (Test-Path -LiteralPath $childPidPath) -and [DateTime]::UtcNow -lt $pidDeadline) {
            Start-Sleep -Milliseconds 50
        }
        Assert-True (Test-Path -LiteralPath $childPidPath -PathType Leaf) "Process-tree fixture did not publish its child PID."
        $treeChildId = [int][System.IO.File]::ReadAllText($childPidPath)
        $treeCleanupErrors = @(Stop-SpawnedProcess -Process $treeProcess)
        $treeProcess = $null
        Assert-Equal 0 $treeCleanupErrors.Count "Process-tree cleanup reported failures."
        Assert-True ($null -eq (Get-Process -Id $treeParentId -ErrorAction SilentlyContinue)) "Spawned parent process leaked."
        Assert-True ($null -eq (Get-Process -Id $treeChildId -ErrorAction SilentlyContinue)) "Spawned child process leaked."
    } finally {
        if ($null -ne $treeProcess) {
            [void](Stop-SpawnedProcess -Process $treeProcess)
        }
    }

    $copySource = Join-Path $testRoot "copy-source.txt"
    Write-Utf8NoBomFile -Path $copySource -Content "copied file"
    $directorySource = Initialize-SmokeRealDirectory `
        -Path (Join-Path $testRoot "directory-source\nested") `
        -Label "directory fixture source"
    Write-Utf8NoBomFile -Path (Join-Path $directorySource "copied.txt") -Content "nested copy"
    $archiveRoot = Initialize-SmokeRealDirectory `
        -Path (Join-Path $testRoot "archives") `
        -Label "archive output root"
    $zipA = Join-Path $archiveRoot "fixture-a.zip"
    $zipB = Join-Path $archiveRoot "fixture-b.zip"
    $textFiles = @{
        "manifest.json" = '{"id":"fixture"}'
        "runtime/main.ps1" = "Write-Output fixture"
    }
    $fileCopies = @{ "runtime/copied.txt" = $copySource }
    $directoryCopies = @{ "payload" = (Split-Path -Parent $directorySource) }
    New-ZipFixture -ZipPath $zipA -TextFiles $textFiles -FileCopies $fileCopies -DirectoryCopies $directoryCopies
    New-ZipFixture -ZipPath $zipB -TextFiles $textFiles -FileCopies $fileCopies -DirectoryCopies $directoryCopies

    $hashA = (Get-FileHash -LiteralPath $zipA -Algorithm SHA256).Hash.ToLowerInvariant()
    $hashB = (Get-FileHash -LiteralPath $zipB -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert-Equal $hashA $hashB "Fixture ZIP output is not deterministic."
    $expectedSidecar = "$hashA  fixture-a.zip" + [Environment]::NewLine
    $actualSidecar = [System.IO.File]::ReadAllText("$zipA.sha256")
    Assert-Equal $expectedSidecar $actualSidecar "Fixture ZIP checksum sidecar mismatch."

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipA)
    try {
        $entryNames = @($archive.Entries | ForEach-Object { $_.FullName })
        $expectedEntries = @(
            "manifest.json",
            "payload/nested/copied.txt",
            "runtime/copied.txt",
            "runtime/main.ps1"
        )
        Assert-Equal `
            -Expected ($expectedEntries -join ",") `
            -Actual ($entryNames -join ",") `
            -Message "Fixture ZIP entry set or order mismatch."
        Assert-True `
            -Condition (@($entryNames | Where-Object { $_.Contains("\") }).Count -eq 0) `
            -Message "Fixture ZIP contains a Windows-style entry name."
        Assert-True `
            -Condition (@($archive.Entries | Where-Object { $_.LastWriteTime.Year -ne 1980 }).Count -eq 0) `
            -Message "Fixture ZIP timestamps are not normalized."
    } finally {
        $archive.Dispose()
    }

    $escapePath = Join-Path $testRoot "escape.txt"
    Assert-ThrowsWithText `
        -Action {
            New-ZipFixture `
                -ZipPath (Join-Path $archiveRoot "traversal.zip") `
                -TextFiles @{ "../escape.txt" = "escape" }
        } `
        -ExpectedText "unsafe segment"
    Assert-True (-not (Test-Path -LiteralPath $escapePath)) "Traversal fixture escaped its stage."

    Write-Output "Framework Art Store/Hook smoke module tests passed."
} finally {
    [void](Remove-SmokeRealDirectoryTree -Path $testRoot -ExpectedRoot $tempRoot)
}
