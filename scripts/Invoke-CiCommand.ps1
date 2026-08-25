[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateNotNullOrEmpty()][string]$Title,
    [Parameter(Mandatory = $true)][ValidateNotNullOrEmpty()][string]$Executable,
    [string[]]$CommandArguments = @(),
    [string]$WorkingDirectory = ".",
    [ValidateRange(1, 100)][int]$TailLines = 30,
    [ValidateRange(256, 4000)][int]$MaxDiagnosticCharacters = 2000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Protect-CiDiagnostic {
    param([AllowEmptyString()][string]$Value)

    $protected = [regex]::Replace(
        $Value,
        '(?i)(token|secret|password|api[_-]?key)\s*[:=]\s*\S+',
        '$1=[REDACTED]'
    )
    return [regex]::Replace($protected, '(?i)\bgh[pousr]_[A-Za-z0-9_]+\b', '[REDACTED]')
}

function ConvertTo-GitHubCommandValue {
    param([AllowEmptyString()][string]$Value)

    return $Value.Replace("%", "%25").Replace("`r", "%0D").Replace("`n", "%0A")
}

$resolvedWorkingDirectory = [System.IO.Path]::GetFullPath($WorkingDirectory)
$tail = [System.Collections.Generic.Queue[string]]::new()
$locationPushed = $false
$previousErrorActionPreference = $ErrorActionPreference
$exitCode = 1

try {
    Push-Location -LiteralPath $resolvedWorkingDirectory
    $locationPushed = $true
    $ErrorActionPreference = "Continue"
    & $Executable @CommandArguments 2>&1 | ForEach-Object {
        $line = Protect-CiDiagnostic -Value $_.ToString()
        Write-Output $line
        if ($tail.Count -ge $TailLines) {
            [void]$tail.Dequeue()
        }
        $tail.Enqueue($line)
    }
    $exitCode = $LASTEXITCODE
}
catch {
    $line = Protect-CiDiagnostic -Value $_.Exception.ToString()
    Write-Output $line
    if ($tail.Count -ge $TailLines) {
        [void]$tail.Dequeue()
    }
    $tail.Enqueue($line)
}
finally {
    $ErrorActionPreference = $previousErrorActionPreference
    if ($locationPushed) {
        Pop-Location
    }
}

if ($exitCode -eq 0) {
    exit 0
}

$diagnostic = if ($tail.Count -gt 0) {
    $tail.ToArray() -join " | "
}
else {
    "No command output was captured."
}
if ($diagnostic.Length -gt $MaxDiagnosticCharacters) {
    $diagnostic = $diagnostic.Substring($diagnostic.Length - $MaxDiagnosticCharacters)
}
if ([string]::Equals($env:GITHUB_ACTIONS, "true", [System.StringComparison]::OrdinalIgnoreCase)) {
    $escapedTitle = ConvertTo-GitHubCommandValue -Value $Title
    $escapedDiagnostic = ConvertTo-GitHubCommandValue -Value $diagnostic
    Write-Output "::error title=${escapedTitle}::$escapedDiagnostic"
}
throw "CI command failed with exit code ${exitCode}: $Title"
