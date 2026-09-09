[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ArtifactRoot,
    [string]$PluginCliExecutable = ".\target\release\loom-plugin.exe",
    [string]$PythonExecutable = "python"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Resolve-ArtifactFile {
    param([string]$Root, [string]$RelativePath)
    Assert-True (-not [string]::IsNullOrWhiteSpace($RelativePath) -and
        -not [System.IO.Path]::IsPathRooted($RelativePath)) "Artifact path must be package-relative."
    $rootPrefix = [System.IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    $resolved = [System.IO.Path]::GetFullPath((Join-Path $Root $RelativePath))
    Assert-True (($resolved + '\').StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) "Artifact escaped its package root."
    Assert-LoomPathHasNoReparsePoints -RootPath $Root -Path $resolved
    return $resolved
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
. (Join-Path $repoRoot "scripts\LoomReleaseLayout.ps1")
$root = [System.IO.Path]::GetFullPath($ArtifactRoot)
$pluginCli = if ([System.IO.Path]::IsPathRooted($PluginCliExecutable)) {
    [System.IO.Path]::GetFullPath($PluginCliExecutable)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $PluginCliExecutable))
}
Assert-True (Test-Path -LiteralPath $root -PathType Container) "Translation artifact root is missing: $root"
Assert-True (Test-Path -LiteralPath $pluginCli -PathType Leaf) "Plugin CLI is missing: $pluginCli"
Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $root
$summary = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $root "summary.json") | ConvertFrom-Json
$capabilities = @($summary.capabilities)
Assert-True ($capabilities.Count -eq 1) "Translation summary must contain exactly one capability."
$capability = $capabilities[0]
Assert-True ([string]$capability.qualifiedId -ceq "neuro.official/text-translation") "Translation capability identity mismatch."
Assert-True ([string]$capability.version -ceq "1.0.0") "Translation capability version mismatch."
Assert-True ($capability.deterministic -eq $true) "Translation capability must be deterministic."

$zipPath = Resolve-ArtifactFile -Root $root -RelativePath ([string]$capability.zip)
$sidecarPath = Resolve-ArtifactFile -Root $root -RelativePath ([string]$capability.sidecar)
$sbomPath = Resolve-ArtifactFile -Root $root -RelativePath ([string]$capability.sbom.path)
$provenancePath = Resolve-ArtifactFile -Root $root -RelativePath ([string]$capability.provenance.path)
foreach ($path in @($zipPath, $sidecarPath, $sbomPath, $provenancePath, (Join-Path $root "catalog.json"), (Join-Path $root "plugin-trust.json"))) {
    Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "Translation capability artifact is missing: $path"
}
$zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
Assert-True ($zipHash -ceq [string]$capability.sha256) "Translation ZIP hash does not match summary."
$sidecar = (Get-Content -Raw -Encoding UTF8 -LiteralPath $sidecarPath).Trim()
Assert-True ($sidecar -ceq "$zipHash  text-translation.zip") "Translation ZIP sidecar is invalid."
$sbom = Get-Content -Raw -Encoding UTF8 -LiteralPath $sbomPath | ConvertFrom-Json
Assert-True ([string]$sbom.bomFormat -ceq "CycloneDX" -and [string]$sbom.specVersion -ceq "1.6") "Translation SBOM contract mismatch."
$provenance = Get-Content -Raw -Encoding UTF8 -LiteralPath $provenancePath | ConvertFrom-Json
Assert-True ([string]$provenance.qualifiedId -ceq [string]$capability.qualifiedId) "Translation provenance identity mismatch."
Assert-True ([string]$provenance.subject.sha256 -ceq $zipHash -and $provenance.deterministic -eq $true) "Translation provenance subject mismatch."

& $pluginCli catalog validate (Join-Path $root "catalog.json") (Join-Path $root "plugin-trust.json") | Out-Null
Assert-True ($LASTEXITCODE -eq 0) "Translation capability catalog signature validation failed."

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$tempRoot = Join-Path $tempBase ("loom-translation-contract-" + [Guid]::NewGuid().ToString("N"))
try {
    $expected = @(
        "capability.manifest.json",
        "runtime/loom-text-translation-host.exe",
        "schemas/translate-input.v1.schema.json",
        "schemas/translate-output.v1.schema.json",
        "signature.json"
    ) | Sort-Object
    $actual = @($archive.Entries | ForEach-Object { $_.FullName.Replace('\', '/') } | Sort-Object)
    Assert-True (($actual -join "`n") -ceq ($expected -join "`n")) "Translation ZIP entries do not match the package contract."
    $runtimeEntry = @($archive.Entries | Where-Object { $_.FullName -ceq "runtime/loom-text-translation-host.exe" })
    Assert-True ($runtimeEntry.Count -eq 1) "Translation ZIP runtime entry is missing or duplicated."
    New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
    Assert-LoomPathHasNoReparsePoints -RootPath $tempBase -Path $tempRoot
    $runtimePath = Join-Path $tempRoot "loom-text-translation-host.exe"
    $source = $runtimeEntry[0].Open()
    $destination = [System.IO.File]::Create($runtimePath)
    try { $source.CopyTo($destination) } finally { $destination.Dispose(); $source.Dispose() }
    & $PythonExecutable (Join-Path $repoRoot "sdk\capability\fake_host.py") `
        --expected-text-utf8-hex e4bda0e5a5bd --target-language zh-CN `
        --expected-translated true `
        --command-id neuro.official/text-translation.translate `
        --working-directory $tempRoot $runtimePath
    Assert-True ($LASTEXITCODE -eq 0) "Packaged translation runtime failed fake-host conformance."
}
finally {
    $archive.Dispose()
    if (([System.IO.Path]::GetFullPath($tempRoot)).StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Test-Path -LiteralPath $tempRoot)) {
        Assert-LoomPathHasNoReparsePoints -RootPath $tempBase -Path $tempRoot
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

Write-Output "Text translation capability package contract passed."
