. (Join-Path $PSScriptRoot "common.ps1")
$ErrorActionPreference = "Stop"

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

function Test-ImageLocation {
    param([AllowNull()][object]$Value)

    if ($Value -isnot [string]) {
        return $false
    }
    $location = $Value.Trim()
    if ($location.StartsWith("data:image/", [System.StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }
    if (-not ($location.StartsWith("http://", [System.StringComparison]::OrdinalIgnoreCase) -or
            $location.StartsWith("https://", [System.StringComparison]::OrdinalIgnoreCase))) {
        return $false
    }
    $path = ($location -split '[?#]', 2)[0].ToLowerInvariant()
    return $null -ne (@(".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".svg", ".avif") |
        Where-Object { $path.EndsWith($_, [System.StringComparison]::Ordinal) } |
        Select-Object -First 1)
}

function Convert-ToMcpImageCandidate {
    param([AllowNull()][object]$Value)

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
        return [ordered]@{
            imageUrl = $imageData
            thumbnailUrl = $imageData
            title = "MCP image"
            sourcePageUrl = $null
            width = $null
            height = $null
        }
    }

    $properties = Get-McpPropertyValue -Value $Value -Names @("properties")
    $imageUrl = Get-McpPropertyValue -Value $Value -Names @(
        "image_url", "imageUrl", "thumbnail_url", "thumbnailUrl", "src"
    )
    if ($imageUrl -isnot [string] -or [string]::IsNullOrWhiteSpace($imageUrl)) {
        $imageUrl = Get-McpPropertyValue -Value $properties -Names @(
            "image_url", "imageUrl", "thumbnail_url", "thumbnailUrl", "src", "url"
        )
    }
    $width = Get-McpPropertyValue -Value $Value -Names @("width")
    if ($null -eq $width) {
        $width = Get-McpPropertyValue -Value $properties -Names @("width")
    }
    $height = Get-McpPropertyValue -Value $Value -Names @("height")
    if ($null -eq $height) {
        $height = Get-McpPropertyValue -Value $properties -Names @("height")
    }
    if (($imageUrl -isnot [string] -or [string]::IsNullOrWhiteSpace($imageUrl)) -and
        ($null -ne $width -or $null -ne $height)) {
        $imageUrl = Get-McpPropertyValue -Value $Value -Names @("url")
    }
    if ($imageUrl -isnot [string] -or [string]::IsNullOrWhiteSpace($imageUrl)) {
        return $null
    }

    $thumbnailUrl = Get-McpPropertyValue -Value $Value -Names @(
        "thumbnail_url", "thumbnailUrl", "thumbnail", "placeholder"
    )
    if ($thumbnailUrl -isnot [string] -or [string]::IsNullOrWhiteSpace($thumbnailUrl)) {
        $thumbnailUrl = Get-McpPropertyValue -Value $properties -Names @(
            "thumbnail_url", "thumbnailUrl", "thumbnail", "placeholder"
        )
    }
    $sourcePageUrl = Get-McpPropertyValue -Value $Value -Names @("source_page_url", "sourcePageUrl", "url")
    if ($sourcePageUrl -eq $imageUrl) {
        $sourcePageUrl = $null
    }
    return [ordered]@{
        imageUrl = $imageUrl.Trim()
        thumbnailUrl = if ($thumbnailUrl -is [string]) { $thumbnailUrl.Trim() } else { $null }
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
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][System.Collections.Generic.HashSet[string]]$Seen
    )

    if ($null -eq $Value) {
        return
    }
    if ($Value -is [string]) {
        $text = $Value.Trim()
        if (Test-ImageLocation -Value $text) {
            if ($Seen.Add($text)) {
                [void]$Candidates.Add([ordered]@{
                    imageUrl = $text
                    thumbnailUrl = $text
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
                Add-McpImageCandidates -Value ($text | ConvertFrom-Json) -Candidates $Candidates -Seen $Seen
            }
            catch {
            }
        }
        return
    }
    if ($Value -is [System.Collections.IEnumerable] -and
        $Value -isnot [System.Collections.IDictionary] -and
        $Value -isnot [pscustomobject]) {
        foreach ($item in $Value) {
            Add-McpImageCandidates -Value $item -Candidates $Candidates -Seen $Seen
        }
        return
    }

    $candidate = Convert-ToMcpImageCandidate -Value $Value
    if ($null -ne $candidate -and $Seen.Add([string]$candidate.imageUrl)) {
        [void]$Candidates.Add($candidate)
    }
    foreach ($property in $Value.PSObject.Properties) {
        if ($property.Name -in @("image_url", "imageUrl", "thumbnail_url", "thumbnailUrl", "src", "data", "url")) {
            continue
        }
        Add-McpImageCandidates -Value $property.Value -Candidates $Candidates -Seen $Seen
    }
}

function Get-ImageMimeType {
    param(
        [string]$Location,
        [AllowNull()][string]$ContentType
    )

    if ($Location.StartsWith("data:", [System.StringComparison]::OrdinalIgnoreCase)) {
        $separator = $Location.IndexOf(';')
        if ($separator -gt 5) {
            return $Location.Substring(5, $separator - 5)
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($ContentType)) {
        $mimeType = ($ContentType -split ';', 2)[0].Trim()
        if ($mimeType.StartsWith("image/", [System.StringComparison]::OrdinalIgnoreCase)) {
            return $mimeType
        }
    }
    $path = ($Location -split '[?#]', 2)[0].ToLowerInvariant()
    if ($path.EndsWith(".jpg") -or $path.EndsWith(".jpeg")) { return "image/jpeg" }
    if ($path.EndsWith(".webp")) { return "image/webp" }
    if ($path.EndsWith(".gif")) { return "image/gif" }
    if ($path.EndsWith(".bmp")) { return "image/bmp" }
    if ($path.EndsWith(".svg")) { return "image/svg+xml" }
    if ($path.EndsWith(".avif")) { return "image/avif" }
    return "image/png"
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
    $handler.AllowAutoRedirect = $true
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
        $response = $client.GetAsync(
            $Location,
            [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead
        ).GetAwaiter().GetResult()
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
        $mimeType = Get-ImageMimeType -Location $Location -ContentType $contentType
        return "data:$mimeType;base64,$([Convert]::ToBase64String($bytes))"
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
        try {
            $dataUrl = Convert-ImageLocationToDataUrl `
                -Location ([string]$candidate.imageUrl) `
                -Referer ([string]$candidate.sourcePageUrl)
        }
        catch {
            if ([string]::IsNullOrWhiteSpace([string]$candidate.thumbnailUrl) -or
                [string]$candidate.thumbnailUrl -eq [string]$candidate.imageUrl) {
                continue
            }
            try {
                $dataUrl = Convert-ImageLocationToDataUrl `
                    -Location ([string]$candidate.thumbnailUrl) `
                    -Referer ([string]$candidate.sourcePageUrl)
            }
            catch {
                continue
            }
        }
        $index = $candidates.Count
        $candidates += [ordered]@{
            id = "brave-search-$($index + 1)"
            title = if ([string]::IsNullOrWhiteSpace([string]$candidate.title)) {
                "$query #$($index + 1)"
            }
            else {
                [string]$candidate.title
            }
            thumbnail = $dataUrl
            data = $dataUrl
            sourceUrl = $candidate.sourcePageUrl
            width = $candidate.width
            height = $candidate.height
            index = $index
        }
    }
    if ($candidates.Count -eq 0) {
        throw "MCP image search returned candidates, but none could be downloaded"
    }

    $selectedIndex = 0
    $selectedIndexValue = Get-RequestParamValue `
        -Request $request `
        -Names @("result_index") `
        -DefaultValue 0
    $parsedSelectedIndex = 0
    if ([int]::TryParse([string]$selectedIndexValue, [ref]$parsedSelectedIndex)) {
        $selectedIndex = [Math]::Max(
            0,
            [Math]::Min($candidates.Count - 1, $parsedSelectedIndex)
        )
    }
    $selected = $candidates[$selectedIndex]
    $output = [ordered]@{
        output_base64 = $selected.data
        query = $query
        count = $candidates.Count
        selectedCandidate = $selected.id
        content = @([ordered]@{
            type = "image"
            data = $selected.data
            mimeType = (Get-ImageMimeType -Location $selected.data -ContentType "")
        })
    }
    Write-SuccessResponse -Output $output -Candidates $candidates
}
catch {
    Write-ErrorResponse -Code "image_search_failed" -Message $_.Exception.Message
}
