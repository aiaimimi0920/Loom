param(
    [Parameter(Mandatory = $true)][int]$Port,
    [Parameter(Mandatory = $true)][string]$ReadyPath,
    [Parameter(Mandatory = $true)][string]$RequestPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Write-Utf8NoBom {
    param([string]$Path, [string]$Value)
    $parent = Split-Path -Parent $Path
    if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    [IO.File]::WriteAllText($Path, $Value, [Text.UTF8Encoding]::new($false))
}

function Write-JsonResponse {
    param(
        [System.Net.HttpListenerContext]$Context,
        [object]$Value,
        [int]$StatusCode = 200
    )
    $json = $Value | ConvertTo-Json -Depth 30 -Compress
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
    $Context.Response.StatusCode = $StatusCode
    $Context.Response.ContentType = "application/json; charset=utf-8"
    $Context.Response.ContentLength64 = $bytes.Length
    $Context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
    $Context.Response.OutputStream.Close()
}

$listener = [System.Net.HttpListener]::new()
$listener.Prefixes.Add("http://127.0.0.1:$Port/")
$listener.IgnoreWriteExceptions = $true
$listener.Start()
Write-Utf8NoBom -Path $RequestPath -Value ""
Write-Utf8NoBom -Path $ReadyPath -Value "ready`n"

$servedApiRequests = 0
try {
    while ($servedApiRequests -lt 2) {
        $context = $listener.GetContext()
        [IO.File]::AppendAllText(
            $RequestPath,
            "$($context.Request.HttpMethod) $($context.Request.RawUrl)`n",
            [Text.UTF8Encoding]::new($false)
        )
        switch ($context.Request.Url.AbsolutePath) {
            "/api/qt/stock/get" {
                $servedApiRequests++
                Write-JsonResponse -Context $context -Value ([ordered]@{
                    rc = 0
                    data = [ordered]@{
                        f43 = 2499
                        f44 = 2545
                        f45 = 2459
                        f46 = 2489
                        f47 = 459631
                        f48 = 1147741811.53
                        f49 = 208267
                        f50 = 105
                        f51 = 2738
                        f52 = 2240
                        f57 = "000034"
                        f58 = "神州数码"
                        f60 = 2489
                        f116 = 25408324178.37
                        f117 = 21220204571.58
                        f162 = 2692
                        f167 = 227
                        f168 = 541
                        f169 = 10
                        f170 = 40
                        f171 = 346
                    }
                })
            }
            "/api/qt/stock/trends2/get" {
                $servedApiRequests++
                Write-JsonResponse -Context $context -Value ([ordered]@{
                    rc = 0
                    data = [ordered]@{
                        code = "000034"
                        market = 0
                        name = "神州数码"
                        trends = @(
                            "2026-08-14 09:30,24.89,24.89,24.89,24.89,2430,6048270.00,24.890",
                            "2026-08-14 10:30,25.10,25.10,25.10,25.10,3120,7823400.00,25.010",
                            "2026-08-14 15:00,24.99,24.99,24.99,24.99,6916,17283084.00,24.971"
                        )
                    }
                })
            }
            default {
                Write-JsonResponse -Context $context -Value @{ error = "not found" } -StatusCode 404
            }
        }
    }
}
finally {
    $listener.Stop()
    $listener.Close()
}
