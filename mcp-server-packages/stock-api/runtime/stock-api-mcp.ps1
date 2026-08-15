[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$nodePath = Join-Path $PSScriptRoot "node\node.exe"
$entryPath = Join-Path $PSScriptRoot "stock-api-entry.js"
if (-not (Test-Path -LiteralPath $nodePath -PathType Leaf)) {
    throw "Bundled stock-api Node.js runtime is missing: $nodePath"
}
if (-not (Test-Path -LiteralPath $entryPath -PathType Leaf)) {
    throw "Bundled stock-api MCP entry is missing: $entryPath"
}

& $nodePath $entryPath
exit $LASTEXITCODE
