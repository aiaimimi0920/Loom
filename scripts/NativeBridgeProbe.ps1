# Test-only native TLS adapter. The explicit manifest belongs to this smoke run.
function Stop-LoomNativeBridgeProbe {
    param([AllowNull()][object]$Client)
    if ($null -eq $Client -or $Client.State -eq 'Closed') { return }
    $Client.State = 'Closing'
    $process = $Client.Process
    $terminated = $false
    try {
        if (-not $Client.Started) { $terminated = $true; return }
        try { $process.StandardInput.Close() } catch {}
        if (-not $process.WaitForExit(1000)) {
            $process.Kill()
            if (-not $process.WaitForExit(3000)) { throw 'Native bridge probe did not exit.' }
        }
        $terminated = $true
    } finally {
        # Retain the owned handle after a kill failure so outer cleanup can retry.
        if ($terminated) {
            if ($null -ne $Client.Writer) { try { $Client.Writer.Dispose() } catch {} }
            $process.Dispose()
            $Client.State = 'Closed'
        }
    }
}

function Read-LoomNativeBridgeEnvelope {
    param([object]$Client, [ValidateRange(1, 151000)][int]$BudgetMs)
    $task = $Client.Process.StandardOutput.ReadLineAsync()
    if (-not $task.Wait($BudgetMs)) { throw 'Native bridge probe response timed out.' }
    $line = $task.GetAwaiter().GetResult()
    if ($null -eq $line -or $line.Length -gt 8MB) { throw 'Invalid native bridge probe response.' }
    $envelope = $line | ConvertFrom-Json
    if ($null -eq $envelope.PSObject.Properties['ok'] -or $envelope.ok -ne $true) {
        throw 'Native bridge probe operation failed.'
    }
    return $envelope
}

function Invoke-LoomNativeBridgeProbe {
    param([object]$Client, [hashtable]$Command, [ValidateRange(1, 151000)][int]$BudgetMs)
    if ($Client.State -ne 'Open') { throw 'Native bridge probe is closed.' }
    $watch = [Diagnostics.Stopwatch]::StartNew()
    try {
        $json = $Command | ConvertTo-Json -Depth 40 -Compress
        if ([Text.Encoding]::UTF8.GetByteCount($json) -gt 8MB) { throw 'Probe command exceeds its limit.' }
        $task = $Client.Writer.WriteLineAsync($json)
        if (-not $task.Wait($BudgetMs)) { throw 'Native bridge probe write timed out.' }
        [void]$task.GetAwaiter().GetResult()
        $remaining = $BudgetMs - [int]$watch.ElapsedMilliseconds
        if ($remaining -lt 1) { throw 'Native bridge probe operation timed out.' }
        return Read-LoomNativeBridgeEnvelope -Client $Client -BudgetMs $remaining
    } catch {
        Stop-LoomNativeBridgeProbe -Client $Client
        throw
    }
}

function Start-LoomNativeBridgeProbe {
    param([string]$Executable, [string]$ManifestPath, [ValidateRange(1, 10000)][int]$BudgetMs = 10000)
    foreach ($path in @($Executable, $ManifestPath)) {
        if (-not [IO.Path]::IsPathRooted($path) -or $path.Contains('"')) { throw 'Probe paths must be explicit absolute paths.' }
        $item = Get-Item -LiteralPath $path -Force
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'Probe paths must be regular files.'
        }
    }
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
    $info.Arguments = '"' + $ManifestPath + '"'
    $info.WorkingDirectory = [IO.Path]::GetDirectoryName($Executable)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.StandardOutputEncoding = [Text.UTF8Encoding]::new($false, $true)
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $client = [pscustomobject]@{ Process = $process; State = 'Open'; Started = $false; Writer = $null }
    try {
        # .NET Framework lacks StandardInputEncoding. Process.Start creates and
        # auto-flushes its stdin writer, so set no-BOM encoding before that point.
        $previousInputEncoding = [Console]::InputEncoding
        try {
            [Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
            if (-not $process.Start()) { throw 'Native bridge probe did not start.' }
            $client.Started = $true
        } finally { [Console]::InputEncoding = $previousInputEncoding }
        $client.Writer = $process.StandardInput
        [void](Read-LoomNativeBridgeEnvelope -Client $client -BudgetMs $BudgetMs)
        return $client
    } catch {
        Stop-LoomNativeBridgeProbe -Client $client
        throw
    }
}
