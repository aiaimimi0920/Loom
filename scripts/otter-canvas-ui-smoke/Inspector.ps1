# Bounded WebView inspection and failure diagnostics for the canvas smoke.
# The dot-sourcing entry supplies Write-Utf8NoBom for captured stream evidence.

function Limit-SmokeText {
    param(
        [AllowNull()][object]$Text,
        [int]$MaxLength = 4000
    )

    $value = [string]$Text
    if ($value.Length -le $MaxLength) { return $value }
    return $value.Substring(0, $MaxLength) + "..."
}

function Read-BoundedTaskText {
    param(
        [System.Threading.Tasks.Task[string]]$Task,
        [string]$StreamName,
        [int]$TimeoutMilliseconds = 2000
    )

    if ($null -eq $Task) { return "" }
    try {
        if (-not $Task.Wait($TimeoutMilliseconds)) {
            return "[$StreamName drain timed out after $TimeoutMilliseconds milliseconds]"
        }
        return $Task.GetAwaiter().GetResult()
    }
    catch {
        return "[$StreamName drain failed: $($_.Exception.Message)]"
    }
}

function Get-OtterCanvasFailureDiagnostic {
    param(
        [AllowNull()][object]$DesktopPid,
        [int]$CdpPort,
        [string]$DesktopStderrPath
    )

    $sections = [System.Collections.Generic.List[string]]::new()
    try {
        $processes = @(Get-CimInstance Win32_Process -OperationTimeoutSec 2 -ErrorAction Stop)
        $relatedIds = @()
        if ($null -ne $DesktopPid) { $relatedIds += [int]$DesktopPid }
        $added = $true
        while ($added) {
            $added = $false
            foreach ($candidate in $processes) {
                if ($relatedIds -contains [int]$candidate.ProcessId) { continue }
                if ($relatedIds -contains [int]$candidate.ParentProcessId) {
                    $relatedIds += [int]$candidate.ProcessId
                    $added = $true
                }
            }
        }
        $processTree = @($processes | Where-Object { $relatedIds -contains [int]$_.ProcessId } | ForEach-Object {
            [ordered]@{
                processId = [int]$_.ProcessId
                parentProcessId = [int]$_.ParentProcessId
                name = [string]$_.Name
                commandLine = [string]$_.CommandLine
            }
        })
        $processTreeText = Limit-SmokeText -Text ($processTree | ConvertTo-Json -Depth 5 -Compress) -MaxLength 1800
        $sections.Add("Desktop process tree: " + $processTreeText)
    }
    catch {
        $sections.Add("Desktop process tree unavailable: $($_.Exception.Message)")
    }

    try {
        $listeners = @([System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() | Where-Object { [int]$_.Port -eq $CdpPort } | ForEach-Object {
            [ordered]@{
                localAddress = [string]$_.Address
                localPort = [int]$_.Port
            }
        })
        $listenerText = Limit-SmokeText -Text ($listeners | ConvertTo-Json -Depth 4 -Compress) -MaxLength 500
        $sections.Add("CDP listeners: " + $listenerText)
    }
    catch {
        $sections.Add("CDP listeners unavailable: $($_.Exception.Message)")
    }

    $desktopStderr = ""
    if (Test-Path -LiteralPath $DesktopStderrPath -PathType Leaf) {
        try { $desktopStderr = (Get-Content -Raw -Encoding UTF8 -LiteralPath $DesktopStderrPath).Trim() }
        catch { $desktopStderr = "could not read desktop stderr: $($_.Exception.Message)" }
    }
    $desktopStderrText = if ([string]::IsNullOrWhiteSpace($desktopStderr)) { "<empty>" } else { Limit-SmokeText -Text $desktopStderr -MaxLength 900 }
    $sections.Add("Desktop stderr: " + $desktopStderrText)
    return Limit-SmokeText -Text ($sections -join [Environment]::NewLine) -MaxLength 3500
}

function Invoke-Inspector {
    param(
        [string]$InspectorPath,
        [int]$DebugPort,
        [string]$OutputPath,
        [string]$ScreenshotPath,
        [int]$MinimumNodes = 1,
        [int]$TimeoutSeconds = 20
    )

    $stdoutPath = "$OutputPath.inspector.stdout.log"
    $stderrPath = "$OutputPath.inspector.stderr.log"
    Remove-Item -LiteralPath $OutputPath, $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    $quotedInspectorPath = '"' + $InspectorPath.Replace('"', '\"') + '"'
    $quotedOutputPath = '"' + $OutputPath.Replace('"', '\"') + '"'
    $quotedScreenshotPath = '"' + $ScreenshotPath.Replace('"', '\"') + '"'
    $process = $null
    $stdoutTask = $null
    $stderrTask = $null
    $timedOut = $false
    $terminationError = $null
    $terminated = $true
    try {
        $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = (Get-Command node).Source
        $startInfo.WorkingDirectory = Split-Path -Parent ([IO.Path]::GetFullPath($OutputPath))
        $startInfo.Arguments = @(
            $quotedInspectorPath,
            "--debug-port", [string]$DebugPort,
            "--output", $quotedOutputPath,
            "--screenshot", $quotedScreenshotPath,
            "--min-nodes", [string]$MinimumNodes
        ) -join " "
        $startInfo.UseShellExecute = $false
        $startInfo.CreateNoWindow = $true
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        $process = [System.Diagnostics.Process]::new()
        $process.StartInfo = $startInfo
        if (-not $process.Start()) { throw "Could not start the WebView inspector process." }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            $timedOut = $true
            try { $process.Kill() }
            catch { $terminationError = "Initial inspector termination failed: $($_.Exception.Message)" }
            try { $terminated = $process.WaitForExit(2000) }
            catch { $terminated = $false }
            if (-not $terminated) {
                try { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
                catch { $terminationError = (($terminationError, "Forced inspector termination failed: $($_.Exception.Message)") -join " ").Trim() }
                try { $terminated = $process.WaitForExit(2000) }
                catch { $terminated = $false }
            }
            if (-not $terminated -and [string]::IsNullOrWhiteSpace($terminationError)) {
                $terminationError = "Inspector process did not exit after bounded termination attempts."
            }
        }
        $stdout = Read-BoundedTaskText -Task $stdoutTask -StreamName "stdout"
        $stderr = Read-BoundedTaskText -Task $stderrTask -StreamName "stderr"
        $exitCode = $null
        if ($process.HasExited) { $exitCode = $process.ExitCode }
        Write-Utf8NoBom -Path $stdoutPath -Content $stdout
        Write-Utf8NoBom -Path $stderrPath -Content $stderr
        if ($timedOut) {
            $terminationSuffix = if ([string]::IsNullOrWhiteSpace($terminationError)) { "" } else { " $terminationError" }
            throw "WebView inspector timed out after $TimeoutSeconds seconds.$terminationSuffix $(Limit-SmokeText -Text $stderr)"
        }
        if ($null -eq $exitCode) {
            throw "WebView inspector exited without a concrete exit code."
        }
        if ($exitCode -ne 0) {
            $output = Limit-SmokeText -Text (($stdout, $stderr | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join [Environment]::NewLine)
            throw "WebView inspector failed with exit code $exitCode`: $output"
        }
    }
    finally {
        if ($null -ne $process) { $process.Dispose() }
    }
    return Get-Content -Raw -Encoding UTF8 -LiteralPath $OutputPath | ConvertFrom-Json
}

function Wait-ForOtterCanvasUi {
    param(
        [string]$InspectorPath,
        [int]$DebugPort,
        [string]$OutputPath,
        [string]$ScreenshotPath,
        [int]$MinimumNodes,
        [AllowNull()][string]$PreviousRevision,
        [int]$TimeoutSeconds = 90
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastError = $null
    while ((Get-Date) -lt $deadline) {
        try {
            $remainingSeconds = [Math]::Max(1, [Math]::Ceiling(($deadline - (Get-Date)).TotalSeconds))
            $inspectorTimeoutSeconds = [int][Math]::Min(20, $remainingSeconds)
            $snapshot = Invoke-Inspector `
                -InspectorPath $InspectorPath `
                -DebugPort $DebugPort `
                -OutputPath $OutputPath `
                -ScreenshotPath $ScreenshotPath `
                -MinimumNodes $MinimumNodes `
                -TimeoutSeconds $inspectorTimeoutSeconds
            $revisionChanged = [string]::IsNullOrWhiteSpace($PreviousRevision) -or `
                ([string]$snapshot.revision -ne $PreviousRevision)
            if (@($snapshot.thumbnailNodeCount).Count -gt 0 -and `
                [int]$snapshot.thumbnailNodeCount -ge $MinimumNodes -and `
                $revisionChanged) {
                return $snapshot
            }
            $lastError = "Observed nodes=$($snapshot.thumbnailNodeCount), revision=$($snapshot.revision)."
        }
        catch {
            $lastError = Limit-SmokeText -Text $_.Exception.Message
        }
        Start-Sleep -Milliseconds 250
    }
    $diagnostic = "unavailable"
    if (Test-Path -LiteralPath $OutputPath -PathType Leaf) {
        try {
            $diagnostic = Limit-SmokeText -Text ((Get-Content -Raw -Encoding UTF8 -LiteralPath $OutputPath).Trim())
        }
        catch {
            $diagnostic = "could not read ${OutputPath}: $($_.Exception.Message)"
        }
    }
    throw "Timed out waiting for Otter canvas UI nodes=$MinimumNodes with a new revision. $lastError Inspector diagnostic: $diagnostic"
}
