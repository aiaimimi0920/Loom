function Complete-LoomPluginBoundaryCleanup {
    param(
        [AllowNull()][System.Management.Automation.ErrorRecord]$PrimaryFailure,
        [string]$EvidencePath,
        [scriptblock[]]$Actions
    )
    $firstCleanupFailure = $null
    $failureCount = 0
    foreach ($action in $Actions) {
        try { & $action | Out-Null } catch {
            $failureCount++
            if ($null -eq $firstCleanupFailure) { $firstCleanupFailure = $_ }
        }
    }
    if ($failureCount -eq 0) { return }
    $reportedFailure = if ($null -ne $PrimaryFailure) { $PrimaryFailure } else { $firstCleanupFailure }
    Save-LoomPluginBoundaryDiagnostic -EvidencePath $EvidencePath -Failure $reportedFailure `
        -CleanupFailure $firstCleanupFailure -CleanupFailureCount $failureCount
    if ($null -eq $PrimaryFailure) { throw $firstCleanupFailure }
    Write-Warning "Plugin boundary cleanup failed; the original smoke failure is retained." -WarningAction Continue
}
