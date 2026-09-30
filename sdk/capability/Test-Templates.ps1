[CmdletBinding()]
param(
    [string]$CargoExecutable = "cargo",
    [string]$NodeExecutable = "node",
    [string]$PythonExecutable = "python"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$sdkRoot = [System.IO.Path]::GetFullPath($PSScriptRoot)
$fakeHost = Join-Path $sdkRoot "fake_host.py"
$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$tempRoot = Join-Path $tempBase ("loom-capability-sdk-" + [Guid]::NewGuid().ToString("N"))
$rustCopy = Join-Path $tempRoot "rust"

try {
    New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    Copy-Item -LiteralPath (Join-Path $sdkRoot "templates\rust") -Destination $rustCopy -Recurse
    $rustTarget = Join-Path $tempRoot "rust-target"
    & $CargoExecutable build --quiet --manifest-path (Join-Path $rustCopy "Cargo.toml") --target-dir $rustTarget
    if ($LASTEXITCODE -ne 0) { throw "Rust capability template failed to build." }
    & $PythonExecutable $fakeHost --expected-language rust --working-directory $rustCopy (Join-Path $rustTarget "debug\loom-capability-template.exe")
    if ($LASTEXITCODE -ne 0) { throw "Rust capability template failed conformance." }
    & $PythonExecutable $fakeHost --expected-language typescript --working-directory (Join-Path $sdkRoot "templates\typescript") $NodeExecutable runtime.ts
    if ($LASTEXITCODE -ne 0) { throw "TypeScript capability template failed conformance." }
    & $PythonExecutable $fakeHost --expected-language python --working-directory (Join-Path $sdkRoot "templates\python") $PythonExecutable runtime.py
    if ($LASTEXITCODE -ne 0) { throw "Python capability template failed conformance." }
}
finally {
    $resolvedTemp = [System.IO.Path]::GetFullPath($tempRoot)
    if ($resolvedTemp.StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $resolvedTemp)) {
        Remove-Item -LiteralPath $resolvedTemp -Recurse -Force
    }
}

Write-Output "Rust, TypeScript, and Python capability templates passed."
