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
    [System.IO.File]::WriteAllText(
        $Path,
        (($Value | ConvertTo-Json -Depth 40) + "`n"),
        [System.Text.UTF8Encoding]::new($false)
    )
}

function Resolve-PackageFile {
    param([string]$Root, [string]$RelativePath)
    if ([string]::IsNullOrWhiteSpace($RelativePath) -or [System.IO.Path]::IsPathRooted($RelativePath)) {
        throw "Catalog artifact path must be package-relative: $RelativePath"
    }
    $rootPrefix = [System.IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    $resolved = [System.IO.Path]::GetFullPath((Join-Path $Root $RelativePath))
    if (-not ($resolved + '\').StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Catalog artifact escaped the package root: $RelativePath"
    }
    return $resolved
}

function Assert-ArtifactRecord {
    param([string]$Path, [object]$Record, [string]$Label)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "$Label is missing: $Path" }
    $file = Get-Item -LiteralPath $Path
    $hash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([int64]$Record.bytes -ne [int64]$file.Length -or [string]$Record.sha256 -cne $hash) {
        throw "$Label does not match summary.json."
    }
}

function Assert-ZipSidecar {
    param([string]$Path, [string]$ExpectedHash, [string]$ExpectedName)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "ZIP sidecar is missing: $Path" }
    $fields = @((Get-Content -Raw -Encoding UTF8 -LiteralPath $Path).Trim() -split '\s+')
    if ($fields.Count -ne 2 -or
        -not [string]::Equals($fields[0], $ExpectedHash, [System.StringComparison]::OrdinalIgnoreCase) -or
        [string]$fields[1] -cne $ExpectedName) {
        throw "ZIP sidecar does not match summary.json."
    }
}

function Join-ArtifactUrl {
    param([Uri]$Root, [string]$Name)
    return [Uri]::new($Root, $Name).AbsoluteUri
}

function Expand-And-ValidateCapability {
    param(
        [string]$ZipPath,
        [string]$ValidationRoot,
        [int64]$ExpectedDiskBytes,
        [object]$Capability,
        [string]$ExpectedPublisher,
        [string]$ExpectedKeyId,
        [string]$PluginCli,
        [string]$TrustPath
    )
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($ZipPath)
    $entryNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    $expandedBytes = [int64]0
    try {
        if ($archive.Entries.Count -eq 0 -or $archive.Entries.Count -gt 4096) {
            throw "Capability ZIP contains an invalid number of entries."
        }
        New-Item -ItemType Directory -Path $ValidationRoot | Out-Null
        $validationPrefix = [System.IO.Path]::GetFullPath($ValidationRoot).TrimEnd('\') + '\'
        foreach ($entry in $archive.Entries) {
            $relative = $entry.FullName.Replace('/', '\')
            $segments = @($relative -split '\\')
            if ([string]::IsNullOrWhiteSpace($relative) -or
                [System.IO.Path]::IsPathRooted($relative) -or
                $relative.Contains(':') -or
                $segments -contains '..' -or
                -not $entryNames.Add($relative)) {
                throw "Capability ZIP contains an unsafe or duplicate entry: $($entry.FullName)"
            }
            $destination = [System.IO.Path]::GetFullPath((Join-Path $ValidationRoot $relative))
            if (-not ($destination + '\').StartsWith($validationPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
                throw "Capability ZIP entry escaped its validation root: $($entry.FullName)"
            }
            if ([string]::IsNullOrEmpty($entry.Name)) {
                New-Item -ItemType Directory -Force -Path $destination | Out-Null
                continue
            }
            $expandedBytes += [int64]$entry.Length
            if ($expandedBytes -gt $ExpectedDiskBytes) {
                throw "Capability ZIP expands beyond its declared disk budget."
            }
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
            $source = $entry.Open()
            $target = [System.IO.File]::Open($destination, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
            try { $source.CopyTo($target) } finally { $target.Dispose(); $source.Dispose() }
        }
    }
    finally {
        $archive.Dispose()
    }
    if ($expandedBytes -ne $ExpectedDiskBytes) {
        throw "Capability ZIP expanded size does not match summary.json."
    }
    $validation = & $PluginCli validate $ValidationRoot --trust-store $TrustPath 2>&1
    if ($LASTEXITCODE -ne 0 -or ($validation -join ' ') -notmatch 'trust=Trusted') {
        throw "Capability ZIP failed trusted package validation: $($validation -join ' ')"
    }
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $ValidationRoot "capability.manifest.json") | ConvertFrom-Json
    $signature = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $ValidationRoot "signature.json") | ConvertFrom-Json
    $expectedId = ([string]$Capability.qualifiedId).Substring($ExpectedPublisher.Length + 1)
    if ([string]$manifest.id -cne $expectedId -or
        [string]$manifest.version -cne [string]$Capability.version -or
        [string]$manifest.publisher.id -cne $ExpectedPublisher -or
        [string]$manifest.publisher.keyId -cne $ExpectedKeyId -or
        [string]$manifest.signature.keyId -cne $ExpectedKeyId -or
        [string]$signature.keyId -cne $ExpectedKeyId) {
        throw "Capability ZIP identity or signing key does not match summary.json."
    }
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")
. (Join-Path $PSScriptRoot "PackageSigning.ps1")
$root = [System.IO.Path]::GetFullPath($PackageRoot)
if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    throw "Capability package root is missing: $root"
}
Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $root
$base = [Uri]$BaseUrl
if (-not $base.IsAbsoluteUri -or $base.UserInfo -or $base.Query -or $base.Fragment) {
    throw "BaseUrl must be an absolute artifact directory URL without credentials, query, or fragment."
}
$isLoopback = $base.IsLoopback -and $base.Scheme -eq "http"
if ($base.Scheme -ne "https" -and -not ($AllowHttpLoopback -and $isLoopback)) {
    throw "BaseUrl must use HTTPS; HTTP is allowed only for an explicit loopback catalog."
}
if (-not $base.AbsoluteUri.EndsWith('/')) { $base = [Uri]($base.AbsoluteUri + '/') }
if ([string]::IsNullOrWhiteSpace($SigningKeyPath) -or [string]::IsNullOrWhiteSpace($SigningPublisherId)) {
    throw "Capability catalog requires an explicit signing key and publisher."
}
$pluginCli = Join-Path $repoRoot "target\release\loom-plugin.exe"
& cargo build --locked --release -p loom-plugin-cli
if ($LASTEXITCODE -ne 0) { throw "Failed to build loom-plugin." }
$signingContext = New-LoomPackageSigningContext -RepoRoot $repoRoot -KeyPath $SigningKeyPath -PublisherId $SigningPublisherId
$signingKey = Get-Content -Raw -Encoding UTF8 -LiteralPath $signingContext.KeyPath | ConvertFrom-Json
$signingKeyId = [string]$signingKey.keyId
if ([string]::IsNullOrWhiteSpace($signingKeyId)) { throw "Capability signing key has no keyId." }
$payloadPath = Join-Path $root ".catalog-payload.json"
$catalogPath = Join-Path $root "catalog.json"
$trustPath = Join-Path $root "plugin-trust.json"
$catalogSummaryPath = Join-Path $root "catalog-summary.json"
$catalogSidecarPath = "$catalogPath.sha256"
foreach ($output in @($payloadPath, $catalogPath, $trustPath, $catalogSummaryPath, $catalogSidecarPath)) {
    if (Test-Path -LiteralPath $output) {
        if (-not $Force) { throw "Catalog output already exists: $output" }
        Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $output
        Remove-Item -LiteralPath $output -Force
    }
}
& $pluginCli trust add $trustPath $SigningPublisherId $signingContext.KeyPath | Out-Null
if ($LASTEXITCODE -ne 0) { throw "Failed to create catalog trust store." }
$summaryPath = Join-Path $root "summary.json"
if (-not (Test-Path -LiteralPath $summaryPath -PathType Leaf)) {
    throw "Capability summary is missing: $summaryPath"
}
$summary = Get-Content -Raw -Encoding UTF8 -LiteralPath $summaryPath | ConvertFrom-Json
$capabilities = @($summary.capabilities)
if ($capabilities.Count -eq 0 -or $capabilities.Count -gt 128) {
    throw "Capability summary must contain between 1 and 128 packages."
}
$seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$catalogPackages = @()
foreach ($capability in @($capabilities | Sort-Object qualifiedId)) {
    $qualifiedId = [string]$capability.qualifiedId
    if (-not $qualifiedId.StartsWith("$SigningPublisherId/", [System.StringComparison]::Ordinal) -or
        -not $seen.Add($qualifiedId)) {
        throw "Capability summary contains an invalid or duplicate publisher identity: $qualifiedId"
    }
    foreach ($field in @("name", "description", "version", "zip", "sidecar")) {
        if ([string]::IsNullOrWhiteSpace([string]$capability.$field)) {
            throw "Capability $qualifiedId is missing $field."
        }
    }
    if ([string]$capability.signature.algorithm -cne "ed25519" -or
        [string]$capability.signature.keyId -cne $signingKeyId) {
        throw "Capability $qualifiedId was not signed by the catalog signing key."
    }
    $zipPath = Resolve-PackageFile -Root $root -RelativePath ([string]$capability.zip)
    $sidecarPath = Resolve-PackageFile -Root $root -RelativePath ([string]$capability.sidecar)
    $sbomPath = Resolve-PackageFile -Root $root -RelativePath ([string]$capability.sbom.path)
    $provenancePath = Resolve-PackageFile -Root $root -RelativePath ([string]$capability.provenance.path)
    foreach ($path in @($zipPath, $sidecarPath, $sbomPath, $provenancePath)) {
        Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $path
    }
    Assert-ArtifactRecord -Path $zipPath -Record $capability -Label "$qualifiedId ZIP"
    Assert-ZipSidecar -Path $sidecarPath -ExpectedHash ([string]$capability.sha256) -ExpectedName ([string]$capability.zip)
    Assert-ArtifactRecord -Path $sbomPath -Record $capability.sbom -Label "$qualifiedId SBOM"
    Assert-ArtifactRecord -Path $provenancePath -Record $capability.provenance -Label "$qualifiedId provenance"
    $provenance = Get-Content -Raw -Encoding UTF8 -LiteralPath $provenancePath | ConvertFrom-Json
    if ([string]$provenance.qualifiedId -cne $qualifiedId -or
        [string]$provenance.subject.sha256 -cne [string]$capability.sha256 -or
        $provenance.deterministic -ne $true) {
        throw "Capability provenance identity or deterministic evidence is invalid: $qualifiedId"
    }
    $validationRoot = Join-Path $root (".catalog-validation-" + [Guid]::NewGuid().ToString("N"))
    try {
        Expand-And-ValidateCapability -ZipPath $zipPath -ValidationRoot $validationRoot `
            -ExpectedDiskBytes ([int64]$capability.diskBytes) -Capability $capability `
            -ExpectedPublisher $SigningPublisherId -ExpectedKeyId $signingKeyId `
            -PluginCli $pluginCli -TrustPath $trustPath
    }
    finally {
        if (Test-Path -LiteralPath $validationRoot) {
            Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $validationRoot
            Remove-Item -LiteralPath $validationRoot -Recurse -Force
        }
    }
    $catalogPackages += [ordered]@{
        qualifiedId = $qualifiedId
        name = [string]$capability.name
        description = [string]$capability.description
        version = [string]$capability.version
        publisher = [ordered]@{ id = $SigningPublisherId; keyId = [string]$capability.signature.keyId }
        package = [ordered]@{
            url = Join-ArtifactUrl -Root $base -Name ([string]$capability.zip)
            sha256 = [string]$capability.sha256
            bytes = [int64]$capability.bytes
            signature = $capability.signature
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
        hostCompatibility = $capability.hostCompatibility
        permissions = @($capability.permissions)
        diskBytes = [int64]$capability.diskBytes
    }
}

$generatedAt = [DateTimeOffset]::UtcNow
$payload = [ordered]@{
    schemaVersion = 1
    publisher = [ordered]@{ id = $SigningPublisherId; keyId = $signingKeyId }
    generatedAt = $generatedAt.ToString("o")
    expiresAt = $generatedAt.AddDays($ExpiresInDays).ToString("o")
    packages = $catalogPackages
}
try {
    Write-Utf8NoBomJson -Path $payloadPath -Value $payload
    & $pluginCli catalog sign $payloadPath $signingContext.KeyPath $catalogPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to sign capability catalog." }
    & $pluginCli catalog validate $catalogPath $trustPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Signed capability catalog validation failed." }
    $catalogHash = (Get-FileHash -LiteralPath $catalogPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText(
        $catalogSidecarPath,
        "$catalogHash  catalog.json`n",
        [System.Text.UTF8Encoding]::new($false)
    )
    Write-Utf8NoBomJson -Path $catalogSummaryPath -Value ([ordered]@{
        schemaVersion = 1
        catalogUrl = Join-ArtifactUrl -Root $base -Name "catalog.json"
        catalog = [ordered]@{
            path = "catalog.json"
            bytes = (Get-Item -LiteralPath $catalogPath).Length
            sha256 = $catalogHash
        }
        packages = @($capabilities | ForEach-Object {
            [ordered]@{ qualifiedId = [string]$_.qualifiedId; version = [string]$_.version; bytes = [int64]$_.bytes; sha256 = [string]$_.sha256 }
        })
        expiresAt = $payload.expiresAt
        trustStore = "plugin-trust.json"
    })
    Get-Content -Raw -Encoding UTF8 -LiteralPath $catalogSummaryPath
}
finally {
    if (Test-Path -LiteralPath $payloadPath) {
        Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $payloadPath
        Remove-Item -LiteralPath $payloadPath -Force
    }
}
