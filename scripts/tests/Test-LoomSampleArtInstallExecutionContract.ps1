<# Verifies the sample Art install smoke module graph and its security-critical helpers. #>

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)

    if (-not $Condition) { throw $Message }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$mainPath = Join-Path $scriptRoot "Test-LoomSampleArtInstallExecution.ps1"
$moduleRoot = Join-Path $scriptRoot "sample-art-install"
$moduleNames = @("Common.ps1", "Http.ps1", "Packages.ps1", "ImageSearchApiFixture.ps1")
$sourcePaths = @($mainPath) + @($moduleNames | ForEach-Object { Join-Path $moduleRoot $_ })
foreach ($path in $sourcePaths) {
    Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "Sample Art install smoke source is missing: $path"
    $tokens = $null
    $parseErrors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count -gt 0) {
        throw "PowerShell parse failed for $path`: $($parseErrors.Message -join '; ')"
    }
    $bytes = [System.IO.File]::ReadAllBytes($path)
    Assert-True (-not ($bytes | Where-Object { $_ -gt 127 } | Select-Object -First 1)) "PowerShell source must remain ASCII-only: $path"
    Assert-True (-not ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)) "PowerShell source must remain UTF-8 without BOM: $path"
}

$mainText = [System.IO.File]::ReadAllText($mainPath)
Assert-True ($mainText.Contains('@("Common.ps1", "Http.ps1", "Packages.ps1", "ImageSearchApiFixture.ps1")')) "Sample Art install helper module order changed."
Assert-True (-not $mainText.Contains("function Assert-True")) "Extracted helpers were copied back into the orchestrator."
Assert-True (-not $mainText.Contains("StandardError.ReadToEnd")) "A redirected fixture pipe can deadlock the sample Art smoke."
Assert-True ($mainText.Contains("Stop-ProcessTree -Process `$daemon")) "Daemon process-tree cleanup is not wired."
Assert-True ($mainText.Contains("Remove-VerifiedTemporaryTree -Path `$controlPlane")) "Verified temporary-tree cleanup is not wired."

$commonPath = Join-Path $moduleRoot "Common.ps1"
$commonText = [System.IO.File]::ReadAllText($commonPath)
Assert-True (-not $commonText.Contains("ReadAllBytes(`$ZipPath)")) "Package installation must use the bounded reader."
Assert-True ($commonText.Contains("ConvertTo-WindowsCommandLineArgument")) "Windows argument quoting helper is missing."
Assert-True ($commonText.Contains("Get-CimInstance -ClassName Win32_Process")) "Fixture descendant cleanup is missing."
Assert-True ($commonText.Contains("[System.IO.FileAttributes]::ReparsePoint")) "Temporary cleanup does not reject reparse points."
Assert-True (-not $commonText.Contains("Remove-Item -LiteralPath `$resolved -Recurse")) "Temporary cleanup reverted to a recursive traversal after validation."

$httpPath = Join-Path $moduleRoot "Http.ps1"
$httpText = [System.IO.File]::ReadAllText($httpPath)
Assert-True ($httpText.Contains("Read-BoundedHttpStream")) "Loom JSON responses are not byte-bounded."
Assert-True ($httpText.Contains("-MaximumBytes (1MB)")) "Loom HTTP error bodies are not tightly bounded."

$fixtureModuleText = [System.IO.File]::ReadAllText((Join-Path $moduleRoot "ImageSearchApiFixture.ps1"))
Assert-True ($fixtureModuleText.Contains("New-Object byte[] (64KB)")) "Image-search fixture header parsing is not bounded."
Assert-True ($fixtureModuleText.Contains("ReadTimeout = 5000")) "Image-search fixture reads are not timeout-bounded."
Assert-True (-not $fixtureModuleText.Contains("RedirectStandardError = `$true")) "Image-search fixture must redirect to files, not an undrained pipe."

. $commonPath
. $httpPath
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-sample-art-install-" + [Guid]::NewGuid().ToString("N"))
$process = $null
New-Item -ItemType Directory -Force -Path (Join-Path $tempRoot "fixture dir") | Out-Null
try {
    $smallPath = Join-Path $tempRoot "small.bin"
    [System.IO.File]::WriteAllBytes($smallPath, [byte[]]@(1, 2, 3))
    $small = Read-BoundedFileBytes -Path $smallPath -MaximumBytes 3
    Assert-True ($small.GetType().FullName -eq "System.Byte[]" -and $small.Length -eq 3) "Bounded package reader changed its byte-array contract."

    $largePath = Join-Path $tempRoot "large.bin"
    [System.IO.File]::WriteAllBytes($largePath, [byte[]](0..8))
    $boundedReadRejected = $false
    try { $null = Read-BoundedFileBytes -Path $largePath -MaximumBytes 8 }
    catch { $boundedReadRejected = $_.Exception.Message -like "Package exceeds*" }
    Assert-True $boundedReadRejected "Bounded package reader accepted an oversized file."

    $httpStream = [System.IO.MemoryStream]::new([byte[]](0..8))
    $boundedResponseRejected = $false
    try { $null = Read-BoundedHttpStream -Stream $httpStream -MaximumBytes 8 }
    catch { $boundedResponseRejected = $_.Exception.Message -like "Loom HTTP response exceeded*" }
    finally { $httpStream.Dispose() }
    Assert-True $boundedResponseRejected "Bounded Loom HTTP reader accepted an oversized response."

    $redacted = Redact-SensitiveText 'Authorization: Bearer secret token=second api_key=third loom-package-smoke-key'
    Assert-True (-not $redacted.Contains("secret") -and -not $redacted.Contains("second") -and -not $redacted.Contains("third") -and -not $redacted.Contains("loom-package-smoke-key")) "Diagnostic redaction leaked a representative secret."

    $childPath = Join-Path $tempRoot "fixture dir\echo args.ps1"
    $outputPath = Join-Path $tempRoot "fixture dir\captured args.json"
    $stdoutPath = Join-Path $tempRoot "fixture stdout.log"
    $stderrPath = Join-Path $tempRoot "fixture stderr.log"
    $childSource = @'
param(
    [string]$Value,
    [string]$Trailing,
    [AllowEmptyString()][string]$EmptyValue,
    [string]$OutputPath
)
[System.IO.File]::WriteAllText(
    $OutputPath,
    (@($Value, $Trailing, $EmptyValue) | ConvertTo-Json -Compress),
    [System.Text.UTF8Encoding]::new($false)
)
'@
    Write-Utf8NoBomFile -Path $childPath -Content ($childSource + "`n")
    $expectedValue = 'alpha "beta" gamma'
    $expectedTrailing = 'C:\fixture root\'
    $process = Start-PowerShellFixtureProcess `
        -ScriptPath $childPath `
        -Parameters @{ Value = $expectedValue; Trailing = $expectedTrailing; EmptyValue = ""; OutputPath = $outputPath } `
        -StdoutPath $stdoutPath `
        -StderrPath $stderrPath
    Assert-True $process.WaitForExit(10000) "Quoted fixture child did not exit."
    $process.WaitForExit()
    Assert-True ($process.ExitCode -eq 0) "Quoted fixture child failed with exit code $($process.ExitCode). stdout=$(Read-BoundedRedactedText -Path $stdoutPath) stderr=$(Read-BoundedRedactedText -Path $stderrPath)"
    $captured = (Read-BoundedUtf8Text -Path $outputPath) | ConvertFrom-Json
    Assert-True ($captured.Count -eq 3) "Quoted fixture child returned an unexpected argument count."
    Assert-True ([string]$captured[0] -eq $expectedValue) "Quoted argument containing spaces and quotes changed."
    Assert-True ([string]$captured[1] -eq $expectedTrailing) "Quoted argument ending in a backslash changed."
    Assert-True ([string]$captured[2] -eq "") "Empty quoted argument changed."

    $unexpectedCleanupRejected = $false
    try { Remove-VerifiedTemporaryTree -Path (Join-Path ([System.IO.Path]::GetTempPath()) "not-owned-by-loom") }
    catch { $unexpectedCleanupRejected = $_.Exception.Message -like "Refusing to remove*" }
    Assert-True $unexpectedCleanupRejected "Temporary cleanup accepted a path outside its ownership pattern."
    [System.IO.File]::SetAttributes($smallPath, [System.IO.FileAttributes]::ReadOnly)
    [System.IO.File]::SetAttributes((Join-Path $tempRoot "fixture dir"), [System.IO.FileAttributes]::ReadOnly)
}
finally {
    Stop-ProcessTree -Process $process
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-VerifiedTemporaryTree -Path $tempRoot
    }
}

Write-Host "Loom sample Art install execution contract passed."
