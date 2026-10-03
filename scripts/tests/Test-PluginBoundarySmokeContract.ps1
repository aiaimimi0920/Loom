[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
function Assert-Contract([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
$entry = [IO.File]::ReadAllText((Join-Path $repoRoot "scripts\Invoke-LoomPluginBoundarySmoke.ps1"))
foreach ($module in @('Packages', 'SourceIntegrity', 'Diagnostics', 'WebSocket', 'Cleanup')) {
    $relative = "plugin-boundary-smoke\$module.ps1"
    Assert-Contract ($entry.Contains('. (Join-Path $PSScriptRoot "' + $relative + '")')) "Module is not loaded by the smoke: $module"
    $tokens = $null
    $errors = $null
    $null = [Management.Automation.Language.Parser]::ParseFile((Join-Path $repoRoot "scripts\$relative"), [ref]$tokens, [ref]$errors)
    Assert-Contract (@($errors).Count -eq 0) "Module failed PowerShell parsing: $module"
}
foreach ($phase in @('connect', 'subscribe-send', 'subscribe-receive', 'instantiation-receive', 'execute-send', 'execute-receive')) {
    Assert-Contract ($entry.Contains('Set-LoomPluginBoundaryPhase -Phase "' + $phase + '"')) "Missing diagnostic phase: $phase"
}
Assert-Contract ($entry -match '(?s)catch\s*\{\s*\$primaryFailure = \$_\s*Save-LoomPluginBoundaryDiagnostic[^}]+\bthrow\s*\}\s*finally\s*\{\s*Complete-LoomPluginBoundaryCleanup') "Diagnostic must be saved before cleanup and preserve the original failure."
Assert-Contract ($entry.Contains('Remove-Item -LiteralPath (Join-Path $evidencePath "plugin-boundary-diagnostic.json")')) "New smoke run must clear stale diagnostic evidence."
$workflow = [IO.File]::ReadAllText((Join-Path $repoRoot ".github\workflows\build-windows.yml"))
$upload = [regex]::Match($workflow, '(?ms)^      - name: Upload bounded smoke phase diagnostics\r?\n(?<body>.*?)(?=^      - name:|\z)').Groups['body'].Value
Assert-Contract ($upload.Contains('if: failure()')) "Diagnostic artifacts should upload only on failure."
$paths = [regex]::Match($upload, '(?m)^          path: \|\r?\n(?<paths>(?:            [^\r\n]+\r?\n)+)').Groups['paths'].Value
$actual = @($paths -split '\r?\n' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$expected = @('target/runtime-smoke/latest/mcp-smoke-diagnostic.json', 'target/runtime-smoke/plugin-boundary/plugin-boundary-diagnostic.json')
Assert-Contract (($actual -join ',') -eq ($expected -join ',')) "Upload must contain exactly the two bounded JSON files, never raw logs or directories."
Assert-Contract ($upload.Contains('retention-days: 14')) "Diagnostic retention changed."
$testWorkflow = [IO.File]::ReadAllText((Join-Path $repoRoot ".github\workflows\plugin-boundary-diagnostics.yml"))
Assert-Contract ($testWorkflow.Contains('contents: read') -and $testWorkflow.Contains('shell: powershell')) "Pure test workflow must use read-only permissions and Windows PowerShell."
Assert-Contract ($testWorkflow -notmatch 'secrets\.|id-token:|contents: write|build-release|verify-release|attest|gh-release|workflow_run:') "Pure diagnostic tests must not build, sign, or publish candidates."
Write-Output "Plugin boundary module and bounded failure-artifact contracts passed."
