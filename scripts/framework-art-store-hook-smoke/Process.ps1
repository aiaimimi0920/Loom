# Process argument quoting, launch and temporary environment inheritance.
function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)

    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) {
        return $Argument
    }

    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $backslashCount = 0
    foreach ($character in $Argument.ToCharArray()) {
        if ($character -eq [char]0x5c) {
            $backslashCount += 1
            continue
        }

        if ($character -eq [char]0x22) {
            if ($backslashCount -gt 0) {
                [void]$builder.Append("\" * (($backslashCount * 2) + 1))
            } else {
                [void]$builder.Append("\")
            }
            [void]$builder.Append('"')
            $backslashCount = 0
            continue
        }

        if ($backslashCount -gt 0) {
            [void]$builder.Append("\" * $backslashCount)
            $backslashCount = 0
        }
        [void]$builder.Append($character)
    }

    if ($backslashCount -gt 0) {
        [void]$builder.Append("\" * ($backslashCount * 2))
    }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Start-SmokeProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory = "",
        [string]$StdoutPath = "",
        [string]$StderrPath = ""
    )

    $FilePath = Resolve-SmokeRealFile -Path $FilePath -Label "spawned process binary"
    if (-not [string]::IsNullOrWhiteSpace($WorkingDirectory)) {
        $WorkingDirectory = Resolve-SmokeRealDirectory -Path $WorkingDirectory -Label "spawned process working directory"
    }
    foreach ($redirectPath in @($StdoutPath, $StderrPath)) {
        if ([string]::IsNullOrWhiteSpace($redirectPath)) {
            continue
        }
        $redirectFullPath = [System.IO.Path]::GetFullPath($redirectPath)
        $redirectParent = Split-Path -Parent $redirectFullPath
        [void](Resolve-SmokeRealDirectory -Path $redirectParent -Label "process log parent")
        if (Test-Path -LiteralPath $redirectFullPath) {
            [void](Resolve-SmokeRealFile -Path $redirectFullPath -Label "process log")
        }
    }

    $argumentLine = (@($ArgumentList) | ForEach-Object { ConvertTo-ProcessArgument -Argument $_ }) -join " "
    $parameters = @{
        FilePath = $FilePath
        PassThru = $true
        WindowStyle = "Hidden"
    }
    if (-not [string]::IsNullOrWhiteSpace($argumentLine)) {
        $parameters.ArgumentList = $argumentLine
    }
    if (-not [string]::IsNullOrWhiteSpace($WorkingDirectory)) {
        $parameters.WorkingDirectory = $WorkingDirectory
    }
    if (-not [string]::IsNullOrWhiteSpace($StdoutPath)) {
        $parameters.RedirectStandardOutput = $StdoutPath
    }
    if (-not [string]::IsNullOrWhiteSpace($StderrPath)) {
        $parameters.RedirectStandardError = $StderrPath
    }

    $process = Start-Process @parameters
    try {
        # Windows PowerShell can return a PID-only wrapper when streams are redirected.
        $handle = $process.Handle
        if ($null -eq $handle -or $handle -eq [IntPtr]::Zero) { throw "No retained handle for spawned process." }
        return $process
    } catch {
        if ($null -ne $process) { $process.Dispose() }
        throw
    }
}

# Retain a real kernel handle before trusting a PID discovered in a process snapshot.
function Get-SmokeOwnedChildProcess {
    param([int]$ProcessId, [object]$Parent)

    $child = $null
    $retained = $false
    try {
        try { $child = Get-Process -Id $ProcessId -ErrorAction Stop }
        catch {
            if ($_.FullyQualifiedErrorId -like 'NoProcessFoundForGivenId*') { return $null }
            throw
        }
        try {
            $handle = $child.Handle
            if ($null -eq $handle -or $handle -eq [IntPtr]::Zero) { throw "No retained handle for descendant $ProcessId." }
        } catch {
            $acquisitionError = $_
            try { if ($child.WaitForExit(0)) { return $null } } catch { }
            throw $acquisitionError
        }
        # Query/access failures remain failures even if the process exits concurrently.
        $records = @(Get-SmokeProcessRecords -Filter "ProcessId=$ProcessId")
        if ($records.Count -eq 0 -and $child.WaitForExit(0)) { return $null }
        if ($records.Count -ne 1 -or [int]$records[0].ParentProcessId -ne $Parent.Id -or
            $child.StartTime.ToUniversalTime() -lt $Parent.StartTime.ToUniversalTime()) {
            throw "Cannot verify descendant $ProcessId belongs to process $($Parent.Id)."
        }
        $retained = $true
        return $child
    } finally {
        if ($null -ne $child -and -not $retained) { $child.Dispose() }
    }
}

function Wait-SmokeOwnedProcessExit {
    param([object]$Process, [object]$Clock, [int]$TimeoutMilliseconds)

    $remaining = [int][Math]::Max(0L, [long]$TimeoutMilliseconds - [long]$Clock.ElapsedMilliseconds)
    # HasExited can precede the final handle signal on Framework; always wait on that signal.
    return $Process.WaitForExit($remaining)
}

function Stop-SpawnedProcess {
    param(
        [System.Diagnostics.Process]$Process,
        [ValidateRange(1, 2147483647)][int]$TimeoutMilliseconds = 5000
    )

    if ($null -eq $Process) { return @() }
    $owned = New-Object System.Collections.ArrayList
    [void]$owned.Add($Process)
    $knownIds = [System.Collections.Generic.HashSet[int]]::new()
    [void]$knownIds.Add($Process.Id)
    $terminationRequested = [System.Collections.Generic.HashSet[int]]::new()
    $failures = New-Object System.Collections.ArrayList
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $rootHandle = $Process.Handle
        if ($null -eq $rootHandle -or $rootHandle -eq [IntPtr]::Zero) { throw "No retained root process handle." }
        $null = $Process.StartTime
        for ($wave = 0; $wave -lt 3; $wave++) {
            $discoveryExpired = $false
            # Revisit retained parents even after they exit; their children may outlive them.
            for ($parentIndex = 0; $parentIndex -lt $owned.Count; $parentIndex++) {
                if ($clock.ElapsedMilliseconds -ge $TimeoutMilliseconds) {
                    $discoveryExpired = $true
                    break
                }
                $parent = $owned[$parentIndex]
                try {
                    foreach ($childId in @(Get-SmokeChildProcessIds -ParentProcessId $parent.Id)) {
                        if ($clock.ElapsedMilliseconds -ge $TimeoutMilliseconds) { $discoveryExpired = $true; break }
                        if ($knownIds.Contains($childId)) { continue }
                        try {
                            $child = Get-SmokeOwnedChildProcess -ProcessId $childId -Parent $parent
                            if ($null -ne $child) {
                                [void]$knownIds.Add($childId)
                                [void]$owned.Add($child)
                            }
                        } catch {
                            [void]$failures.Add("Failed to retain descendant ${childId}: $($_.Exception.Message)")
                        }
                    }
                } catch {
                    [void]$failures.Add("Failed to enumerate descendants for process $($parent.Id): $($_.Exception.Message)")
                }
            }
            # Request every known termination before spending the single exit-wait budget.
            for ($index = $owned.Count - 1; $index -ge 0; $index--) {
                $member = $owned[$index]
                if (-not $terminationRequested.Add($member.Id)) { continue }
                try { if (-not $member.HasExited) { $member.Kill() } }
                catch {
                    $stopError = $_
                    $exited = $false
                    try { $exited = $member.WaitForExit(0) } catch { }
                    if (-not $exited) {
                        [void]$failures.Add("Failed to stop owned process $($member.Id): $($stopError.Exception.Message)")
                    }
                }
            }
            if ($discoveryExpired) {
                [void]$failures.Add("Process-tree discovery exceeded the cleanup budget.")
                break
            }
            if ($wave -lt 2) {
                $pause = [int][Math]::Max(0L, [Math]::Min(100L, $TimeoutMilliseconds - $clock.ElapsedMilliseconds))
                if ($pause -gt 0) { Start-Sleep -Milliseconds $pause }
            }
        }
        foreach ($member in $owned) {
            try {
                if (-not (Wait-SmokeOwnedProcessExit -Process $member -Clock $clock -TimeoutMilliseconds $TimeoutMilliseconds)) {
                    [void]$failures.Add("Timed out waiting for owned process $($member.Id) to exit after termination.")
                }
            } catch {
                [void]$failures.Add("Failed to wait for owned process $($member.Id): $($_.Exception.Message)")
            }
        }
    } catch {
        [void]$failures.Add("Failed to stop spawned process $($Process.Id): $($_.Exception.Message)")
    } finally {
        foreach ($member in $owned) {
            try { $member.Dispose() }
            catch { [void]$failures.Add("Failed to dispose an owned process handle.") }
        }
        $clock.Stop()
    }
    # CIM/WMI discovery is synchronous; this deadline bounds waits, not a stalled provider call.
    $uniqueFailures = @($failures | ForEach-Object { [string]$_ } | Select-Object -Unique)
    foreach ($failure in $uniqueFailures) { Write-Warning $failure }
    return $uniqueFailures
}

function Get-SmokeDescendantProcessIds {
    param([int]$ProcessId)

    $pending = New-Object System.Collections.ArrayList
    $descendants = New-Object System.Collections.ArrayList
    $seen = @{ $ProcessId = $true }
    [void]$pending.Add($ProcessId)
    $pendingIndex = 0

    while ($pendingIndex -lt $pending.Count) {
        $parentProcessId = [int]$pending[$pendingIndex]
        $pendingIndex += 1

        foreach ($childProcessId in @(Get-SmokeChildProcessIds -ParentProcessId $parentProcessId)) {
            if (-not $seen.ContainsKey($childProcessId)) {
                $seen[$childProcessId] = $true
                [void]$descendants.Add($childProcessId)
                [void]$pending.Add($childProcessId)
            }
        }
    }

    return @($descendants | ForEach-Object { [int]$_ })
}

function Get-SmokeChildProcessIds {
    param([int]$ParentProcessId)

    return @(Get-SmokeProcessRecords -Filter "ParentProcessId=$ParentProcessId" | ForEach-Object { [int]$_.ProcessId })
}

function Get-SmokeProcessRecords {
    param([string]$Filter)

    try {
        return @(Get-CimInstance -ClassName Win32_Process -Filter $Filter -ErrorAction Stop)
    } catch {
        try { return @(Get-WmiObject -Class Win32_Process -Filter $Filter -ErrorAction Stop) }
        catch { throw "Unable to query the smoke process tree: $($_.Exception.Message)" }
    }
}

function Start-InheritedEnvProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory = "",
        [hashtable]$Environment = @{},
        [string]$StdoutPath = "",
        [string]$StderrPath = ""
    )

    $previous = @{}
    foreach ($entry in $Environment.GetEnumerator()) {
        $previous[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key, "Process")
        [Environment]::SetEnvironmentVariable($entry.Key, [string]$entry.Value, "Process")
    }

    try {
        return Start-SmokeProcess `
            -FilePath $FilePath `
            -ArgumentList $ArgumentList `
            -WorkingDirectory $WorkingDirectory `
            -StdoutPath $StdoutPath `
            -StderrPath $StderrPath
    } finally {
        foreach ($entry in $previous.GetEnumerator()) {
            [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, "Process")
        }
    }
}
