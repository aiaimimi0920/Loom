[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$LoomPackageDir,
    [Parameter(Mandatory = $true)][string]$CatalogRoot,
    [string]$ControlPlaneRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-HttpResponse {
    param(
        [System.Net.Sockets.NetworkStream]$Stream,
        [int]$Status,
        [string]$Reason,
        [string]$ContentType,
        [int64]$Length
    )
    $header = "HTTP/1.1 $Status $Reason`r`nContent-Type: $ContentType`r`nContent-Length: $Length`r`nConnection: close`r`nX-Content-Type-Options: nosniff`r`n`r`n"
    $bytes = [System.Text.Encoding]::ASCII.GetBytes($header)
    $Stream.Write($bytes, 0, $bytes.Length)
}

function Send-HttpError {
    param([System.Net.Sockets.NetworkStream]$Stream, [int]$Status, [string]$Reason)
    $body = [System.Text.Encoding]::UTF8.GetBytes("$Status $Reason`n")
    Write-HttpResponse -Stream $Stream -Status $Status -Reason $Reason -ContentType "text/plain; charset=utf-8" -Length $body.Length
    $Stream.Write($body, 0, $body.Length)
}

function Read-HttpRequestHead {
    param([System.Net.Sockets.NetworkStream]$Stream)
    $buffer = New-Object byte[] 1
    $bytes = New-Object System.Collections.Generic.List[byte]
    $tail = ""
    while ($bytes.Count -lt 16384) {
        $count = $Stream.Read($buffer, 0, 1)
        if ($count -eq 0) { break }
        $bytes.Add($buffer[0])
        $tail = ($tail + [char]$buffer[0])
        if ($tail.Length -gt 4) { $tail = $tail.Substring($tail.Length - 4) }
        if ($tail -eq "`r`n`r`n") { return [System.Text.Encoding]::ASCII.GetString($bytes.ToArray()) }
    }
    throw "HTTP request headers exceeded the local catalog limit or were incomplete."
}

function Serve-Client {
    param(
        [System.Net.Sockets.TcpClient]$Client,
        [hashtable]$AllowedFiles,
        [string]$TrustedRoot
    )
    try {
        $Client.ReceiveTimeout = 3000
        $Client.SendTimeout = 30000
        $stream = $Client.GetStream()
        $head = Read-HttpRequestHead -Stream $stream
        $requestLine = ($head -split "`r`n", 2)[0]
        if ($requestLine -notmatch '^(GET|HEAD) /([A-Za-z0-9._-]+) HTTP/1\.[01]$') {
            Send-HttpError -Stream $stream -Status 400 -Reason "Bad Request"
            return
        }
        $method = $Matches[1]
        $name = $Matches[2]
        if (-not $AllowedFiles.ContainsKey($name)) {
            Send-HttpError -Stream $stream -Status 404 -Reason "Not Found"
            return
        }
        $record = $AllowedFiles[$name]
        $path = [string]$record.Path
        Assert-LoomPathHasNoReparsePoints -RootPath $TrustedRoot -Path $path
        $file = Get-Item -LiteralPath $path
        Write-HttpResponse -Stream $stream -Status 200 -Reason "OK" -ContentType ([string]$record.ContentType) -Length $file.Length
        if ($method -eq "GET") {
            $input = [System.IO.File]::Open($path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::Read)
            try { $input.CopyTo($stream, 65536) } finally { $input.Dispose() }
        }
    }
    catch {
        try { Send-HttpError -Stream $Client.GetStream() -Status 500 -Reason "Internal Server Error" } catch { }
    }
    finally {
        $Client.Dispose()
    }
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")
$packageRoot = [System.IO.Path]::GetFullPath($LoomPackageDir)
$catalogPath = [System.IO.Path]::GetFullPath($CatalogRoot)
$loomExe = Join-Path $packageRoot "Loom.exe"
$trustSource = Join-Path $catalogPath "plugin-trust.json"
$catalogFile = Join-Path $catalogPath "catalog.json"
if (-not (Test-Path -LiteralPath $loomExe -PathType Leaf)) { throw "Loom.exe is missing: $loomExe" }
if (-not (Test-Path -LiteralPath $trustSource -PathType Leaf)) { throw "OCR catalog trust store is missing: $trustSource" }
Assert-LoomPathHasNoReparsePoints -RootPath $packageRoot -Path $loomExe
Assert-LoomPathHasNoReparsePoints -RootPath $catalogPath -Path $catalogPath

$allowed = @{
    "catalog.json" = @{ Path = (Join-Path $catalogPath "catalog.json"); ContentType = "application/json" }
    "ocr.zip" = @{ Path = (Join-Path $catalogPath "ocr.zip"); ContentType = "application/zip" }
    "ocr.cdx.json" = @{ Path = (Join-Path $catalogPath "ocr.cdx.json"); ContentType = "application/json" }
    "ocr.provenance.json" = @{ Path = (Join-Path $catalogPath "ocr.provenance.json"); ContentType = "application/json" }
}
foreach ($record in $allowed.Values) {
    if (-not (Test-Path -LiteralPath $record.Path -PathType Leaf)) { throw "OCR catalog artifact is missing: $($record.Path)" }
    Assert-LoomPathHasNoReparsePoints -RootPath $catalogPath -Path $record.Path
}
$catalog = Get-Content -Raw -Encoding UTF8 -LiteralPath $catalogFile | ConvertFrom-Json
$entries = @($catalog.signed.packages)
if ([string]$catalog.signed.publisher.id -cne "neuro.official" -or
    $entries.Count -ne 1 -or
    [string]$entries[0].qualifiedId -cne "neuro.official/ocr") {
    throw "Local catalog must contain exactly the official OCR capability."
}
$artifactUris = @(
    [Uri][string]$entries[0].package.url,
    [Uri][string]$entries[0].sbom.url,
    [Uri][string]$entries[0].provenance.url
)
$expectedNames = @("ocr.zip", "ocr.cdx.json", "ocr.provenance.json")
for ($index = 0; $index -lt $artifactUris.Count; $index++) {
    $uri = $artifactUris[$index]
    if ($uri.Scheme -ne "http" -or $uri.Host -ne "127.0.0.1" -or
        [System.IO.Path]::GetFileName($uri.AbsolutePath) -cne $expectedNames[$index] -or
        $uri.Query -or $uri.Fragment) {
        throw "Local OCR artifact URLs must use the same explicit 127.0.0.1 HTTP catalog origin."
    }
    if ($index -gt 0 -and $uri.GetLeftPart([UriPartial]::Authority) -cne $artifactUris[0].GetLeftPart([UriPartial]::Authority)) {
        throw "Local OCR artifact URLs must share one loopback origin."
    }
}
$catalogUrl = "$($artifactUris[0].GetLeftPart([UriPartial]::Authority))/catalog.json"

$controlRoot = if ([string]::IsNullOrWhiteSpace($ControlPlaneRoot)) {
    Join-Path $catalogPath "manual-control-plane"
} else {
    [System.IO.Path]::GetFullPath($ControlPlaneRoot)
}
New-Item -ItemType Directory -Force -Path $controlRoot | Out-Null
Copy-Item -LiteralPath $trustSource -Destination (Join-Path $controlRoot "plugin-trust.json") -Force

$listener = [System.Net.Sockets.TcpListener]::new(
    [System.Net.IPAddress]::Loopback,
    $artifactUris[0].Port
)
$savedEnvironment = @{
    LOOM_CAPABILITY_CATALOG_URL = $env:LOOM_CAPABILITY_CATALOG_URL
    LOOM_CAPABILITY_CATALOG_ALLOW_LOOPBACK = $env:LOOM_CAPABILITY_CATALOG_ALLOW_LOOPBACK
    LOOM_CONTROL_PLANE_ROOT = $env:LOOM_CONTROL_PLANE_ROOT
}
$loom = $null
try {
    $listener.Start()
    $env:LOOM_CAPABILITY_CATALOG_URL = $catalogUrl
    $env:LOOM_CAPABILITY_CATALOG_ALLOW_LOOPBACK = "1"
    $env:LOOM_CONTROL_PLANE_ROOT = $controlRoot
    $loom = Start-Process -FilePath $loomExe -WorkingDirectory $packageRoot -PassThru
    Write-Host "Loom OCR test catalog: $env:LOOM_CAPABILITY_CATALOG_URL"
    Write-Host "Keep this window open while testing. Start Hook, then open Loom Settings > Capability Extensions."

    $accept = $listener.AcceptTcpClientAsync()
    while (-not $loom.HasExited) {
        if ($accept.Wait(250)) {
            $client = $accept.GetAwaiter().GetResult()
            $accept = $listener.AcceptTcpClientAsync()
            Serve-Client -Client $client -AllowedFiles $allowed -TrustedRoot $catalogPath
        }
        $loom.Refresh()
    }
}
finally {
    $listener.Stop()
    foreach ($name in $savedEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], "Process")
    }
}
