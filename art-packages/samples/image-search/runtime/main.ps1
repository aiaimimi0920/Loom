$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "common.ps1")

. (Join-Path $PSScriptRoot "image-search-candidates.ps1")
. (Join-Path $PSScriptRoot "image-search-download.ps1")

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
