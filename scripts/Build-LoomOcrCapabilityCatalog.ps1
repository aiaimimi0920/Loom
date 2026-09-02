[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageRoot,
    [Parameter(Mandatory = $true)][string]$BaseUrl,
    [string]$SigningKeyPath = $env:LOOM_PACKAGE_SIGNING_KEY_PATH,
    [string]$SigningPublisherId = $env:LOOM_PACKAGE_SIGNING_PUBLISHER_ID,
    [ValidateRange(1, 90)][int]$ExpiresInDays = 30,
    [switch]$AllowHttpLoopback,
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Utf8NoBomJson {
    param([string]$Path, [object]$Value)
    $json = ($Value | ConvertTo-Json -Depth 40) + "`n"
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.UTF8Encoding]::new($false))
}

function Resolve-PackageFile {
    param([string]$Root, [string]$RelativePath)
    if ([string]::IsNullOrWhiteSpace($RelativePath) -or [System.IO.Path]::IsPathRooted($RelativePath)) {
        throw "Catalog artifact path must be package-relative: $RelativePath"
    }
    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    $resolved = [System.IO.Path]::GetFullPath((Join-Path $Root $RelativePath))
    if (-not ($resolved + '\').StartsWith($resolvedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Catalog artifact escaped the package root: $RelativePath"
    }
    return $resolved
}

function Assert-ArtifactRecord {
    param([string]$Path, [object]$Record, [string]$Label)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Label is missing: $Path"
    }
    $file = Get-Item -LiteralPath $Path
    $hash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([int64]$Record.bytes -ne [int64]$file.Length -or [string]$Record.sha256 -cne $hash) {
        throw "$Label does not match summary.json."
    }
}

function Assert-ZipSidecar {
    param([string]$Path, [string]$ExpectedHash, [string]$ExpectedName)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "OCR ZIP checksum sidecar is missing: $Path"
    }
    $fields = @((Get-Content -Raw -Encoding UTF8 -LiteralPath $Path).Trim() -split '\s+')
    if ($fields.Count -ne 2 -or
        -not [string]::Equals($fields[0], $ExpectedHash, [System.StringComparison]::OrdinalIgnoreCase) -or
        [string]$fields[1] -cne $ExpectedName) {
        throw "OCR ZIP checksum sidecar does not match summary.json."
    }
}

function Join-ArtifactUrl {
    param([Uri]$Root, [string]$Name)
    return [Uri]::new($Root, $Name).AbsoluteUri
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")
. (Join-Path $PSScriptRoot "PackageSigning.ps1")
$root = [System.IO.Path]::GetFullPath($PackageRoot)
if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    throw "OCR capability package root is missing: $root"
}
Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $root

$base = [Uri]$BaseUrl
if (-not $base.IsAbsoluteUri -or $base.UserInfo -or $base.Query -or $base.Fragment) {
    throw "BaseUrl must be an absolute artifact directory URL without credentials, query, or fragment."
}
$isLoopback = $base.IsLoopback -and $base.Scheme -eq "http"
if ($base.Scheme -ne "https" -and -not ($AllowHttpLoopback -and $isLoopback)) {
    throw "BaseUrl must use HTTPS; HTTP is allowed only for an explicit loopback test catalog."
}
if (-not $base.AbsoluteUri.EndsWith('/')) {
    $base = [Uri]($base.AbsoluteUri + '/')
}
if ([string]::IsNullOrWhiteSpace($SigningKeyPath) -or $SigningPublisherId -ne "neuro.official") {
    throw "The OCR catalog must be signed by neuro.official with an explicit signing key."
}

$pluginCli = Join-Path $repoRoot "target\release\loom-plugin.exe"
if (-not (Test-Path -LiteralPath $pluginCli -PathType Leaf)) {
    & cargo build --locked --release -p loom-plugin-cli
    if ($LASTEXITCODE -ne 0) { throw "Failed to build loom-plugin." }
}
$signingContext = New-LoomPackageSigningContext `
    -RepoRoot $repoRoot `
    -KeyPath $SigningKeyPath `
    -PublisherId $SigningPublisherId

$summaryPath = Join-Path $root "summary.json"
$manifestPath = Join-Path $repoRoot "capability-packages\ocr\capability.manifest.json"
foreach ($path in @($summaryPath, $manifestPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required catalog input is missing: $path" }
}
$summary = Get-Content -Raw -Encoding UTF8 -LiteralPath $summaryPath | ConvertFrom-Json
$manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
$capability = @($summary.capabilities)
if ($capability.Count -ne 1 -or [string]$capability[0].qualifiedId -cne "neuro.official/ocr") {
    throw "summary.json must describe exactly one neuro.official/ocr package."
}
$capability = $capability[0]
if ([string]$capability.version -cne [string]$manifest.version) {
    throw "OCR package and source manifest versions differ."
}

$zipPath = Resolve-PackageFile -Root $root -RelativePath ([string]$capability.zip)
$sidecarPath = Resolve-PackageFile -Root $root -RelativePath ([string]$capability.sidecar)
$sbomPath = Resolve-PackageFile -Root $root -RelativePath ([string]$capability.sbom.path)
$provenancePath = Resolve-PackageFile -Root $root -RelativePath ([string]$capability.provenance.path)
Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $zipPath
Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $sidecarPath
Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $sbomPath
Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $provenancePath
Assert-ArtifactRecord -Path $zipPath -Record $capability -Label "OCR ZIP"
Assert-ZipSidecar `
    -Path $sidecarPath `
    -ExpectedHash ([string]$capability.sha256) `
    -ExpectedName ([string]$capability.zip)
Assert-ArtifactRecord -Path $sbomPath -Record $capability.sbom -Label "OCR SBOM"
Assert-ArtifactRecord -Path $provenancePath -Record $capability.provenance -Label "OCR provenance"

$provenance = Get-Content -Raw -Encoding UTF8 -LiteralPath $provenancePath | ConvertFrom-Json
$diskBytes = [int64](@($provenance.materials | Measure-Object -Property bytes -Sum).Sum)
if ($diskBytes -le 0) { $diskBytes = [int64]$capability.bytes * 2 }
$generatedAt = [DateTimeOffset]::UtcNow
$payload = [ordered]@{
    schemaVersion = 1
    publisher = [ordered]@{ id = "neuro.official"; keyId = [string]$capability.signature.keyId }
    generatedAt = $generatedAt.ToString("o")
    expiresAt = $generatedAt.AddDays($ExpiresInDays).ToString("o")
    packages = @([ordered]@{
        qualifiedId = "neuro.official/ocr"
        name = [string]$manifest.name
        description = [string]$manifest.description
        version = [string]$manifest.version
        publisher = [ordered]@{ id = "neuro.official"; keyId = [string]$capability.signature.keyId }
        package = [ordered]@{
            url = Join-ArtifactUrl -Root $base -Name ([string]$capability.zip)
            sha256 = [string]$capability.sha256
            bytes = [int64]$capability.bytes
            signature = [ordered]@{
                algorithm = [string]$capability.signature.algorithm
                keyId = [string]$capability.signature.keyId
            }
        }
        sbom = [ordered]@{
            url = Join-ArtifactUrl -Root $base -Name ([string]$capability.sbom.path)
            sha256 = [string]$capability.sbom.sha256
            bytes = [int64]$capability.sbom.bytes
        }
        provenance = [ordered]@{
            url = Join-ArtifactUrl -Root $base -Name ([string]$capability.provenance.path)
            sha256 = [string]$capability.provenance.sha256
            bytes = [int64]$capability.provenance.bytes
        }
        hostCompatibility = $manifest.hostCompatibility
        permissions = @($manifest.permissions)
        diskBytes = $diskBytes
    })
}

$payloadPath = Join-Path $root ".catalog-payload.json"
$catalogPath = Join-Path $root "catalog.json"
$trustPath = Join-Path $root "plugin-trust.json"
$catalogSummaryPath = Join-Path $root "catalog-summary.json"
$catalogSidecarPath = "$catalogPath.sha256"
foreach ($output in @($catalogPath, $trustPath, $catalogSummaryPath, $catalogSidecarPath)) {
    if (Test-Path -LiteralPath $output) {
        if (-not $Force) { throw "Catalog output already exists: $output" }
        Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $output
        Remove-Item -LiteralPath $output -Force
    }
}

try {
    Write-Utf8NoBomJson -Path $payloadPath -Value $payload
    & $signingContext.Executable catalog sign $payloadPath $signingContext.KeyPath $catalogPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to sign the OCR Capability catalog." }
    & $signingContext.Executable trust add $trustPath "neuro.official" $signingContext.KeyPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to create the public OCR catalog trust store." }
    $validation = & $signingContext.Executable catalog validate $catalogPath $trustPath 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Signed OCR catalog validation failed: $($validation -join ' ')" }

    $catalogHash = (Get-FileHash -LiteralPath $catalogPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $catalogName = Split-Path -Leaf $catalogPath
    [System.IO.File]::WriteAllText(
        $catalogSidecarPath,
        "$catalogHash  $catalogName`n",
        [System.Text.UTF8Encoding]::new($false)
    )
    $catalogSummary = [ordered]@{
        schemaVersion = 1
        catalogUrl = Join-ArtifactUrl -Root $base -Name $catalogName
        catalog = [ordered]@{
            path = $catalogName
            bytes = (Get-Item -LiteralPath $catalogPath).Length
            sha256 = $catalogHash
        }
        package = [ordered]@{
            qualifiedId = "neuro.official/ocr"
            version = [string]$capability.version
            bytes = [int64]$capability.bytes
            sha256 = [string]$capability.sha256
        }
        expiresAt = $payload.expiresAt
        trustStore = "plugin-trust.json"
    }
    Write-Utf8NoBomJson -Path $catalogSummaryPath -Value $catalogSummary
    Write-Output ($catalogSummary | ConvertTo-Json -Depth 20)
}
finally {
    if (Test-Path -LiteralPath $payloadPath) {
        Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $payloadPath
        Remove-Item -LiteralPath $payloadPath -Force
    }
}
