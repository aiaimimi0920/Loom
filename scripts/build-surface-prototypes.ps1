param(
    [string]$OutputDir = "",
    [string]$PluginCli = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$sourceRoot = Join-Path $repoRoot "art-packages\surface-prototypes"
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $repoRoot "target\surface-prototypes"
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)

if ([string]::IsNullOrWhiteSpace($PluginCli)) {
    & cargo build -p loom-plugin-cli --manifest-path (Join-Path $repoRoot "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "failed to build loom-plugin" }
    $PluginCli = Join-Path $repoRoot "target\debug\loom-plugin.exe"
}
$PluginCli = [System.IO.Path]::GetFullPath($PluginCli)
if (-not (Test-Path -LiteralPath $PluginCli -PathType Leaf)) {
    throw "loom-plugin executable is missing: $PluginCli"
}

New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$records = @()
foreach ($name in @("stock-card", "dashboard", "form")) {
    $source = Join-Path $sourceRoot $name
    $zip = Join-Path $OutputDir ("surface-prototype-$name.zip")
    $sha = "$zip.sha256"
    Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $sha -Force -ErrorAction SilentlyContinue

    & $PluginCli validate $source
    if ($LASTEXITCODE -ne 0) { throw "Surface prototype validation failed: $name" }
    & $PluginCli pack $source $zip
    if ($LASTEXITCODE -ne 0) { throw "Surface prototype packaging failed: $name" }

    $file = Get-Item -LiteralPath $zip
    $digest = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    $records += [ordered]@{
        id = $name
        file = $file.Name
        bytes = [int64]$file.Length
        sha256 = $digest
    }
}

$catalog = [ordered]@{
    protocolVersion = "loom.surface.v1"
    generatedAt = [DateTimeOffset]::UtcNow.ToString("O")
    packages = @($records)
}
$catalogPath = Join-Path $OutputDir "surface-prototypes.catalog.json"
$catalogJson = $catalog | ConvertTo-Json -Depth 10
[System.IO.File]::WriteAllText(
    $catalogPath,
    $catalogJson + "`n",
    [System.Text.UTF8Encoding]::new($false)
)
Write-Host "Surface prototypes built at $OutputDir"
