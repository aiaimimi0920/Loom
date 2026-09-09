[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageSourceRoot,
    [Parameter(Mandatory = $true)][string]$RuntimeCargoPackage,
    [Parameter(Mandatory = $true)][string]$RuntimeBinaryName,
    [Parameter(Mandatory = $true)][string]$OutputRoot,
    [ValidateSet("Debug", "Release")][string]$Configuration = "Release",
    [string]$SigningKeyPath = $env:LOOM_PACKAGE_SIGNING_KEY_PATH,
    [string]$SigningPublisherId = $env:LOOM_PACKAGE_SIGNING_PUBLISHER_ID
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

function Assert-PathInside {
    param([string]$Path, [string]$Root, [string]$Label)
    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    if (-not ($resolvedPath + '\').StartsWith($resolvedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
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

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "PackageSigning.ps1")
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")
$capabilitySourceRoot = Join-Path $repoRoot "capability-packages"
$sourceRoot = if ([System.IO.Path]::IsPathRooted($PackageSourceRoot)) {
    [System.IO.Path]::GetFullPath($PackageSourceRoot)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $PackageSourceRoot))
}
$outputRootPath = if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    [System.IO.Path]::GetFullPath($OutputRoot)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputRoot))
}
Assert-PathInside -Path $sourceRoot -Root $capabilitySourceRoot -Label "Capability source"
if (-not (Test-Path -LiteralPath $sourceRoot -PathType Container)) {
    throw "Capability source root is missing: $sourceRoot"
}
Assert-LoomPathHasNoReparsePoints -RootPath $capabilitySourceRoot -Path $sourceRoot
$manifestPath = Join-Path $sourceRoot "capability.manifest.json"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Capability manifest is missing: $manifestPath"
}
$manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
$packageId = [string]$manifest.id
$publisherId = [string]$manifest.publisher.id
$reservedBase = @($packageId -split '\.', 2)[0].ToUpperInvariant()
$reserved = $reservedBase -match '^(CON|PRN|AUX|NUL)$' -or $reservedBase -match '^(COM|LPT)0*[1-9]$'
$runtimeReserved = $RuntimeBinaryName.ToUpperInvariant() -match '^(CON|PRN|AUX|NUL|(COM|LPT)0*[1-9])$'
if ($packageId -notmatch '^[a-z0-9](?:[a-z0-9_-]*[a-z0-9])?$' -or
    $RuntimeBinaryName -notmatch '^[a-z0-9](?:[a-z0-9_-]*[a-z0-9])?$' -or $reserved -or $runtimeReserved) {
    throw "Capability package or runtime binary id is unsafe."
}
if ([string]::IsNullOrWhiteSpace($SigningKeyPath) -or $SigningPublisherId -cne $publisherId) {
    throw "Capability package must be signed by its manifest publisher: $publisherId"
}
$entryCommand = [string]$manifest.entrypoints.service.targets.'windows-x64'.command
if ($entryCommand -cne "runtime/$RuntimeBinaryName.exe") {
    throw "Capability manifest entry does not match runtime/$RuntimeBinaryName.exe."
}

$stageRoot = Join-Path $outputRootPath ".staging-$packageId"
$trustPath = Join-Path $outputRootPath ".signing-trust.json"
$zipPath = Join-Path $outputRootPath "$packageId.zip"
$reproZipPath = Join-Path $outputRootPath ".$packageId-repro.zip"
$sbomPath = Join-Path $outputRootPath "$packageId.cdx.json"
$provenancePath = Join-Path $outputRootPath "$packageId.provenance.json"
$summaryPath = Join-Path $outputRootPath "summary.json"
$sourcePrefix = $sourceRoot.TrimEnd('\') + '\'
if (($outputRootPath + '\').StartsWith($sourcePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Capability output root must not be inside its source package."
}
New-Item -ItemType Directory -Force -Path $outputRootPath | Out-Null
Assert-LoomPathHasNoReparsePoints -RootPath $outputRootPath -Path $outputRootPath
foreach ($path in @($stageRoot, $trustPath, $zipPath, "$zipPath.sha256", $reproZipPath, "$reproZipPath.sha256", $sbomPath, $provenancePath, $summaryPath)) {
    if (Test-Path -LiteralPath $path) {
        Assert-PathInside -Path $path -Root $outputRootPath -Label "Capability output cleanup"
        Assert-LoomPathHasNoReparsePoints -RootPath $outputRootPath -Path $path
        Remove-Item -LiteralPath $path -Recurse -Force
    }
}

try {
    $cargoArguments = @("build", "--locked", "-p", $RuntimeCargoPackage)
    if ($Configuration -eq "Release") { $cargoArguments += "--release" }
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Failed to build $RuntimeCargoPackage." }
    & cargo build --locked --release -p loom-plugin-cli
    if ($LASTEXITCODE -ne 0) { throw "Failed to build loom-plugin." }
    $pluginCli = Join-Path $repoRoot "target\release\loom-plugin.exe"
    $signingContext = New-LoomPackageSigningContext -RepoRoot $repoRoot -KeyPath $SigningKeyPath -PublisherId $SigningPublisherId
    $profile = if ($Configuration -eq "Release") { "release" } else { "debug" }
    $runtimePath = Join-Path $repoRoot "target\$profile\$RuntimeBinaryName.exe"
    if (-not (Test-Path -LiteralPath $runtimePath -PathType Leaf)) {
        throw "Capability runtime build output is missing: $runtimePath"
    }

    foreach ($file in @(Get-ChildItem -LiteralPath $sourceRoot -Recurse -File)) {
        Assert-LoomPathHasNoReparsePoints -RootPath $sourceRoot -Path $file.FullName
        $relative = $file.FullName.Substring($sourceRoot.TrimEnd('\').Length + 1)
        if ($relative -ieq "signature.json") { throw "Capability source must not contain a generated signature." }
        $destination = Join-Path $stageRoot $relative
        Assert-PathInside -Path $destination -Root $stageRoot -Label "Capability staging file"
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $destination
    }
    $stagedRuntime = Join-Path $stageRoot "runtime\$RuntimeBinaryName.exe"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $stagedRuntime) | Out-Null
    Copy-Item -LiteralPath $runtimePath -Destination $stagedRuntime

    Invoke-LoomPackageSigning -Context $signingContext -PackageDirectory $stageRoot
    & $pluginCli trust add $trustPath $publisherId $signingContext.KeyPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to create package trust store." }
    $validation = & $pluginCli validate $stageRoot --trust-store $trustPath 2>&1
    if ($LASTEXITCODE -ne 0 -or ($validation -join " ") -notmatch 'trust=Trusted') {
        throw "Signed capability failed trusted validation: $($validation -join ' ')"
    }
    & $pluginCli pack $stageRoot $zipPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to pack capability." }
    & $pluginCli pack $stageRoot $reproZipPath | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Failed to reproduce capability package." }
    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $reproHash = (Get-FileHash -LiteralPath $reproZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($zipHash -cne $reproHash) { throw "Capability packaging is not deterministic." }
    [System.IO.File]::WriteAllText(
        "$zipPath.sha256",
        "$zipHash  $packageId.zip`n",
        [System.Text.UTF8Encoding]::new($false)
    )

    $signedManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $stageRoot "capability.manifest.json") | ConvertFrom-Json
    $materials = @(Get-ChildItem -LiteralPath $stageRoot -Recurse -File | ForEach-Object {
        Get-FileRecord -Path $_.FullName -Root $stageRoot
    } | Sort-Object path)
    $components = @($materials | ForEach-Object {
        [ordered]@{ type = "file"; name = $_.path; hashes = @([ordered]@{ alg = "SHA-256"; content = $_.sha256 }) }
    })
    Write-Utf8NoBomJson -Path $sbomPath -Value ([ordered]@{
        bomFormat = "CycloneDX"
        specVersion = "1.6"
        version = 1
        metadata = [ordered]@{ component = [ordered]@{ type = "application"; name = [string]$signedManifest.name; version = [string]$signedManifest.version } }
        components = $components
    })
    $zipRecord = Get-FileRecord -Path $zipPath -Root $outputRootPath
    Write-Utf8NoBomJson -Path $provenancePath -Value ([ordered]@{
        schemaVersion = 1
        builder = "Loom scripts/Build-LoomCapabilityPackage.ps1"
        qualifiedId = "$publisherId/$packageId"
        version = [string]$signedManifest.version
        target = "windows-x64"
        gitHead = ((& git -C $repoRoot rev-parse HEAD) -join "").Trim()
        gitDirty = @(& git -C $repoRoot status --porcelain).Count -gt 0
        deterministic = $true
        subject = $zipRecord
        materials = $materials
    })
    $diskBytes = [int64](@($materials | ForEach-Object { [int64]$_['bytes'] } | Measure-Object -Sum).Sum)
    $summary = [ordered]@{
        schemaVersion = 1
        capabilities = @([ordered]@{
            id = $packageId
            qualifiedId = "$publisherId/$packageId"
            name = [string]$signedManifest.name
            description = [string]$signedManifest.description
            version = [string]$signedManifest.version
            target = "windows-x64"
            zip = "$packageId.zip"
            sidecar = "$packageId.zip.sha256"
            bytes = $zipRecord.bytes
            sha256 = $zipHash
            signature = [ordered]@{ algorithm = "ed25519"; keyId = [string]$signedManifest.signature.keyId }
            sbom = Get-FileRecord -Path $sbomPath -Root $outputRootPath
            provenance = Get-FileRecord -Path $provenancePath -Root $outputRootPath
            deterministic = $true
            diskBytes = $diskBytes
            hostCompatibility = $signedManifest.hostCompatibility
            permissions = @($signedManifest.permissions)
        })
    }
    Write-Utf8NoBomJson -Path $summaryPath -Value $summary
    Write-Output ($summary | ConvertTo-Json -Depth 30)
}
finally {
    foreach ($path in @($stageRoot, $trustPath, $reproZipPath, "$reproZipPath.sha256")) {
        if (Test-Path -LiteralPath $path) {
            Assert-PathInside -Path $path -Root $outputRootPath -Label "Capability package cleanup"
            Assert-LoomPathHasNoReparsePoints -RootPath $outputRootPath -Path $path
            Remove-Item -LiteralPath $path -Recurse -Force
        }
    }
}
