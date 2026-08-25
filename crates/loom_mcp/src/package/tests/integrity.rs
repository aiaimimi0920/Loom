//! Entry containment, digest, and spawn-gate regressions.

use super::*;

#[test]
fn refuses_to_spawn_a_packaged_server_whose_command_points_outside_the_package() {
    // `servers.json` supplies the command, and a row that keeps the package block while
    // pointing `command` elsewhere was still presented in the UI as the installed package.
    let root = std::env::temp_dir().join(staging_name());
    let mut config =
        install_server_package(&root, &stdio_package_bytes()).expect("install package");
    let outside = root.join("outside.ps1");
    fs::write(&outside, b"Write-Output elsewhere").expect("write outside script");
    config.command = outside.display().to_string();

    let error =
        verify_installed_entry(&config).expect_err("a command outside the package must be refused");
    assert!(
        matches!(&error, McpPackageError::Integrity(message)
                if message.contains("not inside its package directory")),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn refuses_to_spawn_a_packaged_entry_without_a_file_extension() {
    // Windows resolves an extensionless command through `PATHEXT`, so `runtime/server` can
    // start `runtime/server.exe`: a file this never hashed.
    let root = std::env::temp_dir().join(staging_name());
    let config = install_server_package(
        &root,
        &package_bytes_with_entry(
            r#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server","args":[]}
            }"#,
            "runtime/server",
            b"Write-Output ready",
        ),
    )
    .expect("install package");

    let error = verify_installed_entry(&config)
        .expect_err("an extensionless packaged entry must be refused");
    assert!(
        matches!(&error, McpPackageError::Integrity(message)
                if message.contains("no file extension")),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn refuses_to_spawn_a_package_whose_entry_was_replaced() {
    // The command, its arguments, and its environment all come from `servers.json`, and the
    // version directory it points at is an ordinary directory in the control plane. Replacing
    // just the entry script left the manifest, the directory name, and the archive digest all
    // agreeing, so nothing noticed that the file about to run with the user's credentials was
    // not the file that was installed.
    let root = std::env::temp_dir().join(staging_name());
    let config = install_server_package(&root, &stdio_package_bytes()).expect("install package");

    fs::write(&config.command, b"Write-Output tampered").expect("replace entry script");

    let error = verify_installed_entry(&config).expect_err("a replaced entry must be refused");
    assert!(
        matches!(&error, McpPackageError::Integrity(message)
                if message.contains("runtime/server.ps1")),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn refuses_to_spawn_a_package_with_no_recorded_digests() {
    // The shape a package installed before digests were recorded has. Refusing it is
    // deliberate: the alternative is spawning an executable nothing can vouch for.
    let root = std::env::temp_dir().join(staging_name());
    let mut config =
        install_server_package(&root, &stdio_package_bytes()).expect("install package");
    let state = config.package.as_mut().expect("package state");
    state.files.clear();
    let package_root = state
        .package_dir
        .parent()
        .and_then(Path::parent)
        .expect("package root")
        .to_path_buf();
    let mut active =
        read_active_state_file(&package_root.join("active.json")).expect("read active state");
    active.files.clear();
    fs::write(
        package_root.join("active.json"),
        serde_json::to_vec_pretty(&active).expect("serialize active state"),
    )
    .expect("write legacy active state");

    let error = verify_installed_entry(&config).expect_err("an unverifiable entry must be refused");
    assert!(
        matches!(&error, McpPackageError::Integrity(message)
                if message.contains("reinstall the package")),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn refuses_a_package_whose_entry_is_a_batch_file() {
    // A batch entry is run by `cmd.exe`, and a package can just as easily ship a real executable
    // or a `.ps1`. Refused at install, where the publisher can still act on the message.
    let root = std::env::temp_dir().join(staging_name());
    let error = install_server_package(
        &root,
        &package_bytes_with_entry(
            r#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server.cmd","args":[]}
            }"#,
            "runtime/server.cmd",
            b"@echo ready",
        ),
    )
    .expect_err("a batch entry must be refused");
    assert!(
        matches!(&error, McpPackageError::InvalidManifest(message)
                if message.contains("batch file")),
        "unexpected error: {error}"
    );
    assert!(!root
        .join("mcp/packages/publisher.test/fixture-search")
        .exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn refuses_to_spawn_a_packaged_server_pointed_at_a_batch_file() {
    // `servers.json` supplies the command, so the install-time check is not the last word: the row
    // can be edited to a batch file that sits inside the package directory.
    let root = std::env::temp_dir().join(staging_name());
    let mut config =
        install_server_package(&root, &stdio_package_bytes()).expect("install package");
    let package_dir = config
        .package
        .as_ref()
        .expect("package state")
        .package_dir
        .clone();
    let batch = package_dir.join("runtime/server.cmd");
    fs::write(&batch, b"@echo ready").expect("write batch entry");
    config.command = batch.display().to_string();

    let error = verify_installed_entry(&config)
        .expect_err("a packaged batch entry must be refused at spawn");
    assert!(
        matches!(&error, McpPackageError::Integrity(message)
                if message.contains("batch file")),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}

// The installed fixture is a PowerShell package; cross-platform integrity is covered by
// `refuses_to_spawn_a_package_whose_entry_was_replaced` without launching that Windows entry.
#[cfg(windows)]
#[test]
fn refuses_to_spawn_a_package_backed_server_whose_entry_was_replaced() {
    // The gate belongs on the spawn path, not only in the checker: `StdioMcpClient` is what
    // turns a stored server row into a running process.
    let root = std::env::temp_dir().join(staging_name());
    let config = install_server_package(&root, &stdio_package_bytes()).expect("install package");
    let client =
        crate::StdioMcpClient::spawn_with_timeout(&config, std::time::Duration::from_secs(5))
            .expect("an untouched package spawns");
    drop(client);

    fs::write(&config.command, b"Write-Output tampered").expect("replace entry script");

    let error =
        match crate::StdioMcpClient::spawn_with_timeout(&config, std::time::Duration::from_secs(5))
        {
            Ok(_) => panic!("a replaced entry must not be spawned"),
            Err(error) => error,
        };
    assert!(
        matches!(&error, crate::McpError::PackageIntegrity(message)
                if message.contains("runtime/server.ps1")),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}
