[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [string]$ArtId = "custom-image-search",
    [string]$ArtName = "",
    [string]$ServerId = "package-image-search",
    [string]$ServerName = "Package Image Search",
    [string]$BraveApiKey = "",
    [string]$StoreRoot,
    [string]$StoreUrl = "http://127.0.0.1:8790",
    [ValidateSet("store", "upload")][string]$InstallMode = "upload",
    [switch]$SkipInstall,
    [switch]$SkipPublish,
    [switch]$SkipServerConfig
)

$ErrorActionPreference = "Stop"
if ($ArtId -ne "custom-image-search") {
    throw "The pluginized image-search package has a fixed manifest id; install a third-party package for custom ids."
}
$installer = Join-Path $PSScriptRoot "Install-LoomSampleArtPackage.ps1"
$arguments = @("-PackageName", "image-search", "-BaseUrl", $BaseUrl, "-InstallMode", $InstallMode)
if (-not [string]::IsNullOrWhiteSpace($StoreRoot)) { $arguments += @("-StoreRoot", $StoreRoot) }
if (-not [string]::IsNullOrWhiteSpace($StoreUrl)) { $arguments += @("-StoreUrl", $StoreUrl) }
if ($SkipInstall) { $arguments += "-SkipInstall" }
if ($SkipPublish) { $arguments += "-SkipPublish" }
& powershell -NoProfile -ExecutionPolicy Bypass -File $installer @arguments
exit $LASTEXITCODE
