. (Join-Path $PSScriptRoot "common.ps1")
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$workRoot = Get-RequestWorkRoot -Request $request

try {
    $query = [string](Get-RequestParamValue -Request $request -Names @("query", "q") -DefaultValue "")
    if ([string]::IsNullOrWhiteSpace($query)) {
        throw "query is required"
    }
    $count = [Math]::Max(1, [Math]::Min(6, [int](Get-RequestParamValue -Request $request -Names @("count") -DefaultValue 3)))
    $colors = @(
        @(38, 92, 148),
        @(128, 74, 144),
        @(182, 106, 46),
        @(38, 132, 102),
        @(156, 54, 82),
        @(90, 94, 150)
    )
    $candidates = @()
    for ($index = 0; $index -lt $count; $index++) {
        $candidatePath = Join-Path $workRoot ("search-candidate-{0}.png" -f ($index + 1))
        $color = $colors[$index % $colors.Count]
        New-PlaceholderImage -Path $candidatePath -Red $color[0] -Green $color[1] -Blue $color[2] -Label ("{0} #{1}" -f $query, ($index + 1))
        $candidateData = Convert-ImagePathToDataUrl -Path $candidatePath
        $candidates += [ordered]@{
            id = "package-search-$($index + 1)"
            title = "$query #$($index + 1)"
            thumbnail = $candidateData
            data = $candidateData
            cachedPath = $candidatePath
            index = $index
        }
    }
    $first = $candidates[0]
    $output = [ordered]@{
        output_base64 = $first.data
        output_path = $first.cachedPath
        width = 256
        height = 160
        query = $query
        count = $count
        selectedCandidate = $first.id
        content = @([ordered]@{ type = "image"; data = $first.data; mimeType = "image/png" })
    }
    Write-SuccessResponse -Output $output -Candidates $candidates
}
catch {
    Write-ErrorResponse -Code "image_search_failed" -Message $_.Exception.Message
}
