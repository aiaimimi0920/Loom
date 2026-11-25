# Normalize bounded MCP result candidates without downloading them.

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
