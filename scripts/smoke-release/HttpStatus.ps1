<# Owns explicit HTTP status assertions for authenticated and unauthenticated smoke requests. #>

function Assert-HttpStatus {
    param(
        [string]$Uri,
        [string]$Method = "Get",
        [int]$ExpectedStatus,
        [object]$Body = $null,
        [hashtable]$Headers = @{}
    )

    Assert-SmokeLoopbackUri -Uri $Uri
    $statusCode = $null
    try {
        if ($null -eq $Body) {
            Invoke-WebRequest -Uri $Uri -Method $Method -Headers $Headers -TimeoutSec 10 -UseBasicParsing -MaximumRedirection 0 | Out-Null
        } else {
            $json = $Body | ConvertTo-Json -Depth 20
            Invoke-WebRequest -Uri $Uri -Method $Method -Headers $Headers -ContentType "application/json" -Body $json -TimeoutSec 10 -UseBasicParsing -MaximumRedirection 0 | Out-Null
        }
        $statusCode = 200
    } catch {
        if ($null -eq $_.Exception.Response) {
            throw
        }
        $statusCode = [int]$_.Exception.Response.StatusCode
    }

    Assert-Equal $ExpectedStatus $statusCode "HTTP status mismatch for $Method $Uri."
}
