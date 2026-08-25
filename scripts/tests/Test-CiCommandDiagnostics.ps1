[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$runner = Join-Path $repoRoot "scripts\Invoke-CiCommand.ps1"
$root = Join-Path ([System.IO.Path]::GetTempPath()) "loom-ci-command-$([System.Guid]::NewGuid().ToString('N'))"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

New-Item -ItemType Directory -Path $root | Out-Null
$previousGitHubActions = $env:GITHUB_ACTIONS
try {
    $success = Join-Path $root "success.cmd"
    [System.IO.File]::WriteAllText($success, "@echo off`r`necho ci-success-marker:%1:%2`r`n", [System.Text.Encoding]::ASCII)
    $successOutput = @(
        & $runner -Title "success fixture" -Executable $success -CommandArguments @("first", "second") 2>&1
    )
    Assert-True -Condition (($successOutput -join "`n").Contains("ci-success-marker:first:second")) -Message "CI command wrapper lost output or command arguments."

    $failure = Join-Path $root "failure.cmd"
    [System.IO.File]::WriteAllText(
        $failure,
        "@echo off`r`necho api_key=diagnostic-secret`r`necho final-diagnostic-marker`r`nexit /b 7`r`n",
        [System.Text.Encoding]::ASCII
    )
    $env:GITHUB_ACTIONS = "true"
    $captured = [System.Collections.Generic.List[string]]::new()
    $failed = $false
    try {
        & $runner -Title "failure fixture" -Executable $failure 2>&1 | ForEach-Object {
            $captured.Add($_.ToString())
        }
    }
    catch {
        $failed = $true
        $captured.Add($_.Exception.Message)
    }
    $failureOutput = $captured.ToArray() -join "`n"
    Assert-True -Condition $failed -Message "CI command wrapper accepted a failing command."
    Assert-True -Condition $failureOutput.Contains("::error title=failure fixture::") -Message "CI command wrapper omitted its GitHub annotation."
    Assert-True -Condition $failureOutput.Contains("final-diagnostic-marker") -Message "CI command wrapper omitted the failure tail."
    Assert-True -Condition (-not $failureOutput.Contains("diagnostic-secret")) -Message "CI command wrapper exposed a secret-shaped value."
    Assert-True -Condition $failureOutput.Contains("api_key=[REDACTED]") -Message "CI command wrapper did not mark redacted output."
}
finally {
    $env:GITHUB_ACTIONS = $previousGitHubActions
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output "CI command diagnostic contract passed."
