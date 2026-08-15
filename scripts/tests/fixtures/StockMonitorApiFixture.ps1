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

function Write-Response {
    param(
        [System.Net.HttpListenerContext]$Context,
        [string]$Body,
        [string]$ContentType,
        [int]$StatusCode = 200
    )
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Body)
    $Context.Response.StatusCode = $StatusCode
    $Context.Response.ContentType = $ContentType
    $Context.Response.ContentLength64 = $bytes.Length
    $Context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
    $Context.Response.OutputStream.Close()
}

function Write-JsonResponse {
    param(
        [System.Net.HttpListenerContext]$Context,
        [object]$Value,
        [int]$StatusCode = 200
    )
    Write-Response `
        -Context $Context `
        -Body ($Value | ConvertTo-Json -Depth 30 -Compress) `
        -ContentType "application/json; charset=utf-8" `
        -StatusCode $StatusCode
}

$listener = [System.Net.HttpListener]::new()
$listener.Prefixes.Add("http://127.0.0.1:$Port/")
$listener.IgnoreWriteExceptions = $true
$listener.Start()
Write-Utf8NoBom -Path $RequestPath -Value ""
Write-Utf8NoBom -Path $ReadyPath -Value "ready`n"

$servedApiRequests = 0
try {
    while ($servedApiRequests -lt 4) {
        $context = $listener.GetContext()
        if ($context.Request.Url.AbsolutePath -ne "/proxy") {
            Write-JsonResponse -Context $context -Value @{ error = "not found" } -StatusCode 404
            continue
        }
        $target = [Uri]::UnescapeDataString([string]$context.Request.QueryString["url"])
        [IO.File]::AppendAllText(
            $RequestPath,
            "$($context.Request.HttpMethod) $target`n",
            [Text.UTF8Encoding]::new($false)
        )
        $servedApiRequests++
        if ($target -match '^https://qt\.gtimg\.cn/q=sz000034$') {
            $fields = @("51", "Digital China", "000034", "24.99", "24.89")
            while ($fields.Count -lt 33) { $fields += "0" }
            $fields += @("25.20", "24.60")
            Write-Response -Context $context -Body ('v_sz000034="' + ($fields -join '~') + '";') -ContentType "text/plain; charset=utf-8"
        }
        elseif ($target -match '^https://hq\.sinajs\.cn/list=sz000034$') {
            Write-Response -Context $context -Body "var hq_str_sz000034=`"`";`n" -ContentType "text/plain; charset=utf-8"
        }
        elseif ($target -match 'push2delay\.eastmoney\.com/api/qt/stock/get\?' -and $target -match 'secid=0(?:%2E|\.)000034') {
            Write-JsonResponse -Context $context -Value ([ordered]@{
                rc = 0
                data = [ordered]@{
                    f43 = 24.99
                    f44 = 25.20
                    f45 = 24.60
                    f57 = "000034"
                    f58 = "Digital China"
                    f60 = 24.89
                    f170 = 0.4
                }
            })
        }
        elseif ($target -match 'web\.ifzq\.gtimg\.cn/appstock/app/kline/kline\?' -and $target -match 'param=sz000034') {
            Write-JsonResponse -Context $context -Value ([ordered]@{
                data = [ordered]@{
                    sz000034 = [ordered]@{
                        day = @(
                            @("2026-08-12", "24.50", "24.60", "24.80", "24.30", "100000"),
                            @("2026-08-13", "24.62", "24.75", "24.90", "24.55", "120000"),
                            @("2026-08-14", "24.80", "24.99", "25.20", "24.60", "150000")
                        )
                    }
                }
            })
        }
        else {
            throw "Unexpected stock-api target: $target"
        }
    }
}
finally {
    $listener.Stop()
    $listener.Close()
}
