[CmdletBinding()]
param(
    # Declared only so that a manifest which still passes the old switch fails loudly instead of
    # being silently ignored. The endpoint is not configurable; see $script:BraveEndpoint below.
    [AllowNull()][AllowEmptyString()]
    [string]$Endpoint
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:ToolName = "brave_image_search"
$script:MaximumResponseBytes = 8MB

# The only endpoint this server ever talks to. It is a constant rather than an input because every
# request carries the user's Brave subscription key in `X-Subscription-Token`: whoever chooses the
# endpoint chooses where that key is sent. A package manifest supplies both `entry.args` and
# `entry.env`, and both reach this process verbatim, so neither a switch nor a bare environment
# variable is a place where the destination of a credential can be decided.
$script:BraveEndpoint = "https://api.search.brave.com/res/v1/images/search"

# Offline tests still have to point the server at a local fixture. That stays possible, but only at
# a loopback address: whatever supplies the value — a test harness or a hostile manifest — the key
# cannot leave the machine, so redirecting it is no longer a way to collect it remotely.
$script:EndpointOverrideVariable = "LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE"

function Test-LoopbackEndpoint {
    param([Parameter(Mandatory = $true)][Uri]$Uri)

    if (@("http", "https") -notcontains $Uri.Scheme) {
        return $false
    }
    # Credentials embedded in the override would be sent to the fixture as well.
    if (-not [string]::IsNullOrEmpty($Uri.UserInfo)) {
        return $false
    }
    $hostName = $Uri.DnsSafeHost
    if ([string]::IsNullOrWhiteSpace($hostName)) {
        return $false
    }
    if ($hostName -eq "localhost") {
        return $true
    }
    # Only literal loopback addresses count: a resolvable name could be made to point anywhere,
    # and re-resolving it here would not be what the HTTP client resolves later anyway.
    $address = $null
    if (-not [System.Net.IPAddress]::TryParse($hostName, [ref]$address)) {
        return $false
    }
    return [System.Net.IPAddress]::IsLoopback($address)
}

function Resolve-SearchEndpoint {
    if (-not [string]::IsNullOrWhiteSpace($Endpoint)) {
        throw "The image-search endpoint is fixed and -Endpoint is not accepted; set $($script:EndpointOverrideVariable) to a loopback URL for offline tests."
    }
    $override = [Environment]::GetEnvironmentVariable($script:EndpointOverrideVariable)
    if ([string]::IsNullOrWhiteSpace($override)) {
        return $script:BraveEndpoint
    }
    $uri = $null
    if (-not [Uri]::TryCreate($override.Trim(), [UriKind]::Absolute, [ref]$uri) -or
        -not (Test-LoopbackEndpoint -Uri $uri)) {
        throw "$($script:EndpointOverrideVariable) must be an unauthenticated loopback http(s) URL"
    }
    return $uri.AbsoluteUri
}

try {
    $script:SearchEndpoint = Resolve-SearchEndpoint
}
catch {
    # Refusing to start is the point: a server that cannot say where the key goes must not send it.
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}

[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

function Get-PropertyValue {
    param(
        [AllowNull()][object]$Value,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($null -eq $Value) {
        return $null
    }
    $property = $Value.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function ConvertTo-HttpUrl {
    param([AllowNull()][object]$Value)

    if ($Value -isnot [string] -or [string]::IsNullOrWhiteSpace($Value)) {
        return $null
    }
    $uri = $null
    if (-not [Uri]::TryCreate($Value.Trim(), [UriKind]::Absolute, [ref]$uri)) {
        return $null
    }
    if ($uri.Scheme -ne "http" -and $uri.Scheme -ne "https") {
        return $null
    }
    return $uri.AbsoluteUri
}

function ConvertTo-OptionalPositiveInteger {
    param([AllowNull()][object]$Value)

    if ($null -eq $Value) {
        return $null
    }
    $parsed = 0
    if (-not [int]::TryParse([string]$Value, [ref]$parsed) -or $parsed -le 0) {
        return $null
    }
    return $parsed
}

function ConvertTo-BraveImageCandidate {
    param([AllowNull()][object]$Result)

    if ($null -eq $Result) {
        return $null
    }
    $properties = Get-PropertyValue -Value $Result -Name "properties"
    $thumbnail = Get-PropertyValue -Value $Result -Name "thumbnail"
    $imageUrl = ConvertTo-HttpUrl (Get-PropertyValue -Value $Result -Name "url")
    if ($null -eq $imageUrl) {
        $imageUrl = ConvertTo-HttpUrl (Get-PropertyValue -Value $properties -Name "url")
    }
    $thumbnailUrl = ConvertTo-HttpUrl (Get-PropertyValue -Value $thumbnail -Name "src")
    if ($null -eq $imageUrl) {
        $imageUrl = $thumbnailUrl
    }
    if ($null -eq $imageUrl) {
        return $null
    }

    $titleValue = Get-PropertyValue -Value $Result -Name "title"
    $sourcePageUrl = ConvertTo-HttpUrl (Get-PropertyValue -Value $Result -Name "source")
    return [ordered]@{
        imageUrl = $imageUrl
        thumbnailUrl = $thumbnailUrl
        title = if ($titleValue -is [string]) { $titleValue.Trim() } else { "" }
        sourcePageUrl = $sourcePageUrl
        width = ConvertTo-OptionalPositiveInteger (Get-PropertyValue -Value $properties -Name "width")
        height = ConvertTo-OptionalPositiveInteger (Get-PropertyValue -Value $properties -Name "height")
    }
}

function ConvertTo-ImageCandidates {
    param(
        [Parameter(Mandatory = $true)][object]$Response,
        [Parameter(Mandatory = $true)][int]$Count
    )

    $results = Get-PropertyValue -Value $Response -Name "results"
    if ($null -eq $results) {
        return @()
    }
    $candidates = @()
    foreach ($result in @($results)) {
        $candidate = ConvertTo-BraveImageCandidate -Result $result
        if ($null -ne $candidate) {
            $candidates += $candidate
        }
        if ($candidates.Count -ge $Count) {
            break
        }
    }
    return @($candidates)
}

function New-SearchUri {
    param(
        [Parameter(Mandatory = $true)][string]$Query,
        [Parameter(Mandatory = $true)][int]$Count
    )

    $builder = [UriBuilder]::new($script:SearchEndpoint)
    $parameters = @(
        "q=$([Uri]::EscapeDataString($Query))",
        "count=$Count",
        "safesearch=strict"
    )
    if (-not [string]::IsNullOrWhiteSpace($builder.Query)) {
        $existing = $builder.Query.TrimStart('?')
        $builder.Query = "$existing&$($parameters -join '&')"
    }
    else {
        $builder.Query = $parameters -join '&'
    }
    return $builder.Uri
}

function Invoke-BraveImageSearch {
    param(
        [Parameter(Mandatory = $true)][string]$Query,
        [Parameter(Mandatory = $true)][int]$Count
    )

    $apiKey = [Environment]::GetEnvironmentVariable("BRAVE_API_KEY")
    if ([string]::IsNullOrWhiteSpace($apiKey)) {
        throw "BRAVE_API_KEY is required"
    }

    Add-Type -AssemblyName System.Net.Http
    $handler = [System.Net.Http.HttpClientHandler]::new()
    $handler.UseProxy = $false
    $handler.AutomaticDecompression =
        [System.Net.DecompressionMethods]::GZip -bor [System.Net.DecompressionMethods]::Deflate
    $client = [System.Net.Http.HttpClient]::new($handler)
    $client.Timeout = [TimeSpan]::FromSeconds(45)
    $request = [System.Net.Http.HttpRequestMessage]::new(
        [System.Net.Http.HttpMethod]::Get,
        (New-SearchUri -Query $Query -Count $Count)
    )
    [void]$request.Headers.TryAddWithoutValidation("Accept", "application/json")
    [void]$request.Headers.TryAddWithoutValidation("X-Subscription-Token", $apiKey)
    try {
        $response = $client.SendAsync(
            $request,
            [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead
        ).GetAwaiter().GetResult()
        try {
            if (-not $response.IsSuccessStatusCode) {
                throw "Brave image search returned HTTP $([int]$response.StatusCode)"
            }
            if ($response.Content.Headers.ContentLength -and
                [long]$response.Content.Headers.ContentLength -gt $script:MaximumResponseBytes) {
                throw "Brave image search response exceeds 8 MiB"
            }
            $stream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $memory = [System.IO.MemoryStream]::new()
            $buffer = New-Object byte[] 81920
            try {
                while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    if ($memory.Length + $read -gt $script:MaximumResponseBytes) {
                        throw "Brave image search response exceeds 8 MiB"
                    }
                    $memory.Write($buffer, 0, $read)
                }
                $json = [Text.Encoding]::UTF8.GetString($memory.ToArray())
            }
            finally {
                $stream.Dispose()
                $memory.Dispose()
            }
            return $json | ConvertFrom-Json
        }
        finally {
            $response.Dispose()
        }
    }
    finally {
        $request.Dispose()
        $client.Dispose()
        $handler.Dispose()
    }
}

function Write-JsonRpcMessage {
    param([Parameter(Mandatory = $true)][object]$Message)

    [Console]::Out.WriteLine(($Message | ConvertTo-Json -Depth 30 -Compress))
    [Console]::Out.Flush()
}

function Write-JsonRpcResult {
    param(
        [AllowNull()][object]$Id,
        [Parameter(Mandatory = $true)][object]$Result
    )

    Write-JsonRpcMessage -Message ([ordered]@{
        jsonrpc = "2.0"
        id = $Id
        result = $Result
    })
}

function Write-JsonRpcError {
    param(
        [AllowNull()][object]$Id,
        [Parameter(Mandatory = $true)][int]$Code,
        [Parameter(Mandatory = $true)][string]$Message
    )

    Write-JsonRpcMessage -Message ([ordered]@{
        jsonrpc = "2.0"
        id = $Id
        error = [ordered]@{
            code = $Code
            message = $Message
        }
    })
}

function Get-ToolsListResult {
    return [ordered]@{
        tools = @([ordered]@{
            name = $script:ToolName
            description = "Searches Brave for image candidates used by the Loom image-search Art."
            inputSchema = [ordered]@{
                type = "object"
                properties = [ordered]@{
                    query = [ordered]@{ type = "string"; minLength = 1; maxLength = 512 }
                    count = [ordered]@{ type = "integer"; minimum = 1; maximum = 6; default = 3 }
                }
                required = @("query")
                additionalProperties = $false
            }
        })
    }
}

function Invoke-ToolCall {
    param([Parameter(Mandatory = $true)][object]$Parameters)

    $name = [string](Get-PropertyValue -Value $Parameters -Name "name")
    if ($name -ne $script:ToolName) {
        throw "unknown MCP tool: $name"
    }
    $arguments = Get-PropertyValue -Value $Parameters -Name "arguments"
    $queryValue = Get-PropertyValue -Value $arguments -Name "query"
    $query = if ($queryValue -is [string]) { $queryValue.Trim() } else { "" }
    if ([string]::IsNullOrWhiteSpace($query)) {
        throw "query is required"
    }
    if ($query.Length -gt 512) {
        throw "query exceeds 512 characters"
    }
    foreach ($property in $arguments.PSObject.Properties) {
        if ($property.Name -ne "query" -and $property.Name -ne "count") {
            throw "unknown image-search argument: $($property.Name)"
        }
    }
    $count = 3
    $countValue = Get-PropertyValue -Value $arguments -Name "count"
    if ($null -ne $countValue) {
        $parsedCount = 0
        if (-not [int]::TryParse([string]$countValue, [ref]$parsedCount)) {
            throw "count must be an integer"
        }
        if ($parsedCount -lt 1 -or $parsedCount -gt 6) {
            throw "count must be between 1 and 6"
        }
        $count = $parsedCount
    }

    $response = Invoke-BraveImageSearch -Query $query -Count $count
    $candidates = @(ConvertTo-ImageCandidates -Response $response -Count $count)
    if ($candidates.Count -eq 0) {
        throw "Brave image search returned no usable image candidates"
    }
    $structured = [ordered]@{
        query = $query
        count = $candidates.Count
        candidates = $candidates
    }
    return [ordered]@{
        content = @([ordered]@{
            type = "text"
            text = ($structured | ConvertTo-Json -Depth 20 -Compress)
        })
        structuredContent = $structured
        isError = $false
    }
}

while ($null -ne ($line = [Console]::In.ReadLine())) {
    $line = $line.TrimStart([char]0xFEFF)
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $message = $null
    try {
        $message = $line | ConvertFrom-Json
    }
    catch {
        Write-JsonRpcError -Id $null -Code -32700 -Message "invalid JSON-RPC payload"
        continue
    }

    $id = Get-PropertyValue -Value $message -Name "id"
    $method = [string](Get-PropertyValue -Value $message -Name "method")
    if ([string](Get-PropertyValue -Value $message -Name "jsonrpc") -ne "2.0" -or
        [string]::IsNullOrWhiteSpace($method)) {
        Write-JsonRpcError -Id $id -Code -32600 -Message "invalid JSON-RPC request"
        continue
    }
    if ($method -eq "notifications/initialized") {
        continue
    }

    try {
        switch ($method) {
            "initialize" {
                Write-JsonRpcResult -Id $id -Result ([ordered]@{
                    protocolVersion = "2024-11-05"
                    capabilities = [ordered]@{ tools = [ordered]@{ listChanged = $false } }
                    serverInfo = [ordered]@{ name = "neuro-image-search-mcp"; version = "1.0.0" }
                })
            }
            "tools/list" {
                Write-JsonRpcResult -Id $id -Result (Get-ToolsListResult)
            }
            "tools/call" {
                $parameters = Get-PropertyValue -Value $message -Name "params"
                if ($null -eq $parameters) {
                    throw "tools/call params are required"
                }
                Write-JsonRpcResult -Id $id -Result (Invoke-ToolCall -Parameters $parameters)
            }
            default {
                Write-JsonRpcError -Id $id -Code -32601 -Message "method not found: $method"
            }
        }
    }
    catch {
        Write-JsonRpcError -Id $id -Code -32000 -Message $_.Exception.Message
    }
}
