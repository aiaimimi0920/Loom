<# Owns spawned-process tree discovery and bounded teardown. #>

function Stop-SpawnedProcess {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$TimeoutMilliseconds = 5000
    )

    if ($null -eq $Process) {
        return
    }

    try {
        $descendantProcessIds = @(Get-SmokeDescendantProcessIds -ProcessId $Process.Id)
        for ($index = $descendantProcessIds.Count - 1; $index -ge 0; $index--) {
            Stop-Process -Id $descendantProcessIds[$index] -Force -ErrorAction SilentlyContinue
        }

        if (-not $Process.HasExited) {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
        if (-not $Process.WaitForExit($TimeoutMilliseconds)) {
            Write-Warning "Timed out waiting for process $($Process.Id) to exit after Stop-Process."
        }

        foreach ($descendantProcessId in $descendantProcessIds) {
            $descendant = Get-Process -Id $descendantProcessId -ErrorAction SilentlyContinue
            if ($null -ne $descendant) {
                Write-Warning "Descendant process $descendantProcessId was still running after cleanup."
            }
        }
    } catch {
        Write-Warning "Failed to stop spawned process $($Process.Id): $($_.Exception.Message)"
    }
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
        return @(Get-CimInstance -ClassName Win32_Process -Filter $filter -ErrorAction Stop | ForEach-Object { [int]$_.ProcessId })
    } catch {
        return @(Get-WmiObject -Class Win32_Process -Filter $filter -ErrorAction SilentlyContinue | ForEach-Object { [int]$_.ProcessId })
    }
}
