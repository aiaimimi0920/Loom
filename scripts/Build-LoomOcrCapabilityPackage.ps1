[CmdletBinding()]
param(
    [string]$OutputRoot = ".loom-capability-packages\ocr",
    [ValidateSet("Debug", "Release")][string]$Configuration = "Release",
    [string]$ResourceRoot = "",
    [string]$SigningKeyPath = $env:LOOM_PACKAGE_SIGNING_KEY_PATH,
    [string]$SigningPublisherId = $env:LOOM_PACKAGE_SIGNING_PUBLISHER_ID
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Utf8NoBomFile {
    param([string]$Path, [object]$Value)

    $json = ($Value | ConvertTo-Json -Depth 30) + "`n"
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.UTF8Encoding]::new($false))
}

function Assert-PathInside {
    param([string]$Path, [string]$Root, [string]$Label)

    $resolvedPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\') + '\'
    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    if (-not $resolvedPath.StartsWith($resolvedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label escaped its intended root: $Path"
    }
}

function Get-FileRecord {
    param([string]$Path, [string]$Root)

    $file = Get-Item -LiteralPath $Path
    return [ordered]@{
        path = $file.FullName.Substring($Root.TrimEnd('\').Length + 1).Replace('\', '/')
        bytes = [int64]$file.Length
        sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
. (Join-Path $scriptRoot "PackageSigning.ps1")
. (Join-Path $scriptRoot "LoomReleaseLayout.ps1")
$signingContext = New-LoomPackageSigningContext `
    -RepoRoot $repoRoot `
    -KeyPath $SigningKeyPath `
    -PublisherId $SigningPublisherId
if ($null -eq $signingContext) {
    throw "The OCR capability candidate must be signed. Set LOOM_PACKAGE_SIGNING_KEY_PATH and LOOM_PACKAGE_SIGNING_PUBLISHER_ID."
}
if ([string]$signingContext.PublisherId -ne "neuro.official") {
    throw "The official OCR capability must be signed by publisher neuro.official."
}

$outputRootPath = if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    [System.IO.Path]::GetFullPath($OutputRoot)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputRoot))
}
$stageRoot = Join-Path $outputRootPath ".staging-ocr"
$trustPath = Join-Path $outputRootPath ".signing-trust.json"
$zipPath = Join-Path $outputRootPath "ocr.zip"
$reproZipPath = Join-Path $outputRootPath ".ocr-repro.zip"
$sourceRoot = Join-Path $repoRoot "capability-packages\ocr"
$resourceRoot = if ([string]::IsNullOrWhiteSpace($ResourceRoot)) {
    Join-Path $repoRoot "resources\ocr"
} elseif ([System.IO.Path]::IsPathRooted($ResourceRoot)) {
    [System.IO.Path]::GetFullPath($ResourceRoot)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ResourceRoot))
}
$resourceNames = @(
    "README.txt",
    "ch_PP-OCRv4_det_infer.onnx",
    "ch_ppocr_mobile_v2.0_cls_infer.onnx",
    "ch_PP-OCRv4_rec_infer.onnx",
    "ch_PP-OCRv5_rec_mobile_infer.onnx",
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll"
)

New-Item -ItemType Directory -Force -Path $outputRootPath | Out-Null
Assert-LoomPathHasNoReparsePoints -RootPath $outputRootPath -Path $outputRootPath
Assert-PathInside -Path $stageRoot -Root $outputRootPath -Label "OCR staging root"
foreach ($path in @($stageRoot, $trustPath, $zipPath, "$zipPath.sha256", $reproZipPath, "$reproZipPath.sha256")) {
    if (Test-Path -LiteralPath $path) {
        Assert-LoomPathHasNoReparsePoints -RootPath $outputRootPath -Path $path
        Remove-Item -LiteralPath $path -Recurse -Force
    }
}

try {
    $cargoArguments = @("build", "--locked", "-p", "loom-ocr-host")
    if ($Configuration -eq "Release") {
        $cargoArguments += "--release"
    }
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to build loom-ocr-host."
    }
    $profile = if ($Configuration -eq "Release") { "release" } else { "debug" }
    $hostPath = Join-Path $repoRoot "target\$profile\loom-ocr-host.exe"
    $manifestPath = Join-Path $sourceRoot "capability.manifest.json"
    $recognizeSchemaPath = Join-Path $sourceRoot "schemas\recognize-input.v1.schema.json"
    $ocrSchemaPath = Join-Path $sourceRoot "schemas\ocr-result.v1.schema.json"
    $codesSchemaPath = Join-Path $sourceRoot "schemas\ocr-codes.v1.schema.json"
    $requiredFiles = @(
        $manifestPath,
        $recognizeSchemaPath,
        $ocrSchemaPath,
        $codesSchemaPath,
        $hostPath
    ) + @($resourceNames | ForEach-Object { Join-Path $resourceRoot $_ })
    foreach ($required in $requiredFiles) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "OCR capability input is missing: $required"
        }
    }

    New-Item -ItemType Directory -Path (Join-Path $stageRoot "schemas") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $stageRoot "runtime\resources\ocr") -Force | Out-Null
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $stageRoot "capability.manifest.json")
    Copy-Item -LiteralPath $recognizeSchemaPath -Destination (Join-Path $stageRoot "schemas\recognize-input.v1.schema.json")
    Copy-Item -LiteralPath $ocrSchemaPath -Destination (Join-Path $stageRoot "schemas\ocr-result.v1.schema.json")
    Copy-Item -LiteralPath $codesSchemaPath -Destination (Join-Path $stageRoot "schemas\ocr-codes.v1.schema.json")
    Copy-Item -LiteralPath $hostPath -Destination (Join-Path $stageRoot "runtime\loom-ocr-host.exe")
    foreach ($name in $resourceNames) {
        Copy-Item -LiteralPath (Join-Path $resourceRoot $name) -Destination (Join-Path $stageRoot "runtime\resources\ocr\$name")
    }

    Invoke-LoomPackageSigning -Context $signingContext -PackageDirectory $stageRoot
    & $signingContext.Executable trust add $trustPath $signingContext.PublisherId $signingContext.KeyPath | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to create the isolated OCR package trust store."
    }
    $validation = & $signingContext.Executable validate $stageRoot --trust-store $trustPath 2>&1
    if ($LASTEXITCODE -ne 0 -or ($validation -join " ") -notmatch 'trust=Trusted') {
        throw "The signed OCR capability failed trusted validation: $($validation -join ' ')"
    }

    & $signingContext.Executable pack $stageRoot $zipPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to pack the OCR capability." }
    & $signingContext.Executable pack $stageRoot $reproZipPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to reproduce the OCR capability package." }
    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $reproHash = (Get-FileHash -LiteralPath $reproZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($zipHash -ne $reproHash) {
        throw "OCR capability packaging is not deterministic."
    }
    $sidecarPath = "$zipPath.sha256"
    [System.IO.File]::WriteAllText(
        $sidecarPath,
        "$zipHash  $([System.IO.Path]::GetFileName($zipPath))`n",
        [System.Text.UTF8Encoding]::new($false)
    )

    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $stageRoot "capability.manifest.json") | ConvertFrom-Json
    $materials = @(Get-ChildItem -LiteralPath $stageRoot -Recurse -File | ForEach-Object {
        Get-FileRecord -Path $_.FullName -Root $stageRoot
    } | Sort-Object path)
    $components = @($materials | ForEach-Object {
        [ordered]@{
            type = "file"
            name = $_.path
            hashes = @([ordered]@{ alg = "SHA-256"; content = $_.sha256 })
        }
    })
    $sbomPath = Join-Path $outputRootPath "ocr.cdx.json"
    Write-Utf8NoBomFile -Path $sbomPath -Value ([ordered]@{
        bomFormat = "CycloneDX"
        specVersion = "1.6"
        version = 1
        metadata = [ordered]@{
            component = [ordered]@{ type = "application"; name = "Loom OCR Capability"; version = [string]$manifest.version }
        }
        components = $components
    })
    $zipRecord = Get-FileRecord -Path $zipPath -Root $outputRootPath
    $provenancePath = Join-Path $outputRootPath "ocr.provenance.json"
    $gitHead = ((& git -C $repoRoot rev-parse HEAD) -join "").Trim()
    $gitDirty = @(& git -C $repoRoot status --porcelain).Count -gt 0
    Write-Utf8NoBomFile -Path $provenancePath -Value ([ordered]@{
        schemaVersion = 1
        builder = "Loom scripts/Build-LoomOcrCapabilityPackage.ps1"
        qualifiedId = "neuro.official/ocr"
        version = [string]$manifest.version
        target = "windows-x64"
        gitHead = $gitHead
        gitDirty = $gitDirty
        deterministic = $true
        subject = $zipRecord
        materials = $materials
    })
    $diskBytes = [int64](@($materials | ForEach-Object { [int64]$_['bytes'] } | Measure-Object -Sum).Sum)
    $summary = [ordered]@{
        schemaVersion = 1
        capabilities = @([ordered]@{
            id = "ocr"
            qualifiedId = "neuro.official/ocr"
            name = [string]$manifest.name
            description = [string]$manifest.description
            version = [string]$manifest.version
            target = "windows-x64"
            zip = "ocr.zip"
            sidecar = "ocr.zip.sha256"
            bytes = $zipRecord.bytes
            sha256 = $zipHash
            signature = [ordered]@{ algorithm = "ed25519"; keyId = [string]$manifest.signature.keyId }
            sbom = Get-FileRecord -Path $sbomPath -Root $outputRootPath
            provenance = Get-FileRecord -Path $provenancePath -Root $outputRootPath
            deterministic = $true
            diskBytes = $diskBytes
            hostCompatibility = $manifest.hostCompatibility
            permissions = @($manifest.permissions)
        })
    }
    Write-Utf8NoBomFile -Path (Join-Path $outputRootPath "summary.json") -Value $summary
    Write-Output ($summary | ConvertTo-Json -Depth 20)
}
finally {
    foreach ($path in @($stageRoot, $trustPath, $reproZipPath, "$reproZipPath.sha256")) {
        if (Test-Path -LiteralPath $path) {
            Assert-PathInside -Path $path -Root $outputRootPath -Label "OCR package cleanup"
            Assert-LoomPathHasNoReparsePoints -RootPath $outputRootPath -Path $path
            Remove-Item -LiteralPath $path -Recurse -Force
        }
    }
}
