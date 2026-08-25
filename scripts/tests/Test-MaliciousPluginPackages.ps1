[CmdletBinding()]
param(
    [ValidateSet("archive", "signature", "dependency", "network", "process", "lifecycle")]
    [string]$Case = "archive"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))

function ConvertTo-GitHubCommandValue {
    param([string]$Value)
    return $Value.Replace("%", "%25").Replace("`r", "%0D").Replace("`n", "%0A")
}

function Stop-CargoTest {
    param([string]$Message)
    $title = ConvertTo-GitHubCommandValue -Value "Malicious plugin case $Case"
    $detail = ConvertTo-GitHubCommandValue -Value $Message
    Write-Output "::error title=${title}::$detail"
    throw $Message
}

function Invoke-CargoTest {
    param([string[]]$Arguments)
    Push-Location -LiteralPath $repoRoot
    try {
        $previousErrorActionPreference = $ErrorActionPreference
        try {
            # Windows PowerShell promotes redirected native stderr to error
            # records; discovery needs the exit code, not Cargo's status text.
            $ErrorActionPreference = "Continue"
            $listedTests = @(& cargo test --locked @Arguments -- --list 2>$null)
            $listExitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }
        if ($listExitCode -ne 0) {
            Stop-CargoTest "cargo test discovery failed: $($Arguments -join ' ')"
        }
        $testCount = @($listedTests | Where-Object { $_ -match ': test$' }).Count
        if ($testCount -lt 1) {
            Stop-CargoTest "No cargo tests matched: $($Arguments -join ' ')"
        }
        $testTail = [System.Collections.Generic.Queue[string]]::new()
        $previousErrorActionPreference = $ErrorActionPreference
        try {
            $ErrorActionPreference = "Continue"
            & cargo test --locked @Arguments 2>&1 | ForEach-Object {
                $line = $_.ToString()
                Write-Output $line
                if ($testTail.Count -ge 40) {
                    [void]$testTail.Dequeue()
                }
                $testTail.Enqueue($line)
            }
            $testExitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousErrorActionPreference
        }
        if ($testExitCode -ne 0) {
            $diagnostic = @($testTail.ToArray() | Select-Object -Last 20) -join " | "
            if ($diagnostic.Length -gt 2000) {
                $diagnostic = $diagnostic.Substring($diagnostic.Length - 2000)
            }
            Stop-CargoTest "cargo test failed: $($Arguments -join ' '). Tail: $diagnostic"
        }
    }
    finally {
        Pop-Location
    }
}

switch ($Case) {
    "archive" {
        Invoke-CargoTest @("-p", "loom_security", "archive::tests::rejects_case_collisions_and_windows_reserved_names")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::lifecycle::framework_package_rejects_unsafe_zip_paths")
    }
    "signature" {
        Invoke-CargoTest @("-p", "loom_plugin_security")
        Invoke-CargoTest @("-p", "loom-plugin-cli", "cli_sign_trust_pack_install_conformance_and_revoke_e2e")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::lifecycle::framework_rollback_rejects_tampered_or_revoked_previous_package")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::activation::art_integrity_and_rollback_reject_revoked_publisher_versions")
    }
    "dependency" {
        Invoke-CargoTest @("-p", "loom_tool_registry", "dependency::tests")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::install_core::rejects_remote_binary_without_sha256_before_downloading")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::recovery::framework_readiness_rejects_tampered_lockfile")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::activation::art_integrity_verification_rejects_package_and_lockfile_tampering")
    }
    "network" {
        Invoke-CargoTest @("-p", "loom_security", "network::tests")
    }
    "process" {
        Invoke-CargoTest @("-p", "loom_process")
        Invoke-CargoTest @("-p", "loom_mcp", "mcp::tests::stdio::stdio_client_times_out_and_terminates_hung_server")
        Invoke-CargoTest @("-p", "loom_sandbox")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::policy::permission_modes_audit_by_default_and_strictly_reject_unenforced_capabilities")
    }
    "lifecycle" {
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::recovery::framework_recovery")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::recovery::art_recovery")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::recovery::art_rollback_rejects_unsafe_previous_pointer")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::recovery::framework_uninstall_tombstone_recovery_restores_or_finishes_from_registry_state")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::recovery::art_uninstall_tombstone_recovery_restores_or_finishes_from_registry_state")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::recovery::framework_version_retention_keeps_active_previous_and_history_limit")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::recovery::art_version_retention_keeps_active_previous_and_writable_state")
    }
}

Write-Output "Malicious plugin package case '$Case' passed."
