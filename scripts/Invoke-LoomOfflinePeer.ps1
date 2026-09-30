[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Status','Trust','Probe','RasterCheck','Remove')][string]$Action,
    [Parameter(Mandatory)][string]$LoomOrigin,
    [Parameter(Mandatory)][Security.SecureString]$AdminToken,
    [string]$PeerOrigin,
    [string]$PeerName = 'Offline Loom',
    [string]$PeerId,
    [string]$IdentityPath,
    [string]$IdentityOutputPath,
    [string]$ImagePath
)
$ErrorActionPreference = 'Stop'
$utf8 = [Text.UTF8Encoding]::new($false)

function Assert-Origin([string]$Value) {
    $uri = $null
    if (-not [Uri]::TryCreate($Value, [UriKind]::Absolute, [ref]$uri) -or
        $Value.Length -gt 256 -or $uri.UserInfo -or $uri.Query -or $uri.Fragment -or
        $uri.AbsolutePath -ne '/' -or $Value.EndsWith('/') -or
        ($uri.Scheme -ne 'https' -and -not ($uri.Scheme -eq 'http' -and $uri.IsLoopback))) {
        throw 'Use an HTTPS origin without credentials, path, query or fragment; HTTP is loopback-only.'
    }
}

Assert-Origin $LoomOrigin
$plainToken = [Net.NetworkCredential]::new('', $AdminToken).Password
if ([string]::IsNullOrWhiteSpace($plainToken)) { throw 'An administrator credential is required.' }
$headers = @{ Authorization = 'Bearer ' + $plainToken }

function Invoke-PeerRequest([string]$Method, [string]$Suffix = '', $Body = $null) {
    $parameters = @{
        Uri = $LoomOrigin + '/v1/projection-peers' + $Suffix
        Method = $Method; Headers = $headers; TimeoutSec = 15; MaximumRedirection = 0
    }
    if ($null -ne $Body) {
        $parameters.ContentType = 'application/json'
        $parameters.Body = $utf8.GetBytes(($Body | ConvertTo-Json -Depth 8 -Compress))
    }
    try { Invoke-RestMethod @parameters }
    catch {
        $status = 'unavailable'
        if ($_.Exception.Response) { $status = [int]$_.Exception.Response.StatusCode }
        throw "Offline peer request failed (HTTP $status). Check the origin, credential, pinned identity and peer trust."
    }
}

try {
    $view = Invoke-PeerRequest 'GET'
    switch ($Action) {
        'Status' {
            if ($IdentityOutputPath) {
                # Export only the public identity. Never serialize the request headers or private state.
                $public = @{ peerId = $view.identity.peerId; publicKey = $view.identity.publicKey }
                $path = [IO.Path]::GetFullPath($IdentityOutputPath)
                if (Test-Path -LiteralPath $path) { throw 'Identity output already exists; choose a new file.' }
                $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
                try {
                    $bytes = $utf8.GetBytes(($public | ConvertTo-Json))
                    $stream.Write($bytes, 0, $bytes.Length)
                } finally { $stream.Dispose() }
            }
            $view
        }
        'Trust' {
            Assert-Origin $PeerOrigin
            if (-not $IdentityPath -or (Get-Item -LiteralPath $IdentityPath).Length -gt 4096) {
                throw 'Provide a public identity JSON file no larger than 4096 bytes.'
            }
            $identity = [IO.File]::ReadAllText([IO.Path]::GetFullPath($IdentityPath), $utf8) | ConvertFrom-Json
            $peer = @{ peerId = $identity.peerId; publicKey = $identity.publicKey
                name = $PeerName; origin = $PeerOrigin; enabled = $true }
            Invoke-PeerRequest 'PUT' '' @{ expectedRevision = $view.revision; peer = $peer }
        }
        'Probe' {
            if (-not $PeerId) { throw 'PeerId is required.' }
            Invoke-PeerRequest 'POST' '/probe' @{ peerId = $PeerId }
        }
        'RasterCheck' {
            if (-not $PeerId -or -not $ImagePath) { throw 'PeerId and a PNG ImagePath are required.' }
            $stream = [IO.File]::Open([IO.Path]::GetFullPath($ImagePath), 'Open', 'Read', 'Read')
            try {
                if ($stream.Length -lt 24 -or $stream.Length -gt 4MB) { throw 'PNG must be 24 bytes to 4 MiB.' }
                $reader = [IO.BinaryReader]::new($stream)
                $bytes = $reader.ReadBytes([int]$stream.Length)
            } finally { $stream.Dispose() }
            if ([BitConverter]::ToString($bytes, 0, 8) -ne '89-50-4E-47-0D-0A-1A-0A' -or
                [BitConverter]::ToString($bytes, 12, 4) -ne '49-48-44-52') { throw 'A PNG with an IHDR header is required.' }
            $width = [double]$bytes[16]*16777216 + [double]$bytes[17]*65536 + [double]$bytes[18]*256 + $bytes[19]
            $height = [double]$bytes[20]*16777216 + [double]$bytes[21]*65536 + [double]$bytes[22]*256 + $bytes[23]
            if ($width -lt 1 -or $height -lt 1 -or $width -gt 8192 -or $height -gt 8192 -or $width*$height -gt 16777216) {
                throw 'PNG dimensions exceed the projection budget.'
            }
            Invoke-PeerRequest 'POST' '/raster-check' @{ peerId = $PeerId; snapshot = @{
                imageBase64 = [Convert]::ToBase64String($bytes); width = [uint32]$width; height = [uint32]$height } }
        }
        'Remove' {
            if (-not $PeerId) { throw 'PeerId is required.' }
            Invoke-PeerRequest 'DELETE' '' @{ expectedRevision = $view.revision; peerId = $PeerId }
        }
    }
} finally {
    $headers.Clear()
    $plainToken = $null
}
