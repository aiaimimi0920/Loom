param(
    [Parameter(Mandatory = $true)][int]$Port,
    [Parameter(Mandatory = $true)][string]$ReadyPath,
    [Parameter(Mandatory = $true)][string]$ImageBase64,
    [string]$RequestPath = "",
    [int]$MaxRequests = 8
)

# A loopback HTTP server that serves one image, for tests that need an image URL a guarded Art is
# allowed to fetch. It runs as its own process because the Art runtime blocks the test while it
# downloads, so the server cannot share the test's thread.

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Write-Utf8NoBom {
    param([string]$Path, [string]$Value)
    $parent = Split-Path -Parent $Path
    if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    [IO.File]::WriteAllText($Path, $Value, [Text.UTF8Encoding]::new($false))
}

$imageBytes = [Convert]::FromBase64String($ImageBase64)

$listener = [System.Net.HttpListener]::new()
$listener.Prefixes.Add("http://127.0.0.1:$Port/")
$listener.IgnoreWriteExceptions = $true
$listener.Start()
if ($RequestPath) { Write-Utf8NoBom -Path $RequestPath -Value "" }
Write-Utf8NoBom -Path $ReadyPath -Value "ready`n"

$served = 0
try {
    while ($served -lt $MaxRequests) {
        $context = $listener.GetContext()
        $served++
        if ($RequestPath) {
            [IO.File]::AppendAllText(
                $RequestPath,
                "$($context.Request.HttpMethod) $($context.Request.Url.AbsolutePath)`n",
                [Text.UTF8Encoding]::new($false)
            )
        }
        if ($context.Request.Url.AbsolutePath -ne "/fixture.png") {
            $context.Response.StatusCode = 404
            $context.Response.ContentLength64 = 0
            $context.Response.OutputStream.Close()
            continue
        }
        $context.Response.StatusCode = 200
        $context.Response.ContentType = "image/png"
        $context.Response.ContentLength64 = $imageBytes.Length
        $context.Response.OutputStream.Write($imageBytes, 0, $imageBytes.Length)
        $context.Response.OutputStream.Close()
    }
}
finally {
    $listener.Stop()
    $listener.Close()
}
