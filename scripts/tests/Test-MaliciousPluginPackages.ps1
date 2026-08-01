[CmdletBinding()]
param(
    [ValidateSet("archive", "signature", "dependency", "network", "process", "lifecycle")]
    [string]$Case = "archive"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))

function Invoke-CargoTest {
    param([string[]]$Arguments)
    Push-Location -LiteralPath $repoRoot
    try {
        & cargo test --locked @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "cargo test failed for malicious plugin case '$Case': $($Arguments -join ' ')"
        }
    }
    finally {
        Pop-Location
    }
}

switch ($Case) {
    "archive" {
        Invoke-CargoTest @("-p", "loom_tool_registry", "secure_zip::tests::rejects_case_collisions_and_windows_reserved_names")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::framework_package_rejects_unsafe_zip_paths")
    }
    "signature" {
        Invoke-CargoTest @("-p", "loom_plugin_security")
        Invoke-CargoTest @("-p", "loom-plugin-cli", "cli_sign_trust_pack_install_conformance_and_revoke_e2e")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::framework_rollback_rejects_tampered_or_revoked_previous_package")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::art_integrity_and_rollback_reject_revoked_publisher_versions")
    }
    "dependency" {
        Invoke-CargoTest @("-p", "loom_tool_registry", "dependency::tests")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::rejects_remote_binary_without_sha256_before_downloading")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::framework_readiness_rejects_tampered_lockfile")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::art_integrity_verification_rejects_package_and_lockfile_tampering")
    }
    "network" {
        Invoke-CargoTest @("-p", "loom_tool_registry", "network_policy::tests")
    }
    "process" {
        Invoke-CargoTest @("-p", "loom_process")
        Invoke-CargoTest @("-p", "loom_mcp", "stdio_client_times_out_and_terminates_hung_server")
        Invoke-CargoTest @("-p", "loom_sandbox")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::permission_modes_audit_by_default_and_strictly_reject_unenforced_capabilities")
    }
    "lifecycle" {
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::framework_recovery")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::art_recovery")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::art_rollback_rejects_unsafe_previous_pointer")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::framework_uninstall_tombstone_recovery_restores_or_finishes_from_registry_state")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::art_uninstall_tombstone_recovery_restores_or_finishes_from_registry_state")
        Invoke-CargoTest @("-p", "loom_tool_registry", "framework::tests::framework_version_retention_keeps_active_previous_and_history_limit")
        Invoke-CargoTest @("-p", "loom_tool_registry", "install::tests::art_version_retention_keeps_active_previous_and_writable_state")
    }
}

Write-Output "Malicious plugin package case '$Case' passed."
