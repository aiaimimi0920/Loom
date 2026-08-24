<# Owns offline MCP and Art package fixture transformations for the install smoke. #>

function New-ImageSearchFixtureMcpPackage {
    param(
        [Parameter(Mandatory = $true)][string]$SourceZip,
        [Parameter(Mandatory = $true)][string]$DestinationZip,
        [Parameter(Mandatory = $true)][string]$WorkRoot,
        [Parameter(Mandatory = $true)][string]$Endpoint
    )

    $stage = Join-Path $WorkRoot "image-search-mcp-fixture"
    Expand-Archive -LiteralPath $SourceZip -DestinationPath $stage -Force
    $serverPath = Join-Path $stage "runtime\image-search-mcp.ps1"
    Assert-True (Test-Path -LiteralPath $serverPath -PathType Leaf) "Independent image-search MCP server is missing: $serverPath"

    # A package manifest cannot select the credential-bearing endpoint. The wrapper only accepts loopback.
    $fixtureEntryPath = Join-Path $stage "runtime\image-search-fixture.ps1"
    $fixtureEntry = @'
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$EndpointOverride)

$ErrorActionPreference = "Stop"
$uri = [Uri]$EndpointOverride
if ($uri.Scheme -ne "http" -or $uri.Host -notin @("127.0.0.1", "::1") -or -not [string]::IsNullOrEmpty($uri.UserInfo) -or -not [string]::IsNullOrEmpty($uri.Query) -or -not [string]::IsNullOrEmpty($uri.Fragment)) {
    throw "EndpointOverride must be an unauthenticated loopback HTTP URL"
}
$env:LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE = $EndpointOverride
& (Join-Path $PSScriptRoot "image-search-mcp.ps1")
exit $LASTEXITCODE
'@
    Write-Utf8NoBomFile -Path $fixtureEntryPath -Content ($fixtureEntry + "`n")

    $manifestPath = Join-Path $stage "mcp.server.json"
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
    Assert-True ([string]$manifest.entry.command -eq "runtime/image-search-mcp.ps1") "Image-search fixture must preserve the independent MCP entry."
    $manifest.entry.command = "runtime/image-search-fixture.ps1"
    $manifest.entry.args = @("-EndpointOverride", $Endpoint)
    Write-Utf8NoBomFile -Path $manifestPath -Content (($manifest | ConvertTo-Json -Depth 40) + "`n")
    if (Test-Path -LiteralPath $DestinationZip) {
        Remove-Item -LiteralPath $DestinationZip -Force
    }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $DestinationZip -CompressionLevel Optimal -Force
}

function New-LoopbackImageSearchArtFixturePackage {
    param(
        [Parameter(Mandatory = $true)][string]$SourceZip,
        [Parameter(Mandatory = $true)][string]$DestinationZip,
        [Parameter(Mandatory = $true)][string]$WorkRoot
    )

    $stage = Join-Path $WorkRoot "image-search-art-fixture"
    Expand-Archive -LiteralPath $SourceZip -DestinationPath $stage -Force
    $manifestPath = Join-Path $stage "manifest.json"
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
    $permissionPolicy = [pscustomobject]@{
        network = [pscustomobject]@{ allowLocalhost = $true }
    }
    $manifest.metadata | Add-Member -MemberType NoteProperty -Name permissionPolicy -Value $permissionPolicy -Force
    Write-Utf8NoBomFile -Path $manifestPath -Content (($manifest | ConvertTo-Json -Depth 40) + "`n")
    $runtimePath = Join-Path $stage "runtime\main.ps1"
    $runtimeImplementationPath = Join-Path $stage "runtime\main.fixture.ps1"
    Move-Item -LiteralPath $runtimePath -Destination $runtimeImplementationPath
    $runtimeWrapper = @'
if ($env:LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES -ne "1") {
    throw "image-search loopback fixture seam was not inherited"
}
& (Join-Path $PSScriptRoot "main.fixture.ps1")
exit $LASTEXITCODE
'@
    Write-Utf8NoBomFile -Path $runtimePath -Content ($runtimeWrapper + "`n")
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $DestinationZip -CompressionLevel Optimal -Force
}

function New-StockApiFixtureMcpPackage {
    param(
        [Parameter(Mandatory = $true)][string]$SourceZip,
        [Parameter(Mandatory = $true)][string]$DestinationZip,
        [Parameter(Mandatory = $true)][string]$WorkRoot,
        [Parameter(Mandatory = $true)][string]$TestBaseUrl
    )

    $stage = Join-Path $WorkRoot "stock-api-mcp-fixture"
    Expand-Archive -LiteralPath $SourceZip -DestinationPath $stage -Force
    $serverPath = Join-Path $stage "runtime\stock-api-mcp.ps1"
    Assert-True (Test-Path -LiteralPath $serverPath -PathType Leaf) "Independent stock-api MCP server is missing: $serverPath"

    $fixtureEntryPath = Join-Path $stage "runtime\stock-api-fixture.ps1"
    $fixtureEntry = @'
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$TestBaseUrl)

$ErrorActionPreference = "Stop"
$uri = [Uri]$TestBaseUrl
if ($uri.Scheme -ne "http" -or $uri.Host -notin @("127.0.0.1", "::1") -or -not [string]::IsNullOrEmpty($uri.UserInfo) -or -not [string]::IsNullOrEmpty($uri.Query) -or -not [string]::IsNullOrEmpty($uri.Fragment)) {
    throw "TestBaseUrl must be an unauthenticated loopback HTTP URL"
}
$env:LOOM_STOCK_API_TEST_BASE_URL = $TestBaseUrl
& (Join-Path $PSScriptRoot "stock-api-mcp.ps1")
exit $LASTEXITCODE
'@
    Write-Utf8NoBomFile -Path $fixtureEntryPath -Content ($fixtureEntry + "`n")

    $manifestPath = Join-Path $stage "mcp.server.json"
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
    Assert-True ([string]$manifest.entry.command -eq "runtime/stock-api-mcp.ps1") "Stock API fixture must preserve the independent MCP entry."
    $manifest.entry.command = "runtime/stock-api-fixture.ps1"
    $manifest.entry.args = @("-TestBaseUrl", $TestBaseUrl)
    Write-Utf8NoBomFile -Path $manifestPath -Content (($manifest | ConvertTo-Json -Depth 40) + "`n")
    if (Test-Path -LiteralPath $DestinationZip) {
        Remove-Item -LiteralPath $DestinationZip -Force
    }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $DestinationZip -CompressionLevel Optimal -Force
}
