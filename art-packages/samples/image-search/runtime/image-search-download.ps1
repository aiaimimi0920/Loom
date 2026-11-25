# Download validated raster candidates; each redirect re-enters the address policy.

# How many redirects a single image download may follow. Each hop is revalidated, so this is a
# bound on work rather than a safety control, but it also stops a redirect loop from holding the
# request open until the client timeout expires.
$script:ImageDownloadRedirectLimit = 5

# Cache for the loopback test seam below, so the environment is read once per process.
$script:LoopbackImageDownloadAllowed = $null

function Test-LoopbackImageDownloadAllowed {
    # A test seam, not a product feature. `scripts/tests/Test-LoomSampleArtInstallExecution.ps1`
    # installs this package for real and serves its fixture image from
    # `http://127.0.0.1:<port>/fixture.png`; the loopback rule in `Test-BlockedImageAddress` refuses
    # that correctly, and the fixture has nowhere else to serve from.
    #
    # The switch is an environment variable rather than a manifest field on purpose. Neither
    # `manifest.json` nor `art.runtime.json` can set process environment — `ArtRuntimeManifest.entry`
    # is a command and its arguments and nothing else — so a published package cannot turn its own
    # guard off; only whoever launches the host can. `LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES` is in
    # the inherited-environment allowlist in `crates/loom_process/src/lib.rs` so that it survives both
    # spawns between the host and this script.
    #
    # It relaxes exactly one thing: a literal loopback address written in the URL. A hostname that
    # resolves to 127.0.0.1 — the DNS-rebinding shape — stays blocked with the switch on, and every
    # other blocked range stays blocked.
    if ($null -eq $script:LoopbackImageDownloadAllowed) {
        $raw = $env:LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES
        $script:LoopbackImageDownloadAllowed = (-not [string]::IsNullOrWhiteSpace($raw)) -and
            (@("1", "true", "yes", "on") -contains $raw.Trim().ToLowerInvariant())
    }
    return $script:LoopbackImageDownloadAllowed
}

function Get-ImageMimeType {
    param(
        [string]$Location,
        [AllowNull()][string]$ContentType
    )

    $declared = Get-DataUrlMediaType -Location $Location
    if (Test-SupportedImageMimeType -MimeType $declared) {
        return $declared.ToLowerInvariant()
    }
    if (-not [string]::IsNullOrWhiteSpace($ContentType)) {
        $mimeType = ($ContentType -split ';', 2)[0].Trim()
        if (Test-SupportedImageMimeType -MimeType $mimeType) {
            return $mimeType.ToLowerInvariant()
        }
    }
    $path = ($Location -split '[?#]', 2)[0].ToLowerInvariant()
    if ($path.EndsWith(".jpg") -or $path.EndsWith(".jpeg")) { return "image/jpeg" }
    if ($path.EndsWith(".webp")) { return "image/webp" }
    if ($path.EndsWith(".gif")) { return "image/gif" }
    if ($path.EndsWith(".bmp")) { return "image/bmp" }
    if ($path.EndsWith(".avif")) { return "image/avif" }
    return "image/png"
}

function Test-BlockedImageAddress {
    param([Parameter(Mandatory = $true)][System.Net.IPAddress]$Address)

    # Mirrors `validate_ip` in `crates/loom_security/src/network.rs`, range for range. Every URL this
    # Art downloads is named by an MCP server, so the Art is a deputy acting on a third party's
    # instructions: the only reason such a URL would point at a loopback, private, or link-local
    # address is to make this process read a service reachable from the user's machine and nowhere
    # else — a local admin port, a LAN device, or a cloud instance-metadata endpoint — and hand the
    # bytes back as an "image". The set is deliberately not widened past the host's: a fake-IP DNS
    # resolver, which is how many machines reach the internet through a local proxy, hands out
    # addresses from otherwise-unroutable ranges such as 198.18/15, so blocking those would refuse
    # ordinary public image hosts.
    if ([System.Net.IPAddress]::IsLoopback($Address)) {
        return $true
    }
    if ($Address.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetworkV6) {
        if ($Address.IsIPv6LinkLocal -or $Address.IsIPv6SiteLocal -or $Address.IsIPv6Multicast) {
            return $true
        }
        $bytes = $Address.GetAddressBytes()
        if (($bytes[0] -band 0xfe) -eq 0xfc) {
            return $true
        }
        # `::ffff:10.0.0.1` and its siblings reach the IPv4 address they embed, so they are judged
        # as that address rather than as an opaque IPv6 one.
        $isMapped = $true
        foreach ($index in 0..9) {
            if ($bytes[$index] -ne 0) {
                $isMapped = $false
                break
            }
        }
        if ($isMapped -and ($bytes[10] -ne 0xff -or $bytes[11] -ne 0xff)) {
            $isMapped = $false
        }
        if ($isMapped) {
            return Test-BlockedImageAddress -Address ([System.Net.IPAddress]::new([byte[]]$bytes[12..15]))
        }
        return $Address.Equals([System.Net.IPAddress]::IPv6Any)
    }
    if ($Address.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetwork) {
        return $true
    }
    $octets = $Address.GetAddressBytes()
    # Unspecified, multicast (224/4), and everything reserved above it, including the broadcast
    # address.
    if ($octets[0] -eq 0 -or $octets[0] -ge 224) { return $true }
    if ($octets[0] -eq 10) { return $true }
    if ($octets[0] -eq 172 -and $octets[1] -ge 16 -and $octets[1] -le 31) { return $true }
    if ($octets[0] -eq 192 -and $octets[1] -eq 168) { return $true }
    # Link-local, which is also where the cloud instance-metadata address 169.254.169.254 lives.
    if ($octets[0] -eq 169 -and $octets[1] -eq 254) { return $true }
    if ($octets[0] -eq 192 -and $octets[1] -eq 0 -and $octets[2] -eq 2) { return $true }
    if ($octets[0] -eq 198 -and $octets[1] -eq 51 -and $octets[2] -eq 100) { return $true }
    if ($octets[0] -eq 203 -and $octets[1] -eq 0 -and $octets[2] -eq 113) { return $true }
    return $false
}

function Resolve-ImageDownloadTarget {
    param([Parameter(Mandatory = $true)][string]$Location)

    # Called for the requested URL and again for every redirect hop, so the address that is finally
    # read is one that passed the policy. An extension check is not a control: `Test-ImageLocation`
    # is satisfied by `http://127.0.0.1:8787/anything.png`, which is exactly the shape this refuses.
    $uri = $null
    if (-not [System.Uri]::TryCreate($Location, [System.UriKind]::Absolute, [ref]$uri)) {
        throw "image location is not an absolute URL"
    }
    if ($uri.Scheme -ne [System.Uri]::UriSchemeHttp -and $uri.Scheme -ne [System.Uri]::UriSchemeHttps) {
        throw "unsupported image URL scheme '$($uri.Scheme)'"
    }
    $literal = $null
    $isLiteralAddress = [System.Net.IPAddress]::TryParse($uri.DnsSafeHost, [ref]$literal)
    # Wrapped in `@(...)` as a whole, because assigning the result of an `if` unrolls a single-element
    # array back to the element.
    $addresses = @(if ($isLiteralAddress) {
        $literal
    }
    else {
        [System.Net.Dns]::GetHostAddresses($uri.DnsSafeHost)
    })
    if ($addresses.Count -eq 0) {
        throw "image host '$($uri.DnsSafeHost)' resolved to no addresses"
    }
    # The loopback exemption applies only to an address written literally in the URL, so a hostname
    # that resolves to a loopback address is still refused even with the seam enabled.
    $loopbackAllowed = $isLiteralAddress -and (Test-LoopbackImageDownloadAllowed)
    foreach ($address in $addresses) {
        if ($loopbackAllowed -and [System.Net.IPAddress]::IsLoopback($address)) {
            continue
        }
        if (Test-BlockedImageAddress -Address $address) {
            throw "image host '$($uri.DnsSafeHost)' resolves to blocked address $address"
        }
    }
    return $uri
}

function Convert-ImageLocationToDataUrl {
    param(
        [Parameter(Mandatory = $true)][string]$Location,
        [AllowNull()][string]$Referer
    )

    if ($Location.StartsWith("data:image/", [System.StringComparison]::OrdinalIgnoreCase)) {
        return $Location
    }
    Add-Type -AssemblyName System.Net.Http
    $handler = [System.Net.Http.HttpClientHandler]::new()
    # Redirects are followed by hand below so that each hop is revalidated; with automatic redirects
    # the URL that passed the policy and the URL that answered can differ. The proxy is bypassed for
    # the same reason: a proxy would connect on this process's behalf to a host it never checked.
    $handler.AllowAutoRedirect = $false
    $handler.UseProxy = $false
    $client = [System.Net.Http.HttpClient]::new($handler)
    $client.Timeout = [TimeSpan]::FromSeconds(30)
    $client.DefaultRequestHeaders.UserAgent.ParseAdd("Loom-MCP-Art/0.2")
    if (-not [string]::IsNullOrWhiteSpace($Referer)) {
        try {
            $client.DefaultRequestHeaders.Referrer = [Uri]$Referer
        }
        catch {
        }
    }
    try {
        $target = Resolve-ImageDownloadTarget -Location $Location
        $response = $null
        foreach ($hop in 0..$script:ImageDownloadRedirectLimit) {
            $response = $client.GetAsync(
                $target,
                [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead
            ).GetAwaiter().GetResult()
            $status = [int]$response.StatusCode
            if ($status -ne 301 -and $status -ne 302 -and $status -ne 303 -and
                $status -ne 307 -and $status -ne 308) {
                break
            }
            $redirect = $response.Headers.Location
            $response.Dispose()
            $response = $null
            if ($null -eq $redirect) {
                throw "image download returned HTTP $status without a location header"
            }
            if (-not $redirect.IsAbsoluteUri) {
                $redirect = [System.Uri]::new($target, $redirect)
            }
            $target = Resolve-ImageDownloadTarget -Location $redirect.AbsoluteUri
        }
        if ($null -eq $response) {
            throw "image download exceeded $($script:ImageDownloadRedirectLimit) redirects"
        }
        try {
            if (-not $response.IsSuccessStatusCode) {
                throw "image download returned HTTP $([int]$response.StatusCode)"
            }
            $maximumBytes = 32MB
            if ($response.Content.Headers.ContentLength -and
                [long]$response.Content.Headers.ContentLength -gt $maximumBytes) {
                throw "image exceeds 32 MiB"
            }
            $stream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $memory = [System.IO.MemoryStream]::new()
            $buffer = New-Object byte[] 81920
            try {
                while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    if ($memory.Length + $read -gt $maximumBytes) {
                        throw "image exceeds 32 MiB"
                    }
                    $memory.Write($buffer, 0, $read)
                }
                $bytes = $memory.ToArray()
            }
            finally {
                $stream.Dispose()
                $memory.Dispose()
            }
            $contentType = if ($response.Content.Headers.ContentType) {
                [string]$response.Content.Headers.ContentType.MediaType
            }
            else {
                ""
            }
            # A response that names its own kind is believed when it refuses: a server answering with
            # `image/svg+xml`, or with something that is not an image at all, is not made into a
            # candidate. A response that names nothing still falls through to the URL below, because
            # plenty of image hosts send no usable content type.
            if (-not [string]::IsNullOrWhiteSpace($contentType) -and
                -not (Test-SupportedImageMimeType -MimeType $contentType)) {
                throw "image download returned unsupported content type '$contentType'"
            }
            $mimeType = Get-ImageMimeType -Location $target.AbsoluteUri -ContentType $contentType
            return "data:$mimeType;base64,$([Convert]::ToBase64String($bytes))"
        }
        finally {
            $response.Dispose()
        }
    }
    finally {
        $client.Dispose()
        $handler.Dispose()
    }
}
