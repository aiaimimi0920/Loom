$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "common.ps1")

# Bounds for the walk over an MCP tool result.
#
# A string inside the result that begins with `{` or `[` is parsed again as JSON, and each parse
# restarts PowerShell's own nesting limit while the walk is already that many frames into the
# stack. A result that is individually shallow at every hop can therefore drive the walk
# arbitrarily deep, and a PowerShell stack overflow cannot be caught: the process dies and the
# Art request dies with it. Tool results come from servers Loom does not control, so the walk
# keeps a counter of its own.
#
# The candidate ceiling bounds the cheaper attack in the other direction: a flat array of a
# million URLs would otherwise be collected and returned in full.
$script:McpImageCandidateDepthLimit = 24
$script:McpImageCandidateLimit = 64

# How many redirects a single image download may follow. Each hop is revalidated, so this is a
# bound on work rather than a safety control, but it also stops a redirect loop from holding the
# request open until the client timeout expires.
$script:ImageDownloadRedirectLimit = 5

# The raster image kinds this Art will collect and deliver.
#
# SVG is deliberately absent from both lists. An SVG is a document that can carry script and
# reference remote content, and nothing downstream of here sandboxes it: the bytes travel to the
# canvas as-is. The host applies the same rule — `IMAGE_URL_EXTENSIONS` and
# `SUPPORTED_IMAGE_MIME_TYPES` in `crates/loom_tool_registry/src/lib.rs` — so a candidate this Art
# refused would have been refused there as well.
$script:ImageUrlExtensions = @(".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".avif")
$script:SupportedImageMimeTypes = @(
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/bmp",
    "image/avif"
)

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

function Get-McpPropertyValue {
    param(
        [AllowNull()][object]$Value,
        [Parameter(Mandatory = $true)][string[]]$Names
    )

    if ($null -eq $Value) {
        return $null
    }
    foreach ($name in $Names) {
        $property = $Value.PSObject.Properties[$name]
        if ($null -ne $property) {
            return $property.Value
        }
    }
    return $null
}

function Find-McpStringProperty {
    param(
        [AllowNull()][object]$Value,
        [Parameter(Mandatory = $true)][string[]]$Names,
        [Parameter(Mandatory = $true)][string]$PathPrefix
    )

    if ($null -eq $Value) {
        return $null
    }
    foreach ($name in $Names) {
        $property = $Value.PSObject.Properties[$name]
        if ($null -ne $property -and $property.Value -is [string] -and
            -not [string]::IsNullOrWhiteSpace([string]$property.Value)) {
            return [pscustomobject]@{
                Value = ([string]$property.Value).Trim()
                Source = "$PathPrefix.$name"
            }
        }
    }
    return $null
}

function Find-McpThumbnailLocation {
    param(
        [AllowNull()][object]$Value,
        [Parameter(Mandatory = $true)][string]$PathPrefix
    )

    $direct = Find-McpStringProperty `
        -Value $Value `
        -Names @("thumbnail_url", "thumbnailUrl", "thumbnail", "placeholder") `
        -PathPrefix $PathPrefix
    if ($null -ne $direct) {
        return $direct
    }

    foreach ($containerName in @("thumbnail", "placeholder")) {
        $container = Get-McpPropertyValue -Value $Value -Names @($containerName)
        $nested = Find-McpStringProperty `
            -Value $container `
            -Names @("image_url", "imageUrl", "src", "url") `
            -PathPrefix "$PathPrefix.$containerName"
        if ($null -ne $nested) {
            return $nested
        }
    }
    return $null
}

function Get-DataUrlMediaType {
    param([AllowNull()][string]$Location)

    if ([string]::IsNullOrWhiteSpace($Location) -or
        -not $Location.StartsWith("data:", [System.StringComparison]::OrdinalIgnoreCase)) {
        return ""
    }
    # `data:` URLs may omit the parameter list, so the media type ends at whichever of `;` and `,`
    # comes first.
    $header = ($Location -split ',', 2)[0].Substring(5)
    return ($header -split ';', 2)[0].Trim()
}

function Test-SupportedImageMimeType {
    param([AllowNull()][string]$MimeType)

    if ([string]::IsNullOrWhiteSpace($MimeType)) {
        return $false
    }
    return $script:SupportedImageMimeTypes -contains $MimeType.Trim().ToLowerInvariant()
}

function Test-RefusedImageLocation {
    param([AllowNull()][string]$Location)

    # A structured result may name an image with an extensionless URL — search APIs routinely do — so
    # a URL is not required to look like an image to be worth trying. It is refused only when it says
    # outright that it is a kind this Art will not deliver.
    if ([string]::IsNullOrWhiteSpace($Location)) {
        return $false
    }
    $location = $Location.Trim()
    if ($location.StartsWith("data:", [System.StringComparison]::OrdinalIgnoreCase)) {
        return -not (Test-SupportedImageMimeType -MimeType (Get-DataUrlMediaType -Location $location))
    }
    $path = ($location -split '[?#]', 2)[0].ToLowerInvariant()
    return $path.EndsWith(".svg", [System.StringComparison]::Ordinal) -or
        $path.EndsWith(".svgz", [System.StringComparison]::Ordinal)
}

function Test-ImageLocation {
    param([AllowNull()][object]$Value)

    if ($Value -isnot [string]) {
        return $false
    }
    $location = $Value.Trim()
    if ($location.StartsWith("data:", [System.StringComparison]::OrdinalIgnoreCase)) {
        return Test-SupportedImageMimeType -MimeType (Get-DataUrlMediaType -Location $location)
    }
    if (-not ($location.StartsWith("http://", [System.StringComparison]::OrdinalIgnoreCase) -or
            $location.StartsWith("https://", [System.StringComparison]::OrdinalIgnoreCase))) {
        return $false
    }
    $path = ($location -split '[?#]', 2)[0].ToLowerInvariant()
    return $null -ne ($script:ImageUrlExtensions |
        Where-Object { $path.EndsWith($_, [System.StringComparison]::Ordinal) } |
        Select-Object -First 1)
}

function Convert-ToMcpImageCandidate {
    param(
        [AllowNull()][object]$Value,
        [string]$PathPrefix = "result",
        [ValidateSet("page", "image")][string]$GenericUrlRole = "page",
        [AllowNull()][string]$InheritedSourcePageUrl,
        [AllowNull()][string]$InheritedSourcePageSource
    )

    if ($null -eq $Value) {
        return $null
    }
    $type = [string](Get-McpPropertyValue -Value $Value -Names @("type"))
    $mimeType = [string](Get-McpPropertyValue -Value $Value -Names @("mimeType", "mime_type"))
    $data = Get-McpPropertyValue -Value $Value -Names @("data")
    if ($type -eq "image" -and $data -is [string] -and -not [string]::IsNullOrWhiteSpace($data)) {
        $imageData = $data.Trim()
        if (-not $imageData.StartsWith("data:image/", [System.StringComparison]::OrdinalIgnoreCase)) {
            if ([string]::IsNullOrWhiteSpace($mimeType)) {
                $mimeType = "image/png"
            }
            $imageData = "data:$mimeType;base64,$imageData"
        }
        # An inline `image` block still has to be a kind this Art delivers. A server that labels its
        # payload `image/svg+xml`, or anything else outside the supported list, is refused here rather
        # than at download time, because inline data never reaches the download path.
        if (-not (Test-ImageLocation -Value $imageData)) {
            return $null
        }
        return [ordered]@{
            imageUrl = $imageData
            thumbnailUrl = $imageData
            imageUrlSource = "$PathPrefix.data"
            thumbnailUrlSource = "$PathPrefix.data"
            sourcePageUrlSource = $null
            title = "MCP image"
            sourcePageUrl = $null
            width = $null
            height = $null
        }
    }

    # Normalization is deliberately ordered by meaning, not by whichever `url`-shaped field happens
    # to be visited first. At the top level, a generic `url` is a result/page URL. Within a search
    # result's `properties`, a generic `url` is the downloadable asset URL used by Brave-style MCP
    # servers. Thumbnail fields are a separate fallback channel and never silently become page URLs.
    $properties = Get-McpPropertyValue -Value $Value -Names @("properties")
    $imageLocation = Find-McpStringProperty `
        -Value $Value `
        -Names @("image_url", "imageUrl", "src") `
        -PathPrefix $PathPrefix
    if ($null -eq $imageLocation) {
        $imageLocation = Find-McpStringProperty `
            -Value $properties `
            -Names @("image_url", "imageUrl", "src", "url") `
            -PathPrefix "$PathPrefix.properties"
    }
    if ($null -eq $imageLocation -and ($GenericUrlRole -eq "image" -or $type -eq "image")) {
        $imageLocation = Find-McpStringProperty `
            -Value $Value `
            -Names @("url") `
            -PathPrefix $PathPrefix
    }

    $thumbnailLocation = Find-McpThumbnailLocation -Value $Value -PathPrefix $PathPrefix
    if ($null -eq $thumbnailLocation) {
        $thumbnailLocation = Find-McpThumbnailLocation -Value $properties -PathPrefix "$PathPrefix.properties"
    }
    if ($null -eq $imageLocation -and $null -ne $thumbnailLocation) {
        $imageLocation = $thumbnailLocation
    }

    $sourcePageNames = @("source_page_url", "sourcePageUrl", "source", "page_url", "pageUrl")
    if ($GenericUrlRole -ne "image" -and $type -ne "image") {
        $sourcePageNames += "url"
    }
    $sourcePageLocation = Find-McpStringProperty `
        -Value $Value `
        -Names $sourcePageNames `
        -PathPrefix $PathPrefix
    if ($null -eq $sourcePageLocation) {
        $sourcePageLocation = Find-McpStringProperty `
            -Value $properties `
            -Names @("source_page_url", "sourcePageUrl", "source", "page_url", "pageUrl") `
            -PathPrefix "$PathPrefix.properties"
    }
    if ($null -eq $sourcePageLocation -and -not [string]::IsNullOrWhiteSpace($InheritedSourcePageUrl)) {
        $sourcePageLocation = [pscustomobject]@{
            Value = $InheritedSourcePageUrl
            Source = $InheritedSourcePageSource
        }
    }

    $width = Get-McpPropertyValue -Value $Value -Names @("width")
    if ($null -eq $width) {
        $width = Get-McpPropertyValue -Value $properties -Names @("width")
    }
    $height = Get-McpPropertyValue -Value $Value -Names @("height")
    if ($null -eq $height) {
        $height = Get-McpPropertyValue -Value $properties -Names @("height")
    }
    if ($null -eq $imageLocation) {
        return $null
    }
    $imageUrl = [string]$imageLocation.Value
    if (Test-RefusedImageLocation -Location $imageUrl) {
        return $null
    }

    $thumbnailUrl = if ($null -ne $thumbnailLocation) { [string]$thumbnailLocation.Value } else { $null }
    $sourcePageUrl = if ($null -ne $sourcePageLocation) { [string]$sourcePageLocation.Value } else { $null }
    if ($sourcePageUrl -eq $imageUrl) {
        $sourcePageUrl = $null
        $sourcePageLocation = $null
    }
    # The thumbnail is the download fallback for this candidate, so a refused one is dropped rather
    # than carried: the fallback would only fail later, and a refused thumbnail is not worth showing.
    if ($thumbnailUrl -is [string] -and (Test-RefusedImageLocation -Location $thumbnailUrl)) {
        $thumbnailUrl = $null
    }
    return [ordered]@{
        imageUrl = $imageUrl
        thumbnailUrl = $thumbnailUrl
        imageUrlSource = [string]$imageLocation.Source
        thumbnailUrlSource = if ($null -ne $thumbnailLocation) { [string]$thumbnailLocation.Source } else { $null }
        sourcePageUrlSource = if ($null -ne $sourcePageLocation) { [string]$sourcePageLocation.Source } else { $null }
        title = [string](Get-McpPropertyValue -Value $Value -Names @("title", "label", "name"))
        sourcePageUrl = $sourcePageUrl
        width = $width
        height = $height
    }
}

function Add-McpImageCandidates {
    param(
        [AllowNull()][object]$Value,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][System.Collections.ArrayList]$Candidates,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][System.Collections.Generic.HashSet[string]]$Seen,
        [int]$Depth = 0,
        [string]$PathPrefix = "result",
        [ValidateSet("page", "image")][string]$GenericUrlRole = "page",
        [AllowNull()][string]$InheritedSourcePageUrl,
        [AllowNull()][string]$InheritedSourcePageSource
    )

    if ($null -eq $Value -or
        $Depth -gt $script:McpImageCandidateDepthLimit -or
        $Candidates.Count -ge $script:McpImageCandidateLimit) {
        return
    }
    if ($Value -is [string]) {
        $text = $Value.Trim()
        if (Test-ImageLocation -Value $text) {
            if ($Seen.Add($text)) {
                [void]$Candidates.Add([ordered]@{
                    imageUrl = $text
                    thumbnailUrl = $text
                    imageUrlSource = $PathPrefix
                    thumbnailUrlSource = $PathPrefix
                    sourcePageUrlSource = $null
                    title = ""
                    sourcePageUrl = $null
                    width = $null
                    height = $null
                })
            }
            return
        }
        if ($text.StartsWith("{") -or $text.StartsWith("[")) {
            try {
                Add-McpImageCandidates `
                    -Value ($text | ConvertFrom-Json) `
                    -Candidates $Candidates `
                    -Seen $Seen `
                    -Depth ($Depth + 1) `
                    -PathPrefix "$PathPrefix.decodedJson" `
                    -GenericUrlRole $GenericUrlRole `
                    -InheritedSourcePageUrl $InheritedSourcePageUrl `
                    -InheritedSourcePageSource $InheritedSourcePageSource
            }
            catch {
            }
        }
        return
    }
    if ($Value -is [System.Collections.IEnumerable] -and
        $Value -isnot [System.Collections.IDictionary] -and
        $Value -isnot [pscustomobject]) {
        $itemIndex = 0
        foreach ($item in $Value) {
            if ($Candidates.Count -ge $script:McpImageCandidateLimit) {
                return
            }
            Add-McpImageCandidates `
                -Value $item `
                -Candidates $Candidates `
                -Seen $Seen `
                -Depth ($Depth + 1) `
                -PathPrefix "$PathPrefix[$itemIndex]" `
                -GenericUrlRole $GenericUrlRole `
                -InheritedSourcePageUrl $InheritedSourcePageUrl `
                -InheritedSourcePageSource $InheritedSourcePageSource
            $itemIndex += 1
        }
        return
    }

    $candidate = Convert-ToMcpImageCandidate `
        -Value $Value `
        -PathPrefix $PathPrefix `
        -GenericUrlRole $GenericUrlRole `
        -InheritedSourcePageUrl $InheritedSourcePageUrl `
        -InheritedSourcePageSource $InheritedSourcePageSource
    if ($null -ne $candidate -and $Seen.Add([string]$candidate.imageUrl)) {
        [void]$Candidates.Add($candidate)
    }
    $childSourcePageLocation = Find-McpStringProperty `
        -Value $Value `
        -Names @("source_page_url", "sourcePageUrl", "source", "page_url", "pageUrl", "url") `
        -PathPrefix $PathPrefix
    $childSourcePageUrl = if ($null -ne $childSourcePageLocation) {
        [string]$childSourcePageLocation.Value
    }
    else {
        $InheritedSourcePageUrl
    }
    $childSourcePageSource = if ($null -ne $childSourcePageLocation) {
        [string]$childSourcePageLocation.Source
    }
    else {
        $InheritedSourcePageSource
    }
    foreach ($property in $Value.PSObject.Properties) {
        if ($property.Name -in @("image_url", "imageUrl", "thumbnail_url", "thumbnailUrl", "src", "data", "url")) {
            continue
        }
        if ($Candidates.Count -ge $script:McpImageCandidateLimit) {
            return
        }
        Add-McpImageCandidates `
            -Value $property.Value `
            -Candidates $Candidates `
            -Seen $Seen `
            -Depth ($Depth + 1) `
            -PathPrefix "$PathPrefix.$($property.Name)" `
            -GenericUrlRole $(if ($property.Name.ToLowerInvariant() -in @(
                "image", "images", "asset", "assets", "photo", "photos", "media"
            )) { "image" } else { $GenericUrlRole }) `
            -InheritedSourcePageUrl $childSourcePageUrl `
            -InheritedSourcePageSource $childSourcePageSource
    }
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

$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
try {
    $query = [string](Get-RequestParamValue -Request $request -Names @("query", "q") -DefaultValue "")
    if ([string]::IsNullOrWhiteSpace($query)) {
        throw "query is required"
    }
    $requestedCount = [Math]::Max(
        1,
        [Math]::Min(6, [int](Get-RequestParamValue -Request $request -Names @("count") -DefaultValue 3))
    )
    $mcpResult = $request.frameworkData.mcp.result
    if ($null -eq $mcpResult) {
        throw "MCP framework result is missing"
    }

    $rawCandidates = [System.Collections.ArrayList]::new()
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    Add-McpImageCandidates -Value $mcpResult -Candidates $rawCandidates -Seen $seen
    if ($rawCandidates.Count -eq 0) {
        throw "MCP image search returned no image candidates"
    }

    $candidates = @()
    foreach ($candidate in @($rawCandidates | Select-Object -First $requestedCount)) {
        $downloadLocation = [string]$candidate.imageUrl
        $downloadLocationSource = [string]$candidate.imageUrlSource
        try {
            $dataUrl = Convert-ImageLocationToDataUrl `
                -Location $downloadLocation `
                -Referer ([string]$candidate.sourcePageUrl)
        }
        catch {
            if ([string]::IsNullOrWhiteSpace([string]$candidate.thumbnailUrl) -or
                [string]$candidate.thumbnailUrl -eq [string]$candidate.imageUrl) {
                continue
            }
            try {
                $downloadLocation = [string]$candidate.thumbnailUrl
                $downloadLocationSource = [string]$candidate.thumbnailUrlSource
                $dataUrl = Convert-ImageLocationToDataUrl `
                    -Location $downloadLocation `
                    -Referer ([string]$candidate.sourcePageUrl)
            }
            catch {
                continue
            }
        }
        $index = $candidates.Count
        # `imageUrl` is the canonical key the host and both candidate consumers read, so the payload is
        # stored under that name and nowhere else. It used to be stored as `data` as well, and the host's
        # normalizer copies whichever key it finds into `imageUrl`, so every candidate's base64 was held
        # and serialized twice for no reader's benefit — six candidates at the 32 MiB per-image cap put
        # roughly half a gigabyte of duplicated string into one response.
        $item = [ordered]@{
            id = "brave-search-$($index + 1)"
            title = if ([string]::IsNullOrWhiteSpace([string]$candidate.title)) {
                "$query #$($index + 1)"
            }
            else {
                [string]$candidate.title
            }
            imageUrl = $dataUrl
            sourceUrl = $candidate.sourcePageUrl
            imageUrlSource = $downloadLocationSource
            thumbnailUrlSource = $candidate.thumbnailUrlSource
            sourceUrlSource = $candidate.sourcePageUrlSource
            width = $candidate.width
            height = $candidate.height
            index = $index
        }
        if (-not $downloadLocation.StartsWith("data:", [System.StringComparison]::OrdinalIgnoreCase)) {
            $item.sourceImageUrl = $downloadLocation
        }
        # The thumbnail helper returns its input unchanged when the image is already small enough, or
        # cannot be decoded. Emitting that as `thumbnail` would be a third copy of the same bytes, so the
        # key is only set when the downscale produced something smaller. The desktop candidate strip
        # renders `thumbnailUrl || imageUrl`, so a candidate without one still paints.
        $thumbnail = New-ImageThumbnailDataUrl -DataUrl $dataUrl
        if ($thumbnail -ne $dataUrl) {
            $item.thumbnail = $thumbnail
        }
        $candidates += $item
    }
    if ($candidates.Count -eq 0) {
        throw "MCP image search returned candidates, but none could be downloaded"
    }

    $selectedIndexValue = Get-RequestParamValue `
        -Request $request `
        -Names @("result_index") `
        -DefaultValue 0
    $selectedIndex = 0
    if (-not [int]::TryParse([string]$selectedIndexValue, [ref]$selectedIndex)) {
        throw "result_index must be an integer"
    }
    if ($selectedIndex -lt 0) {
        throw "result_index must be non-negative"
    }
    if ($selectedIndex -ge $candidates.Count) {
        throw "result_index $selectedIndex is out of range for $($candidates.Count) downloadable candidates"
    }
    $selected = $candidates[$selectedIndex]
    # The selected image already travels twice in this response: once as the chosen candidate's
    # `imageUrl`, once inside `content`. An `output_base64` would make it three copies of the same bytes.
    # This Art declares no `output_path`, so the host cannot strip the duplicate for us the way it
    # does for a file-backed Art; the runtime has to omit it. Every reader falls back to
    # `content[0].data`, and this Art's declared output port is named `output` rather than
    # `output_base64`.
    $output = [ordered]@{
        query = $query
        count = $candidates.Count
        selectedCandidate = $selected.id
        content = @([ordered]@{
            type = "image"
            data = $selected.imageUrl
            mimeType = (Get-ImageMimeType -Location $selected.imageUrl -ContentType "")
        })
    }
    Write-SuccessResponse -Output $output -Candidates $candidates
}
catch {
    Write-ErrorResponse -Code "image_search_failed" -Message $_.Exception.Message
}
