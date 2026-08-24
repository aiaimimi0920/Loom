//! Installation, persistence, versioning, and reinstall regressions.

use super::*;

#[test]
fn installs_independent_mcp_server_package() {
    let root = std::env::temp_dir().join(staging_name());
    let bytes = package_bytes(
        r#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server.ps1","args":[]},
                "tools":["search"],
                "credentials":[{"id":"api_key","label":"API Key","required":true,"target":{"kind":"env","name":"API_KEY"}}]
            }"#,
        b"Write-Output ready",
    );

    let config = install_server_package(&root, &bytes).expect("install package");

    assert_eq!(config.id, "fixture-search");
    assert_eq!(config.tools, vec!["search"]);
    assert_eq!(config.credential_env["API_KEY"], "api_key");
    assert_eq!(
        config.package.as_ref().expect("package state").qualified_id,
        "publisher.test/fixture-search"
    );
    assert!(Path::new(&config.command).is_file());

    // Every extracted file is hashed at install, and the persisted state carries the same
    // record the returned config does.
    let state = config.package.as_ref().expect("package state");
    assert_eq!(
        state.files.keys().collect::<Vec<_>>(),
        vec![MCP_SERVER_PACKAGE_MANIFEST, "runtime/server.ps1"]
    );
    assert_eq!(
        state.files["runtime/server.ps1"],
        format!("{:x}", Sha256::digest(b"Write-Output ready"))
    );
    let active =
        read_active_state(&root, "publisher.test", "fixture-search").expect("read active state");
    assert_eq!(active.files, state.files);
    assert_eq!(active.digest, state.digest);
    assert_eq!(active.trust_status, PackageTrustStatus::Unsigned);
    verify_installed_entry(&config).expect("entry matches its recorded digest");

    uninstall_server_package(&root, &config).expect("uninstall package");
    assert!(!root
        .join("mcp/packages/publisher.test/fixture-search")
        .exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn writing_active_state_leaves_no_temporary_file_behind() {
    // The temporary file is named per install now, so nothing sweeps a stale one up by reusing the
    // same name: the rename has to be what removes it, or every install leaves one behind.
    let root = std::env::temp_dir().join(staging_name());
    let bytes = stdio_package_bytes();
    let config = install_server_package(&root, &bytes).expect("install package");
    install_server_package(&root, &bytes).expect("reinstall package");
    let package_root = config
        .package
        .as_ref()
        .expect("package state")
        .package_dir
        .parent()
        .and_then(Path::parent)
        .expect("package root")
        .to_path_buf();

    let leftovers: Vec<String> = fs::read_dir(&package_root)
        .expect("read package root")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "unexpected temporary files: {leftovers:?}"
    );
    // What survived is a whole record rather than a file the rename published half-written.
    let active =
        read_active_state_file(&package_root.join("active.json")).expect("read active state");
    assert_eq!(active.digest.len(), 64);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn installs_a_package_that_vendors_its_dependencies() {
    // The entry cap was 128, which no npm or Python server fits once its dependency tree is in the
    // archive. The limits that guard extraction live in the shared extractor, not in this number.
    let root = std::env::temp_dir().join(staging_name());
    let vendored: Vec<String> = (0..300)
        .map(|index| format!("runtime/node_modules/dep-{index}/index.js"))
        .collect();
    let mut files: Vec<(&str, &[u8])> = vec![
        (
            MCP_SERVER_PACKAGE_MANIFEST,
            br#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server.ps1","args":[]}
            }"#,
        ),
        ("runtime/server.ps1", b"Write-Output ready"),
    ];
    files.extend(
        vendored
            .iter()
            .map(|name| (name.as_str(), b"module.exports = {};" as &[u8])),
    );

    let config = install_server_package(&root, &package_bytes_with_files(&files))
        .expect("install a package that vendors its dependencies");

    assert_eq!(
        config.package.as_ref().expect("package state").files.len(),
        files.len()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_version_directory_is_named_after_enough_of_the_digest_to_be_unique() {
    // Two archives sharing this directory means one of them runs the other's files, so the part
    // of the digest in the name has to be wide enough that no attacker can arrange the collision.
    let root = std::env::temp_dir().join(staging_name());
    let config = install_server_package(&root, &stdio_package_bytes()).expect("install package");
    let state = config.package.as_ref().expect("package state");

    assert!(PACKAGE_DIRECTORY_DIGEST_CHARS >= 32);
    assert_eq!(
        state.package_dir.file_name().and_then(|name| name.to_str()),
        Some(
            format!(
                "{}-{}",
                state.version,
                &state.digest[..PACKAGE_DIRECTORY_DIGEST_CHARS]
            )
            .as_str()
        )
    );
    // Only the directory name is shortened; what is recorded stays the whole digest.
    assert_eq!(state.digest.len(), 64);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn refuses_to_reinstall_over_a_tampered_version_directory() {
    // A reinstall used to see the version directory already there and throw the freshly
    // extracted copy away, which made the one repair a user can perform by hand a no-op.
    let root = std::env::temp_dir().join(staging_name());
    let bytes = stdio_package_bytes();
    let config = install_server_package(&root, &bytes).expect("install package");
    fs::write(&config.command, b"Write-Output tampered").expect("replace entry script");

    let error = install_server_package(&root, &bytes)
        .expect_err("a reinstall must not adopt a tampered tree");
    assert!(
        matches!(&error, McpPackageError::Integrity(message)
                if message.contains("runtime/server.ps1")),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}
