[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][string]$Version
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$outputRoot = [System.IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null

function Write-Utf8NoBom {
    param([string]$Path, [string]$Value)
    [System.IO.File]::WriteAllText($Path, $Value, [System.Text.UTF8Encoding]::new($false))
}

function Get-Purl {
    param([string]$Type, [string]$Name, [string]$PackageVersion)
    $escapedName = [Uri]::EscapeDataString($Name)
    $escapedVersion = [Uri]::EscapeDataString($PackageVersion)
    return "pkg:${Type}/${escapedName}@${escapedVersion}"
}

$componentsByRef = [ordered]@{}
$cargoLockPath = Join-Path $repoRoot "Cargo.lock"
$cargoLock = Get-Content -Raw -Encoding UTF8 -LiteralPath $cargoLockPath
$cargoPackages = [regex]::Matches(
    $cargoLock,
    '(?ms)^\[\[package\]\]\s+name\s*=\s*"([^"]+)"\s+version\s*=\s*"([^"]+)"'
)
foreach ($match in $cargoPackages) {
    $name = [string]$match.Groups[1].Value
    $packageVersion = [string]$match.Groups[2].Value
    $purl = Get-Purl -Type "cargo" -Name $name -PackageVersion $packageVersion
    $componentsByRef[$purl] = [ordered]@{
        type = "library"
        name = $name
        version = $packageVersion
        purl = $purl
        "bom-ref" = $purl
    }
}

$packageLockPath = Join-Path $repoRoot "apps\desktop\package-lock.json"
if (Test-Path -LiteralPath $packageLockPath -PathType Leaf) {
    $previousPackageLock = $env:LOOM_SBOM_PACKAGE_LOCK
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $env:LOOM_SBOM_PACKAGE_LOCK = $packageLockPath
        $ErrorActionPreference = "Continue"
        $nodeOutput = & node -e "const fs=require('fs');const p=JSON.parse(fs.readFileSync(process.env.LOOM_SBOM_PACKAGE_LOCK,'utf8'));const out=Object.values(p.packages||{}).filter(x=>x&&x.name&&x.version).map(x=>({name:x.name,version:x.version}));process.stdout.write(JSON.stringify(out));" 2>$null
        $nodeExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
        $env:LOOM_SBOM_PACKAGE_LOCK = $previousPackageLock
    }
    if ($nodeExitCode -ne 0) {
        throw "Node.js failed to parse the desktop package lock for the SBOM."
    }
    $npmPackages = (($nodeOutput | ForEach-Object { $_.ToString() }) -join "`n") | ConvertFrom-Json
    foreach ($entry in @($npmPackages)) {
        $name = [string]$entry.name
        $packageVersion = [string]$entry.version
        if ([string]::IsNullOrWhiteSpace($name) -or [string]::IsNullOrWhiteSpace($packageVersion)) {
            continue
        }
        $purl = Get-Purl -Type "npm" -Name $name -PackageVersion $packageVersion
        if (-not $componentsByRef.Contains($purl)) {
            $componentsByRef[$purl] = [ordered]@{
                type = "library"
                name = $name
                version = $packageVersion
                purl = $purl
                "bom-ref" = $purl
            }
        }
    }
}

$stockApiRoot = Join-Path $repoRoot "mcp-server-packages\stock-api\runtime"
$stockApiMetadataPath = Join-Path $stockApiRoot "UPSTREAM.json"
$pysnowballMetadataPath = Join-Path $stockApiRoot "PYSNOWBALL.json"
$nodeMetadataPath = Join-Path $stockApiRoot "node-runtime.json"
if (-not (Test-Path -LiteralPath $stockApiMetadataPath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $pysnowballMetadataPath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $nodeMetadataPath -PathType Leaf)) {
    throw "Stock API supply-chain metadata is required for the Loom SBOM."
}
$stockApiMetadata = Get-Content -Raw -Encoding UTF8 -LiteralPath $stockApiMetadataPath | ConvertFrom-Json
$stockApiPurl = Get-Purl -Type "npm" -Name ([string]$stockApiMetadata.name) -PackageVersion ([string]$stockApiMetadata.version)
$componentsByRef[$stockApiPurl] = [ordered]@{
    type = "library"
    name = [string]$stockApiMetadata.name
    version = [string]$stockApiMetadata.version
    purl = $stockApiPurl
    "bom-ref" = $stockApiPurl
    licenses = @([ordered]@{ license = [ordered]@{ id = [string]$stockApiMetadata.license } })
}
$pysnowballMetadata = Get-Content -Raw -Encoding UTF8 -LiteralPath $pysnowballMetadataPath | ConvertFrom-Json
$pysnowballPurl = Get-Purl -Type "pypi" -Name ([string]$pysnowballMetadata.name) -PackageVersion ([string]$pysnowballMetadata.version)
$componentsByRef[$pysnowballPurl] = [ordered]@{
    type = "library"
    name = [string]$pysnowballMetadata.name
    version = [string]$pysnowballMetadata.version
    purl = $pysnowballPurl
    "bom-ref" = $pysnowballPurl
    licenses = @([ordered]@{ license = [ordered]@{ id = [string]$pysnowballMetadata.license } })
}
$nodeMetadata = Get-Content -Raw -Encoding UTF8 -LiteralPath $nodeMetadataPath | ConvertFrom-Json
$nodePurl = Get-Purl -Type "generic" -Name "nodejs" -PackageVersion ([string]$nodeMetadata.version)
$componentsByRef[$nodePurl] = [ordered]@{
    type = "application"
    name = "nodejs"
    version = [string]$nodeMetadata.version
    purl = $nodePurl
    "bom-ref" = $nodePurl
}

$components = @($componentsByRef.Values | Sort-Object { [string]$_['bom-ref'] })
$serial = "urn:uuid:$([Guid]::NewGuid())"
$timestamp = (Get-Date).ToUniversalTime().ToString("o")
$cycloneDx = [ordered]@{
    bomFormat = "CycloneDX"
    specVersion = "1.6"
    serialNumber = $serial
    version = 1
    metadata = [ordered]@{
        timestamp = $timestamp
        tools = @([ordered]@{
            vendor = "Neuro"
            name = "New-LoomSbom.ps1"
            version = "1"
        })
        component = [ordered]@{
            type = "application"
            name = "Loom"
            version = $Version
            "bom-ref" = "pkg:generic/loom@$([Uri]::EscapeDataString($Version))"
        }
    }
    components = $components
}

$spdxPackages = @()
$index = 0
foreach ($component in $components) {
    $index++
    $declaredLicense = "NOASSERTION"
    $componentLicenses = @()
    if ($component -is [System.Collections.IDictionary] -and $component.Contains("licenses")) {
        $componentLicenses = @($component["licenses"])
    }
    elseif ($null -ne $component.PSObject.Properties["licenses"]) {
        $componentLicenses = @($component.licenses)
    }
    if ($componentLicenses.Count -gt 0) {
        $declaredLicense = [string]$componentLicenses[0].license.id
    }
    $spdxPackages += [ordered]@{
        SPDXID = "SPDXRef-Package-$index"
        name = [string]$component.name
        versionInfo = [string]$component.version
        downloadLocation = "NOASSERTION"
        filesAnalyzed = $false
        licenseConcluded = $declaredLicense
        licenseDeclared = $declaredLicense
        copyrightText = "NOASSERTION"
        externalRefs = @([ordered]@{
            referenceCategory = "PACKAGE-MANAGER"
            referenceType = "purl"
            referenceLocator = [string]$component.purl
        })
    }
}
$spdx = [ordered]@{
    spdxVersion = "SPDX-2.3"
    dataLicense = "CC0-1.0"
    SPDXID = "SPDXRef-DOCUMENT"
    name = "Loom-$Version"
    documentNamespace = "https://github.com/aiaimimi0920/Loom/sbom/$([Guid]::NewGuid())"
    creationInfo = [ordered]@{
        created = $timestamp
        creators = @("Tool: New-LoomSbom.ps1-1")
    }
    packages = $spdxPackages
}

$cyclonePath = Join-Path $outputRoot "Loom-$Version.cdx.json"
$spdxPath = Join-Path $outputRoot "Loom-$Version.spdx.json"
Write-Utf8NoBom -Path $cyclonePath -Value (($cycloneDx | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
Write-Utf8NoBom -Path $spdxPath -Value (($spdx | ConvertTo-Json -Depth 20) + [Environment]::NewLine)

[ordered]@{
    schemaVersion = 1
    componentCount = $components.Count
    cycloneDx = $cyclonePath
    spdx = $spdxPath
} | ConvertTo-Json -Depth 5
