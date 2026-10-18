param([string]$Version)
$ErrorActionPreference = 'Stop'
while ($null -ne ($line = [Console]::ReadLine())) {
    $request = $line | ConvertFrom-Json
    if ($null -eq $request.id) { continue }
    $result = switch ($request.method) {
        'initialize' { @{ protocolVersion = '2024-11-05'; capabilities = @{ tools = @{} }; serverInfo = @{ name = 'version-test'; version = $Version } } }
        'tools/list' { @{ tools = @(@{ name = 'version'; description = 'Return installed version'; inputSchema = @{ type = 'object'; properties = @{} } }) } }
        'tools/call' { @{ content = @(@{ type = 'text'; text = $Version }) } }
        default { @{} }
    }
    @{ jsonrpc = '2.0'; id = $request.id; result = $result } | ConvertTo-Json -Depth 10 -Compress | ForEach-Object { [Console]::WriteLine($_) }
}
