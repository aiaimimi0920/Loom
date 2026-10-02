[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
. (Join-Path $repoRoot "scripts\smoke-release\Evidence.ps1")
. (Join-Path $repoRoot "scripts\smoke-release\McpDiagnostics.ps1")
. (Join-Path $repoRoot "scripts\smoke-release\ReleasePhases.ps1")
$root = Join-Path ([IO.Path]::GetTempPath()) "loom-mcp-diagnostics-$PID-$([Guid]::NewGuid().ToString('N'))"
$EvidenceRoot = Join-Path $root "evidence"
$VersionId = "diagnostic-contract"
$resolvedApps = @("Loom")
$script:SmokeEvidenceRunId = ""
$script:SmokeEvidenceRunDir = ""
$process = $null
$started = $false
$stderr = $null
New-Item -ItemType Directory -Path $root | Out-Null
function Assert-Diagnostic([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Read-Diagnostic {
    $path = Save-LoomMcpSmokeDiagnostic -TempRoot $root -Phase "connection-test" -Outcome "not-received"
    return Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
}
function Write-TestPhases([string]$Name, [string]$Content) {
    [IO.File]::WriteAllText((Join-Path $root $Name), $Content, [Text.UTF8Encoding]::new($false))
}
try {
    $empty = Read-Diagnostic
    Assert-Diagnostic (@($empty.events).Count -eq 0) "Absent phase files must remain unknown, not a claimed fixture start."
    $utc = [DateTime]::UtcNow.ToString("o")
    $safe = @{ pid = 42; phase = "started"; utc = $utc } | ConvertTo-Json -Compress
    $extra = @{ pid = 42; phase = "started"; utc = $utc; body = "synthetic-private-text" } | ConvertTo-Json -Compress
    $bad = @{ pid = 42; phase = "synthetic-private-text"; utc = $utc } | ConvertTo-Json -Compress
    Write-TestPhases "fixture-mcp-phase-contract.jsonl" "$safe`n$extra`n$bad`nnot-json`n"
    $result = Read-Diagnostic
    Assert-Diagnostic (@($result.events).Count -eq 1) "Diagnostic must accept only the strict phase schema."
    Assert-Diagnostic ($result.rejectedRecordsOrFiles -eq 3) "Unexpected fields and invalid records were not rejected."
    $json = $result | ConvertTo-Json -Depth 8
    Assert-Diagnostic (-not $json.Contains("synthetic-private-text") -and -not $json.Contains($root)) "Diagnostic emitted payload or source paths."
    Write-TestPhases "fixture-mcp-phase-contract.jsonl" ((1..40 | ForEach-Object { $safe }) -join "`n")
    $limited = Read-Diagnostic
    Assert-Diagnostic (@($limited.events).Count -eq 32 -and $limited.truncated) "Phase record cap is missing."
    Write-TestPhases "fixture-mcp-phase-contract.jsonl" ("x" * 16385)
    $large = Read-Diagnostic
    Assert-Diagnostic (@($large.events).Count -eq 0 -and $large.rejectedRecordsOrFiles -eq 1) "Oversized phase file was read into the artifact."
    Remove-Item -LiteralPath (Join-Path $root "fixture-mcp-phase-contract.jsonl")
    foreach ($index in 1..10) { Write-TestPhases "fixture-mcp-phase-$index.jsonl" $safe }
    $many = Read-Diagnostic
    Assert-Diagnostic ($many.fixtureFiles -eq 8 -and @($many.events).Count -eq 8 -and $many.truncated) "Phase file cap is missing."
    Get-ChildItem -LiteralPath $root -Filter "fixture-mcp-phase-*.jsonl" -File | Remove-Item -Force

    $fixturePath = New-LoomFixtureMcpServerScript -TempRoot $root
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $PSHOME "powershell.exe"
    $start.Arguments = '-NoProfile -ExecutionPolicy Bypass -File "' + $fixturePath + '"'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    # Framework Process.Start autoflushes the inherited encoding preamble before BaseStream writes.
    $inputEncoding = [Console]::InputEncoding
    try {
        [Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
        $started = $process.Start()
    } finally { [Console]::InputEncoding = $inputEncoding }
    Assert-Diagnostic $started "Could not start the isolated MCP fixture."
    Assert-Diagnostic ($process.StandardInput.Encoding.GetPreamble().Length -eq 0) "Framework emitted a protocol preamble during process start."
    $stderr = $process.StandardError.ReadToEndAsync()
    Write-Output ("MCP test writer encoding={0}; preambleBytes={1}; transport=UTF8-no-BOM" -f
        $process.StandardInput.Encoding.WebName, $process.StandardInput.Encoding.GetPreamble().Length)
    foreach ($request in @(
        @{ jsonrpc = "2.0"; id = 1; method = "initialize"; params = @{} },
        @{ jsonrpc = "2.0"; method = "notifications/initialized" },
        @{ jsonrpc = "2.0"; id = 2; method = "tools/list" },
        @{ jsonrpc = "2.0"; id = 3; method = "tools/call"; params = @{ arguments = @{ text = "fixture-payload-must-stay-out" } } }
    )) {
        # Match Rust MCP and the existing Windows MCP contracts, not the Framework text writer.
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($request | ConvertTo-Json -Depth 8 -Compress) + "`n")
        Assert-Diagnostic ($bytes[0] -eq 123) "Fixture request has an unexpected preamble."
        $process.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
        $process.StandardInput.BaseStream.Flush()
        if (-not $request.ContainsKey("id")) { continue }
        $line = $process.StandardOutput.ReadLineAsync()
        Assert-Diagnostic ($line.Wait(10000)) "Fixture protocol response timed out."
        Assert-Diagnostic (-not [string]::IsNullOrWhiteSpace($line.Result)) "Fixture exited before a JSON response."
        $response = $line.Result | ConvertFrom-Json
        Assert-Diagnostic ($response.id -eq $request.id -and $response.jsonrpc -eq "2.0") "Fixture protocol response identity changed."
        if ($request.id -eq 1) { Assert-Diagnostic ($response.result.serverInfo.name -eq "release-fixture") "Fixture initialize response changed." }
        if ($request.id -eq 2) { Assert-Diagnostic ($response.result.tools[0].name -eq "echo") "Fixture tools list changed." }
        if ($request.id -eq 3) { Assert-Diagnostic ($response.result.content[0].text -eq "fixture-payload-must-stay-out") "Fixture echo response changed." }
    }
    $process.StandardInput.BaseStream.Close()
    Assert-Diagnostic ($process.WaitForExit(10000) -and $process.ExitCode -eq 0) "Fixture did not close cleanly."
    Assert-Diagnostic ($stderr.Wait(1000) -and [string]::IsNullOrWhiteSpace($stderr.Result)) "Fixture wrote unexpected stderr."
    $protocol = Read-Diagnostic
    Assert-Diagnostic ($protocol.rejectedRecordsOrFiles -eq 0) "Actual fixture phases failed the strict diagnostic schema."
    $phases = @($protocol.events | ForEach-Object { $_.phase })
    Assert-Diagnostic (($phases -join ",") -eq "started,initialize-received,initialize-written,initialized-received,tools-list-received,tools-list-written,tools-call-received,tools-call-written,eof") "Fixture phase ordering or protocol changed."
    Assert-Diagnostic (-not ($protocol | ConvertTo-Json -Depth 8).Contains("fixture-payload-must-stay-out")) "Fixture body entered the phase artifact."
} catch {
    # Only this closed-schema phase projection and a fixed category may reach CI logs.
    try { Write-Output ("MCP fixture failure phases: " + ((Read-Diagnostic) | ConvertTo-Json -Depth 8 -Compress)) } catch { }
    $category = "unavailable"
    if ($null -ne $stderr -and $stderr.IsCompleted) {
        $category = if ($stderr.Result -match 'ConvertFrom-Json') { "json-parse" }
            elseif ($stderr.Result -match 'ParserError') { "script-parse" } else { "unclassified" }
    }
    Write-Output "MCP fixture stderr category=$category"
    throw
} finally {
    if ($null -ne $process) {
        if ($started -and -not $process.HasExited) { $process.Kill(); $null = $process.WaitForExit(5000) }
        $process.Dispose()
    }
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
Write-Output "Bounded MCP smoke diagnostics and unchanged fixture protocol passed."
