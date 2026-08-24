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

    return Start-Process @parameters
}

function Stop-SpawnedProcess {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$TimeoutMilliseconds = 5000
    )

    if ($null -eq $Process) {
        return @()
    }

    $processId = $Process.Id
    $knownDescendantIds = [System.Collections.Generic.HashSet[int]]::new()
    $failures = New-Object System.Collections.ArrayList
    try {
        for ($wave = 0; $wave -lt 3; $wave++) {
            $enumerationFailed = $false
            try {
                $descendantProcessIds = @(Get-SmokeDescendantProcessIds -ProcessId $processId)
                foreach ($descendantProcessId in $descendantProcessIds) {
                    [void]$knownDescendantIds.Add($descendantProcessId)
                }
                for ($index = $descendantProcessIds.Count - 1; $index -ge 0; $index--) {
                    Stop-Process -Id $descendantProcessIds[$index] -Force -ErrorAction SilentlyContinue
                }
            } catch {
                [void]$failures.Add(
                    "Failed to enumerate descendants for process ${processId}: $($_.Exception.Message)"
                )
                $enumerationFailed = $true
            }

            if ($wave -eq 0) {
                if (-not $Process.HasExited) {
                    Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
                }
                if (-not $Process.WaitForExit($TimeoutMilliseconds)) {
                    [void]$failures.Add(
                        "Timed out waiting for process $processId to exit after Stop-Process."
                    )
                }
            }
            if ($enumerationFailed) {
                break
            }
            Start-Sleep -Milliseconds 100
        }

        if (-not $Process.HasExited) {
            [void]$failures.Add("Spawned process $processId was still running after cleanup.")
        }
        foreach ($descendantProcessId in $knownDescendantIds) {
            if ($null -ne (Get-Process -Id $descendantProcessId -ErrorAction SilentlyContinue)) {
                Stop-Process -Id $descendantProcessId -Force -ErrorAction SilentlyContinue
                Start-Sleep -Milliseconds 50
                if ($null -ne (Get-Process -Id $descendantProcessId -ErrorAction SilentlyContinue)) {
                    [void]$failures.Add(
                        "Descendant process $descendantProcessId was still running after cleanup."
                    )
                }
            }
        }
    } catch {
        [void]$failures.Add("Failed to stop spawned process ${processId}: $($_.Exception.Message)")
    } finally {
        $Process.Dispose()
    }

    $uniqueFailures = @($failures | ForEach-Object { [string]$_ } | Select-Object -Unique)
    foreach ($failure in $uniqueFailures) {
        Write-Warning $failure
    }
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

    $filter = "ParentProcessId=$ParentProcessId"
    try {
        return @(
            Get-CimInstance -ClassName Win32_Process -Filter $filter -ErrorAction Stop |
                ForEach-Object { [int]$_.ProcessId }
        )
    } catch {
        try {
            return @(
                Get-WmiObject -Class Win32_Process -Filter $filter -ErrorAction Stop |
                    ForEach-Object { [int]$_.ProcessId }
            )
        } catch {
            throw "Unable to query child processes for ${ParentProcessId}: $($_.Exception.Message)"
        }
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
