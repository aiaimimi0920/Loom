# Owns bounded UTF-8 child-process transport and failure diagnostics for runtime contracts.
$script:StockRuntimeInvocationSequence = 0

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)
    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) { return $Argument }
    $escaped = [regex]::Replace($Argument, '(\\*)"', '$1$1\"')
    $escaped = [regex]::Replace($escaped, '(\\+)$', '$1$1')
    return '"' + $escaped + '"'
}

function New-StockRuntimeCapture {
    param([IO.Stream]$Stream, [int]$Limit)
    $buffer = [byte[]]::new(8192)
    return @{
        Stream = $Stream; Buffer = $buffer; Limit = $Limit; Bytes = [long]0
        Data = [IO.MemoryStream]::new(); Ended = $false
        Task = $Stream.ReadAsync($buffer, 0, $buffer.Length)
    }
}

function Receive-StockRuntimeCapture {
    param([hashtable]$Capture)
    # Alternate the pipes in bounded batches, including while stdin is still blocked.
    for ($batch = 0; $batch -lt 16 -and -not $Capture.Ended -and $Capture.Task.IsCompleted; $batch++) {
        $read = $Capture.Task.GetAwaiter().GetResult()
        if ($read -eq 0) { $Capture.Ended = $true; break }
        $keep = [Math]::Min($read, $Capture.Limit - [int]$Capture.Data.Length)
        if ($keep -gt 0) { $Capture.Data.Write($Capture.Buffer, 0, $keep) }
        $Capture.Bytes += $read
        $Capture.Task = $Capture.Stream.ReadAsync($Capture.Buffer, 0, $Capture.Buffer.Length)
    }
}

function Stop-StockRuntimeProcess {
    param([Diagnostics.Process]$Process)
    if (-not $Process.HasExited) {
        try { $Process.Kill() }
        catch { if (-not $Process.HasExited) { throw } }
    }
    if (-not $Process.WaitForExit(5000)) { throw "Stock Monitor runtime did not stop within 5000ms." }
}

function Get-StockRuntimeDiagnostic {
    param([AllowNull()][hashtable]$Capture)
    if ($null -eq $Capture) { return "Category=unavailable" }
    # Child output can contain credentials or echoed requests. Never include arbitrary text,
    # including JSON parser exception messages; extract only fixed categories and numeric lines.
    $text = [Text.Encoding]::UTF8.GetString($Capture.Data.ToArray())
    if ($text -match 'ParserError') {
        $line = if ($text -match ':([1-9][0-9]{0,5})\s+char:[1-9][0-9]{0,5}(?:\s|$)') { $Matches[1] } else { "unknown" }
        return "Category=PowerShellParserError Line=$line"
    }
    return "Category=unclassified"
}

function Invoke-StockRuntimeRequest {
    param(
        [string]$ArtDirectory,
        [object]$Request,
        [ValidateRange(100, 20000)][int]$TimeoutMilliseconds = 20000
    )

    $invocation = ++$script:StockRuntimeInvocationSequence
    $requestJson = $Request | ConvertTo-Json -Depth 40 -Compress
    $requestBytes = [Text.UTF8Encoding]::new($false, $true).GetBytes($requestJson + "`n")
    $runtime = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $ArtDirectory "art.runtime.json") | ConvertFrom-Json
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = [string]$runtime.entry.command
    $psi.Arguments = @($runtime.entry.args | ForEach-Object { ConvertTo-ProcessArgument ([string]$_) }) -join " "
    $psi.WorkingDirectory = $ArtDirectory
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    $started = $false
    $stdout = $null
    $stderr = $null
    $stdinTask = $null
    $reason = "process-start"
    try {
        Assert-True $process.Start() "Failed to start Stock Monitor runtime."
        $started = $true
        $reason = "capture-start"
        $stdout = New-StockRuntimeCapture $process.StandardOutput.BaseStream ([int]$runtime.limits.maxStdoutBytes)
        $stderr = New-StockRuntimeCapture $process.StandardError.BaseStream ([int]$runtime.limits.maxStderrBytes)
        # StreamWriter uses the Windows ANSI/OEM default; the protocol requires UTF-8 bytes.
        $reason = "stdin-write"
        $stdinTask = $process.StandardInput.BaseStream.WriteAsync($requestBytes, 0, $requestBytes.Length)
        $inputClosed = $false
        $watch = [Diagnostics.Stopwatch]::StartNew()
        while ($true) {
            $reason = "pipe-read"
            Receive-StockRuntimeCapture $stdout
            Receive-StockRuntimeCapture $stderr
            if ($stdout.Bytes -gt $stdout.Limit) { $reason = "stdout-limit"; throw $reason }
            if ($stderr.Bytes -gt $stderr.Limit) { $reason = "stderr-limit"; throw $reason }
            if (-not $inputClosed -and $stdinTask.IsCompleted) {
                $reason = "stdin-write"
                $null = $stdinTask.GetAwaiter().GetResult()
                $process.StandardInput.BaseStream.Close()
                $inputClosed = $true
                $watch.Restart()
            }
            if ($inputClosed -and $process.HasExited -and $stdout.Ended -and $stderr.Ended) { break }
            if ($watch.ElapsedMilliseconds -ge $TimeoutMilliseconds) {
                $reason = if ($inputClosed) { "execution-timeout" } else { "stdin-timeout" }
                throw $reason
            }
            Start-Sleep -Milliseconds 10
        }
        $reason = "child-exit"
        Assert-Equal 0 $process.ExitCode "Child exited with an error."
        $reason = "stdout-utf8"
        $output = [Text.UTF8Encoding]::new($false, $true).GetString($stdout.Data.ToArray())
        $reason = "stdout-empty"
        Assert-True (-not [string]::IsNullOrWhiteSpace($output)) "Child returned no stdout."
        $reason = "stdout-json"
        return $output.Trim() | ConvertFrom-Json
    }
    catch {
        $exceptionType = $_.Exception.GetBaseException().GetType().Name
        $cleanup = "ok"
        if ($started) {
            try { Stop-StockRuntimeProcess $process }
            catch { $cleanup = "failed" }
            # Preserve the parser classification even when an early exit breaks the stdin task.
            $drain = [Diagnostics.Stopwatch]::StartNew()
            while ($drain.ElapsedMilliseconds -lt 1000) {
                try {
                    if ($null -ne $stdout) { Receive-StockRuntimeCapture $stdout }
                    if ($null -ne $stderr) { Receive-StockRuntimeCapture $stderr }
                }
                catch { break }
                if (($null -eq $stdout -or $stdout.Ended) -and ($null -eq $stderr -or $stderr.Ended)) { break }
                Start-Sleep -Milliseconds 10
            }
        }
        $exitCode = if ($started -and $process.HasExited) { [string]$process.ExitCode } else { "pending" }
        $childId = if ($started) { [string]$process.Id } else { "pending" }
        $stdoutBytes = if ($null -eq $stdout) { 0 } else { $stdout.Bytes }
        $stderrBytes = if ($null -eq $stderr) { 0 } else { $stderr.Bytes }
        $diagnostic = Get-StockRuntimeDiagnostic $stderr
        throw "Stock Monitor runtime failed. Invocation=$invocation RequestBytes=$($requestBytes.Length) ChildExitCode=$exitCode ChildPid=$childId StdoutBytes=$stdoutBytes StderrBytes=$stderrBytes Reason=$reason ExceptionType=$exceptionType Cleanup=$cleanup Stderr=[$diagnostic]"
    }
    finally {
        if ($started -and -not $process.HasExited) {
            try { Stop-StockRuntimeProcess $process } catch { Write-Warning "Stock Monitor child cleanup failed." }
        }
        foreach ($capture in @($stdout, $stderr)) {
            if ($null -eq $capture) { continue }
            try { $capture.Stream.Dispose() } catch { }
            try {
                if ($capture.Task.Wait(1000)) { $null = $capture.Task.GetAwaiter().GetResult() }
            }
            catch { }
            $capture.Data.Dispose()
        }
        if ($started) {
            # Disposing a broken stdin writer can throw; never mask the captured child error.
            try { $process.StandardInput.Dispose() } catch { }
        }
        if ($null -ne $stdinTask) {
            try { if ($stdinTask.Wait(1000)) { $null = $stdinTask.GetAwaiter().GetResult() } } catch { }
        }
        $process.Dispose()
    }
}
