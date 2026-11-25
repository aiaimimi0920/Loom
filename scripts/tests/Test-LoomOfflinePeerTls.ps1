[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputRoot)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($OutputRoot)
if (Test-Path -LiteralPath $root) { throw 'Choose an unused evidence directory.' }
[void][IO.Directory]::CreateDirectory($root)
$scriptUnderTest = Join-Path $PSScriptRoot '../Invoke-LoomOfflinePeer.ps1'
$global:peerTlsRequests = [Collections.Generic.List[object]]::new()
$global:peerTlsFixture = [pscustomobject]@{
    revision = 3
    identity = [pscustomobject]@{ peerId = 'loom-test'; publicKey = 'public-test'; privateKey = 'never-export' }
    localConnection = [pscustomobject]@{
        origin = 'https://192.168.1.2:8766'; certificate = 'public-certificate'; privateKey = 'never-export'
    }
}
function Invoke-RestMethod {
    param($Uri, $Method, $Headers, $TimeoutSec, $MaximumRedirection, $ContentType, $Body)
    $global:peerTlsRequests.Add([pscustomobject]@{
        Method = $Method; Body = if ($Body) { [Text.Encoding]::UTF8.GetString($Body) | ConvertFrom-Json } else { $null }
    })
    return $global:peerTlsFixture
}
function Assert-Tls([bool]$Value, [string]$Message) {
    if (-not $Value) { throw $Message }
}
$token = ConvertTo-SecureString 'synthetic-admin-not-a-real-credential' -AsPlainText -Force
$export = Join-Path $root 'public.json'
$arguments = @{ LoomOrigin = 'http://127.0.0.1:8765'; AdminToken = $token }
[void](& $scriptUnderTest @arguments -Action Status -IdentityOutputPath $export)
$text = [IO.File]::ReadAllText($export)
$public = $text | ConvertFrom-Json
Assert-Tls ($public.connection.certificate -ceq 'public-certificate') 'Public certificate missing from export.'
Assert-Tls (-not $text.Contains('never-export') -and -not $text.Contains('synthetic-admin')) 'Private data exported.'
Assert-Tls (-not ([IO.File]::ReadAllBytes($export)[0] -eq 0xEF)) 'Unexpected UTF-8 BOM.'
[void](& $scriptUnderTest @arguments -Action Trust -IdentityPath $export)
$put = $global:peerTlsRequests[$global:peerTlsRequests.Count - 1]
Assert-Tls ($put.Method -eq 'PUT' -and $put.Body.expectedRevision -eq 3) 'Trust CAS request missing.'
Assert-Tls ($put.Body.peer.origin -ceq $public.connection.origin) 'Exported origin was not used.'
Assert-Tls ($put.Body.peer.certificate -ceq 'public-certificate') 'Approved certificate was not transmitted.'
$before = @($global:peerTlsRequests | Where-Object Method -eq 'PUT').Count
$rejected = $false
try {
    [void](& $scriptUnderTest @arguments -Action Trust -IdentityPath $export -PeerOrigin 'https://other.example.test')
} catch { $rejected = $_.Exception.Message -like '*must match*' }
Assert-Tls $rejected 'Certificate must not be re-bound to a different origin.'
Assert-Tls (@($global:peerTlsRequests | Where-Object Method -eq 'PUT').Count -eq $before) 'Rejected override still wrote trust.'
$global:peerTlsFixture.localConnection = $null
$without = Join-Path $root 'public-without-listener.json'
[void](& $scriptUnderTest @arguments -Action Status -IdentityOutputPath $without)
Assert-Tls (-not ([IO.File]::ReadAllText($without) | ConvertFrom-Json).PSObject.Properties['connection']) 'Inactive listener exported trust.'
Write-Output 'Passed public-only TLS export, CAS trust import, origin binding and inactive-listener checks.'
