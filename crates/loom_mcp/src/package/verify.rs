//! Active-state lookup and pre-spawn entry integrity verification.

use super::*;

/// Read the `active.json` written for an installed package.
pub fn read_active_state(
    control_plane_root: &Path,
    publisher_id: &str,
    package_id: &str,
) -> Result<McpPackageActiveState, McpPackageError> {
    if !is_safe_identity(publisher_id) || !is_safe_identity(package_id) {
        return Err(McpPackageError::InvalidManifest(format!(
            "`{publisher_id}/{package_id}` is not a safe package identity"
        )));
    }
    read_active_state_file(
        &control_plane_root
            .join("mcp")
            .join("packages")
            .join(publisher_id)
            .join(package_id)
            .join("active.json"),
    )
}

/// Re-verify a package-backed stdio server's entry file against the digest recorded at install.
///
/// For a stdio transport the manifest names an executable that becomes the server's command, and
/// the daemon spawns it with the user's credentials in its environment. Both `servers.json` and the
/// version directory it points at are ordinary files in the control plane, so the command deserved
/// no standing trust: the installer now records a digest per extracted file, `active.json` holds
/// the authoritative copy of that record, and this runs before every spawn. An entry script swapped
/// underneath an otherwise untouched package is refused instead of executed.
///
/// This is not a substitute for signing the package — a writer who can edit the version directory
/// can edit `active.json` beside it. It closes the case where only the executable is replaced, and
/// it is the check a publisher signature would hang off once packages carry one.
///
/// The command is also re-anchored inside the package directory here rather than trusted from
/// `servers.json`, which is the other half of the same problem: the registry row could keep an
/// installed package's publisher, version, and digest while pointing `command` at any file on the
/// machine, and the operator UI would still present it as that package.
pub fn verify_installed_entry(config: &McpServerConfig) -> Result<(), McpPackageError> {
    let Some(package) = &config.package else {
        return Ok(());
    };
    if config.transport != McpTransport::Stdio {
        return Ok(());
    }
    let command = Path::new(&config.command);
    let entry_key = command
        .strip_prefix(&package.package_dir)
        .ok()
        .and_then(package_key)
        .ok_or_else(|| {
            McpPackageError::Integrity(format!(
                "installed server `{}` runs `{}`, which is not inside its package directory `{}`",
                config.id,
                config.command,
                package.package_dir.display()
            ))
        })?;
    // The check above is lexical, so a link inside the package directory still satisfies it while
    // resolving somewhere else entirely. Compare the resolved paths as well.
    let resolved_package_dir = fs::canonicalize(&package.package_dir).map_err(|error| {
        McpPackageError::Integrity(format!(
            "installed server `{}` cannot resolve its package directory `{}`: {error}",
            config.id,
            package.package_dir.display()
        ))
    })?;
    let resolved_command = fs::canonicalize(command).map_err(|error| {
        McpPackageError::Integrity(format!(
            "installed server `{}` cannot resolve its entry `{entry_key}`: {error}",
            config.id
        ))
    })?;
    if !resolved_command.starts_with(&resolved_package_dir) {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` entry `{entry_key}` resolves to `{}`, outside its package directory `{}`",
            config.id,
            resolved_command.display(),
            resolved_package_dir.display()
        )));
    }
    // An extensionless command is resolved by the platform: Windows appends every `PATHEXT` entry
    // in turn, so `runtime/server` can start `runtime/server.exe` — a different file than the one
    // this hashes. Packages name their entry point exactly.
    #[cfg(windows)]
    if command.extension().is_none() {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` entry `{entry_key}` has no file extension, so Windows could \
             resolve it to a different file than the one that was installed",
            config.id
        )));
    }
    // `.bat` and `.cmd` are not run directly: `std::process::Command` hands them to `cmd.exe`, so the
    // only thing between manifest-supplied `args` and a shell command line is the standard library's
    // batch-argument escaping. A package names its own entry point, so it can ship an executable or a
    // `.ps1` script instead and not depend on that one mitigation. Unpackaged servers keep batch files,
    // because on Windows `npx` and `npm` *are* `.cmd` shims and refusing them would rule out most of
    // the MCP ecosystem — which is also why this check lives here rather than in the spawn path.
    if names_a_batch_file(command) {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` entry `{entry_key}` is a batch file, which Windows runs through \
             `cmd.exe`",
            config.id
        )));
    }
    let package_root = package
        .package_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            McpPackageError::Integrity(format!(
                "installed server `{}` has no package root above `{}`",
                config.id,
                package.package_dir.display()
            ))
        })?;
    let active = read_active_state_file(&package_root.join("active.json")).map_err(|error| {
        McpPackageError::Integrity(format!(
            "installed server `{}` has no readable package state: {error}",
            config.id
        ))
    })?;
    if active.qualified_id != package.qualified_id
        || active.version != package.version
        || !active.digest.eq_ignore_ascii_case(&package.digest)
        || active.package_dir != package.package_dir
    {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` does not match the recorded state of package `{}`",
            config.id, package.qualified_id
        )));
    }
    let expected = active.files.get(&entry_key).ok_or_else(|| {
        // A package installed before digests were recorded lands here. Refusing it is deliberate:
        // the alternative is spawning an unverifiable executable, and reinstalling the package
        // restores the record.
        McpPackageError::Integrity(format!(
            "installed server `{}` has no recorded digest for its entry `{entry_key}`; reinstall the package",
            config.id
        ))
    })?;
    if let Some(recorded) = package.files.get(&entry_key) {
        if !recorded.eq_ignore_ascii_case(expected) {
            return Err(McpPackageError::Integrity(format!(
                "installed server `{}` and package `{}` disagree on the digest of `{entry_key}`",
                config.id, package.qualified_id
            )));
        }
    }
    if !file_digest(command)?.eq_ignore_ascii_case(expected) {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` entry `{entry_key}` does not match its recorded digest",
            config.id
        )));
    }
    Ok(())
}
