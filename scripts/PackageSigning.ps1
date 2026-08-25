<# Optional release-package signing shared by the framework, MCP, and Art builders. #>

function New-LoomPackageSigningContext {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$KeyPath = $env:LOOM_PACKAGE_SIGNING_KEY_PATH,
        [string]$PublisherId = $env:LOOM_PACKAGE_SIGNING_PUBLISHER_ID
    )

    $hasKey = -not [string]::IsNullOrWhiteSpace($KeyPath)
    $hasPublisher = -not [string]::IsNullOrWhiteSpace($PublisherId)
    if (-not $hasKey -and -not $hasPublisher) {
        return $null
    }
    if (-not $hasKey -or -not $hasPublisher) {
        throw "Package signing requires both LOOM_PACKAGE_SIGNING_KEY_PATH and LOOM_PACKAGE_SIGNING_PUBLISHER_ID."
    }
    $resolvedKey = [System.IO.Path]::GetFullPath($KeyPath)
    if (-not (Test-Path -LiteralPath $resolvedKey -PathType Leaf)) {
        throw "Package signing key was not found: $resolvedKey"
    }
    $plugin = Join-Path $RepoRoot "target\release\loom-plugin.exe"
    if (-not (Test-Path -LiteralPath $plugin -PathType Leaf)) {
        throw "Release package signer was not built: $plugin"
    }
    return [pscustomobject]@{
        Executable = $plugin
        KeyPath = $resolvedKey
        PublisherId = $PublisherId.Trim()
    }
}

function Invoke-LoomPackageSigning {
    param(
        [Parameter(Mandatory = $false)][object]$Context,
        [Parameter(Mandatory = $true)][string]$PackageDirectory
    )

    if ($null -eq $Context) {
        return
    }
    $output = & $Context.Executable sign $PackageDirectory $Context.KeyPath $Context.PublisherId 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to sign package ${PackageDirectory}: $($output -join ' ')"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $PackageDirectory "signature.json") -PathType Leaf)) {
        throw "Package signer did not create signature.json: $PackageDirectory"
    }
}
