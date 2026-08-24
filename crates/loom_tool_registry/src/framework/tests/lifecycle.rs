//! Installation, upgrade, execution, and lifecycle tests.
use super::*;

#[test]
fn publisher_namespace_prevents_framework_id_takeover() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let id = "shared-framework";
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
            id,
            "0.1.0",
            Some("publisher.alpha"),
        ))
        .expect("install alpha");
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
            id,
            "0.1.0",
            Some("publisher.beta"),
        ))
        .expect("install beta");

    assert!(registry.is_installed("publisher.alpha/shared-framework"));
    assert!(registry.is_installed("publisher.beta/shared-framework"));
    assert!(!registry.is_installed(id), "bare id must be ambiguous");
    assert_ne!(
        registry.runtime_dir("publisher.alpha/shared-framework"),
        registry.runtime_dir("publisher.beta/shared-framework")
    );
    let error = registry
        .upgrade_framework_package(
            "publisher.beta/shared-framework",
            &fake_framework_package_zip_with_identity(id, "0.2.0", Some("publisher.alpha")),
        )
        .expect_err("publisher alpha must not upgrade publisher beta");
    assert!(matches!(error, FrameworkError::InvalidPackage { .. }));
    registry
        .uninstall("publisher.alpha/shared-framework")
        .expect("uninstall alpha");
    assert!(registry.is_installed("publisher.beta/shared-framework"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_package_install_uses_package_directory_and_replaces_version() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);

    let first = registry
        .install_with_runtime_fetcher("process", &|_id| {
            Ok(fake_framework_package_zip_with_version("process", "0.1.0"))
        })
        .expect("install first package");
    assert_eq!(first.version.as_deref(), Some("0.1.0"));
    assert!(registry
        .runtime_dir("process")
        .join(FRAMEWORK_MANIFEST_FILE)
        .is_file());

    let second = registry
        .install_with_runtime_fetcher("process", &|_id| {
            Ok(fake_framework_package_zip_with_version("process", "0.2.0"))
        })
        .expect("upgrade package");
    assert_eq!(second.version.as_deref(), Some("0.2.0"));
    assert!(second.ready);
    let rolled_back = registry.rollback("process").expect("rollback package");
    assert_eq!(rolled_back.version.as_deref(), Some("0.1.0"));
    assert!(rolled_back.ready);

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_rollback_rejects_tampered_or_revoked_previous_package() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let key = loom_plugin_security::generate_signing_key("release-key");
    registry
        .trust_publisher(PublisherTrustRecord {
            publisher_id: "publisher.rollback".to_owned(),
            key_id: key.key_id.clone(),
            public_key: key.public_key.clone(),
            revoked: false,
        })
        .expect("trust publisher");
    let reference = "publisher.rollback/signed-framework";
    registry
        .install_framework_package_from_zip(&signed_framework_package_zip(
            "signed-framework",
            "1.0.0",
            "publisher.rollback",
            &key,
        ))
        .expect("install v1");
    registry
        .install_framework_package_from_zip(&signed_framework_package_zip(
            "signed-framework",
            "2.0.0",
            "publisher.rollback",
            &key,
        ))
        .expect("install v2");
    registry
        .revoke_publisher("publisher.rollback", &key.key_id)
        .expect("revoke publisher");
    let (ready, detail) = registry.readiness(reference);
    assert!(!ready);
    assert!(detail.contains("信任策略"), "detail={detail}");
    assert!(registry.rollback(reference).is_err());

    let unsigned_root = temp_root();
    let unsigned = FrameworkRegistry::new(&unsigned_root);
    unsigned
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .expect("install unsigned v1");
    unsigned
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "2.0.0",
        ))
        .expect("install unsigned v2");
    let activation = unsigned.activation("process").expect("activation");
    let previous = unsigned_root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process")
        .join(activation.previous.expect("previous"));
    set_framework_tree_readonly(&previous, false).expect("unlock previous");
    std::fs::write(
        previous.join("runtime/loom-framework-process.exe"),
        b"tampered",
    )
    .expect("tamper previous runtime");
    assert!(unsigned.rollback("process").is_err());
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&unsigned_root).ok();
}

#[test]
fn reinstall_rejects_tampered_immutable_framework_version() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let package = fake_framework_package_zip_with_version("process", "1.0.0");
    registry
        .install_framework_package_from_zip(&package)
        .expect("install framework");
    let active = registry.runtime_dir("process");
    set_framework_tree_readonly(&active, false).expect("unlock active version");
    let executable = active.join("runtime/loom-framework-process.exe");
    std::fs::write(&executable, b"tampered").expect("tamper active runtime");

    let error = registry
        .install_framework_package_from_zip(&package)
        .expect_err("reinstall must not reuse tampered immutable content");

    assert!(matches!(error, FrameworkError::InvalidPackage { .. }));
    assert_eq!(
        std::fs::read(executable).expect("read runtime"),
        b"tampered"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn runtime_unpack_refuses_a_preexisting_staging_directory() {
    let root = temp_root();
    let staging = root.join("occupied-staging");
    std::fs::create_dir_all(&staging).expect("create occupied staging");
    let sentinel = staging.join("sentinel.txt");
    std::fs::write(&sentinel, b"keep").expect("write sentinel");

    let error = unpack_runtime_zip("process", &fake_framework_package_zip("process"), &staging)
        .expect_err("pre-existing staging path must be rejected");

    assert!(matches!(error, FrameworkError::RuntimeUnpackFailed { .. }));
    assert_eq!(std::fs::read(sentinel).expect("read sentinel"), b"keep");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn readiness_rejects_an_entry_retargeted_through_a_directory_link() {
    let root = temp_root();
    let outside = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip("process"))
        .expect("install framework");
    let active = registry.runtime_dir("process");
    set_framework_tree_readonly(&active, false).expect("unlock active version");
    let runtime = active.join("runtime");
    std::fs::remove_dir_all(&runtime).expect("remove packaged runtime");
    std::fs::write(outside.join("loom-framework-process.exe"), b"outside")
        .expect("write outside executable");
    create_directory_link(&outside, &runtime);

    let (ready, detail) = registry.readiness("process");

    assert!(!ready);
    assert!(detail.contains("不安全"), "detail={detail}");
    #[cfg(windows)]
    std::fs::remove_dir(&runtime).expect("remove junction");
    #[cfg(unix)]
    std::fs::remove_file(&runtime).expect("remove symlink");
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[test]
fn readiness_rejects_an_active_version_directory_link() {
    let root = temp_root();
    let outside = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip("process"))
        .expect("install framework");
    let active = registry.runtime_dir("process");
    let package_root = active
        .parent()
        .and_then(Path::parent)
        .expect("package root");
    set_framework_tree_readonly(package_root, false).expect("unlock package tree");
    let external_version = outside.join("external-version");
    std::fs::rename(&active, &external_version).expect("move active version outside package root");
    create_directory_link(&external_version, &active);

    let (ready, _) = registry.readiness("process");

    assert!(
        !ready,
        "an active directory link must never resolve as ready"
    );
    #[cfg(windows)]
    std::fs::remove_dir(&active).expect("remove junction");
    #[cfg(unix)]
    std::fs::remove_file(&active).expect("remove symlink");
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[test]
fn rollback_rejects_a_previous_version_directory_link() {
    let root = temp_root();
    let outside = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .expect("install v1");
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "2.0.0",
        ))
        .expect("install v2");
    let package_root = root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process");
    set_framework_tree_readonly(&package_root, false).expect("unlock package tree");
    let activation = registry.activation("process").expect("activation");
    let previous = package_root.join(activation.previous.expect("previous version"));
    let external_version = outside.join("external-version");
    std::fs::rename(&previous, &external_version).expect("move previous version outside package");
    create_directory_link(&external_version, &previous);

    let error = registry
        .rollback("process")
        .expect_err("rollback must reject a directory link");

    assert!(matches!(error, FrameworkError::NoRollback { .. }));
    #[cfg(windows)]
    std::fs::remove_dir(&previous).expect("remove junction");
    #[cfg(unix)]
    std::fs::remove_file(&previous).expect("remove symlink");
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[test]
fn framework_package_can_be_disabled_and_reenabled() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip("process"))
        .expect("install package");

    let disabled = registry.disable("process").expect("disable package");
    assert!(disabled.installed);
    assert!(!disabled.enabled);
    assert!(!disabled.ready);
    assert_eq!(disabled.ready_detail, "已禁用");

    let enabled = registry.enable("process").expect("enable package");
    assert!(enabled.installed);
    assert!(enabled.enabled);
    assert!(enabled.ready);

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_package_rejects_unsafe_zip_paths() {
    use std::io::Write;
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let mut zip_bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("../escape.txt", opts).unwrap();
        writer.write_all(b"escape").unwrap();
        writer.finish().unwrap();
    }
    let error = registry
        .install_framework_package_from_zip(&zip_bytes)
        .expect_err("unsafe package path must fail");
    assert!(matches!(error, FrameworkError::RuntimeUnpackFailed { .. }));
    assert!(!root.join("escape.txt").exists());
    std::fs::remove_dir_all(&root).ok();
}

// Build a complete package zip for process. The package manifest and
// process entry are required even when the package also carries Python.
fn fake_python_runtime_zip() -> Vec<u8> {
    fake_framework_package_zip("process")
}

#[test]
fn install_process_downloads_runtime_and_marks_installed() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    // process is NOT installed by default and requires its package.
    assert!(!registry.is_installed("process"));

    let status = registry
        .install_with_runtime_fetcher("process", &|_id| Ok(fake_python_runtime_zip()))
        .expect("install process with runtime");
    assert!(status.installed);
    assert!(status.ready, "package entry present => ready");
    assert!(registry.is_installed("process"));
    // The package landed in the active immutable version directory.
    assert!(registry
        .runtime_dir("process")
        .join("python-embed/python.exe")
        .is_file());

    // Uninstall reclaims the runtime dir.
    registry.uninstall("process").expect("uninstall");
    assert!(!registry.is_installed("process"));
    assert!(!root
        .join(FRAMEWORK_PACKAGES_DIR)
        .join("publisher.test")
        .join("process")
        .exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn process_readiness_reports_framework_package_detail() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let status = registry
        .install_with_runtime_fetcher("process", &|_id| Ok(fake_python_runtime_zip()))
        .expect("install process with runtime");
    let ready_detail = status.ready_detail.replace('\\', "/");
    assert!(status.ready, "status={status:?}");
    assert!(
        ready_detail.contains("process test framework"),
        "status={status:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn install_process_download_failure_leaves_it_uninstalled() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let result = registry.install_with_runtime_fetcher("process", &|id| {
        Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: "network down".to_owned(),
        })
    });
    assert!(result.is_err(), "download failure must error");
    assert!(
        !registry.is_installed("process"),
        "must not be marked installed on failure"
    );
    std::fs::remove_dir_all(&root).ok();
}
