<# Owns bounded, schema-checked MCP fixture phase exports; never payloads or logs. #>
function Save-LoomMcpSmokeDiagnostic {
    param(
        [Parameter(Mandatory = $true)][string]$TempRoot,
        [ValidateSet("not-started", "connection-test", "connection-complete")][string]$Phase = "not-started",
        [ValidateSet("not-received", "success", "failure")][string]$Outcome = "not-received"
    )

    $root = Get-Item -LiteralPath $TempRoot -Force
    if (-not $root.PSIsContainer -or ($root.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "MCP diagnostic source must be a non-reparse directory."
    }
    $events = New-Object 'System.Collections.Generic.List[object]'
    $allowedPhases = @("started", "initialize-received", "initialize-written", "initialized-received",
        "tools-list-received", "tools-list-written", "tools-call-received", "tools-call-written", "unknown-method", "eof")
    $files = 0
    $rejected = 0
    $truncated = $false
    $paths = [System.IO.Directory]::EnumerateFiles($root.FullName, "fixture-mcp-phase-*.jsonl").GetEnumerator()
    try {
        while ($paths.MoveNext()) {
            $files++
            if ($files -gt 8) { $truncated = $true; break }
            $stream = $null
            try {
                $item = Get-Item -LiteralPath $paths.Current -Force
                if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                    $rejected++; continue
                }
                if ($item.Length -gt 16384) { $rejected++; $truncated = $true; continue }
                $stream = [System.IO.FileStream]::new($item.FullName, [System.IO.FileMode]::Open,
                    [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
                $bytes = New-Object byte[] 16385
                $count = 0
                while ($count -lt $bytes.Length) {
                    $read = $stream.Read($bytes, $count, $bytes.Length - $count)
                    if ($read -eq 0) { break }
                    $count += $read
                }
                if ($count -gt 16384) { $rejected++; $truncated = $true; continue }
                $text = [System.Text.UTF8Encoding]::new($false, $true).GetString($bytes, 0, $count)
                $lines = 0
                foreach ($line in @($text -split "`n")) {
                    if ([string]::IsNullOrWhiteSpace($line)) { continue }
                    $lines++
                    if ($lines -gt 32) { $truncated = $true; break }
                    try {
                        $value = $line | ConvertFrom-Json -ErrorAction Stop
                        $names = @($value.PSObject.Properties.Name)
                        $pidValue = 0L
                        $utc = [DateTime]::MinValue
                        if ($names.Count -ne 3 -or @($names | Where-Object { $_ -notin @("pid", "phase", "utc") }).Count -ne 0 -or
                            $value.phase -isnot [string] -or $value.phase -notin $allowedPhases -or
                            ($value.pid -isnot [long] -and $value.pid -isnot [int]) -or
                            -not [long]::TryParse([string]$value.pid, [ref]$pidValue) -or $pidValue -lt 1 -or $pidValue -gt 2147483647 -or
                            $value.utc -isnot [string] -or $value.utc -notmatch '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{7}Z$' -or
                            -not [DateTime]::TryParseExact($value.utc, "o", [Globalization.CultureInfo]::InvariantCulture,
                                [Globalization.DateTimeStyles]::RoundtripKind, [ref]$utc)) {
                            $rejected++; continue
                        }
                        $events.Add([ordered]@{ pid = $pidValue; phase = [string]$value.phase; utc = $utc.ToUniversalTime().ToString("o") })
                    } catch { $rejected++ }
                }
            } catch { $rejected++ }
            finally { if ($null -ne $stream) { $stream.Dispose() } }
        }
    } finally { if ($paths -is [IDisposable]) { $paths.Dispose() } }
    $report = [ordered]@{ schemaVersion = 1; phase = $Phase; response = $Outcome;
        fixtureFiles = [Math]::Min($files, 8); rejectedRecordsOrFiles = $rejected; truncated = $truncated;
        events = @($events.ToArray()) }
    return Write-SmokeJsonEvidence -FileName "mcp-smoke-diagnostic.json" -Value $report -Latest
}
