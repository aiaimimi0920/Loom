[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ArtifactRoot,
    [string]$PluginCliPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$root = [System.IO.Path]::GetFullPath($ArtifactRoot)
Assert-True (Test-Path -LiteralPath $root -PathType Container) "OCR artifact root is missing: $root"

$summaryPath = Join-Path $root "summary.json"
$catalogPath = Join-Path $root "catalog.json"
$trustPath = Join-Path $root "plugin-trust.json"
foreach ($path in @($summaryPath, $catalogPath, $trustPath)) {
    Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "OCR artifact is missing: $path"
}

$summary = Get-Content -Raw -Encoding UTF8 -LiteralPath $summaryPath | ConvertFrom-Json
$capabilities = @($summary.capabilities)
Assert-True ($capabilities.Count -eq 1) "OCR summary must describe exactly one capability."
$capability = $capabilities[0]
Assert-True ([string]$capability.qualifiedId -ceq "neuro.official/ocr") "OCR capability identity mismatch."
Assert-True ([string]$capability.version -ceq "1.3.0") "OCR capability version mismatch."

$zipName = [string]$capability.zip
$zipPath = Join-Path $root $zipName
$sidecarPath = Join-Path $root ([string]$capability.sidecar)
foreach ($path in @($zipPath, $sidecarPath, (Join-Path $root "ocr.cdx.json"), (Join-Path $root "ocr.provenance.json"))) {
    Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "OCR package artifact is missing: $path"
}

$zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
Assert-True ($zipHash -ceq [string]$capability.sha256) "OCR ZIP hash differs from summary.json."
Assert-True ((Get-Item -LiteralPath $zipPath).Length -eq [int64]$capability.bytes) "OCR ZIP length differs from summary.json."
$sidecarFields = @((Get-Content -Raw -Encoding UTF8 -LiteralPath $sidecarPath).Trim() -split '\s+')
Assert-True ($sidecarFields.Count -eq 2) "OCR ZIP checksum sidecar format is invalid."
Assert-True ([string]::Equals($sidecarFields[0], $zipHash, [System.StringComparison]::OrdinalIgnoreCase)) "OCR ZIP checksum mismatch."
Assert-True ([string]$sidecarFields[1] -ceq $zipName) "OCR ZIP checksum filename mismatch."

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
try {
    $entries = @($archive.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
    foreach ($required in @(
        "capability.manifest.json",
        "schemas/recognize-input.v1.schema.json",
        "schemas/ocr-result.v1.schema.json",
        "schemas/ocr-codes.v1.schema.json",
        "runtime/loom-ocr-host.exe"
    )) {
        Assert-True ($entries -contains $required) "OCR ZIP is missing required entry: $required"
    }
    $resourceEntries = @($entries | Where-Object {
        $_ -like "runtime/resources/ocr/*" -and -not $_.EndsWith('/')
    })
    Assert-True ($resourceEntries.Count -eq 7) "OCR ZIP model/runtime resource count mismatch."
    $manifestEntry = $archive.GetEntry("capability.manifest.json")
    $manifestStream = $manifestEntry.Open()
    try {
        $reader = [System.IO.StreamReader]::new($manifestStream, [System.Text.Encoding]::UTF8, $true, 4096, $true)
        try { $packageManifest = $reader.ReadToEnd() | ConvertFrom-Json }
        finally { $reader.Dispose() }
    }
    finally { $manifestStream.Dispose() }
    Assert-True ([string]$packageManifest.publisher.id -ceq "neuro.official") "OCR package manifest publisher mismatch."
    Assert-True ([string]$packageManifest.id -ceq "ocr") "OCR package manifest id mismatch."
    Assert-True ([string]$packageManifest.version -ceq [string]$capability.version) "OCR package manifest version differs from summary.json."
}
finally {
    $archive.Dispose()
}

$catalog = Get-Content -Raw -Encoding UTF8 -LiteralPath $catalogPath | ConvertFrom-Json
$packages = @($catalog.signed.packages)
Assert-True ($packages.Count -eq 1) "OCR catalog must contain exactly one package."
Assert-True ([string]$packages[0].qualifiedId -ceq "neuro.official/ocr") "OCR catalog identity mismatch."
Assert-True ([string]$packages[0].package.sha256 -ceq $zipHash) "OCR catalog ZIP hash mismatch."

$pluginCli = if ([string]::IsNullOrWhiteSpace($PluginCliPath)) {
    Join-Path $repoRoot "target\release\loom-plugin.exe"
} else {
    [System.IO.Path]::GetFullPath($PluginCliPath)
}
Assert-True (Test-Path -LiteralPath $pluginCli -PathType Leaf) "loom-plugin is missing: $pluginCli"
& $pluginCli catalog validate $catalogPath $trustPath | Out-Null
Assert-True ($LASTEXITCODE -eq 0) "Signed OCR catalog failed validation."

Write-Host "OCR capability package contract passed: qualifiedId=neuro.official/ocr version=$($capability.version)"
