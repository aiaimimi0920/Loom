//! Full install pins survive lifecycle changes and fail closed for legacy records.
use super::*;

const ID: &str = "publisher.test/process";

fn admit(registry: &FrameworkRegistry) -> Result<FrameworkExecutionAdmission, String> {
    let package = registry.runtime_dir(ID);
    let text = fs::read_to_string(package.join(FRAMEWORK_MANIFEST_FILE)).unwrap();
    FrameworkExecutionAdmission::capture(&registry.root.join("frameworks"), &package, &text)
}

fn cleanup(root: &Path) {
    set_framework_tree_readonly(root, false).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn admission_rejects_replaced_directory_activation_and_digest_lock() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .unwrap();
    let original_state = fs::read(root.join(FRAMEWORKS_FILE)).unwrap();
    let package = registry.runtime_dir(ID);
    let manifest = registry.package_manifest(ID).unwrap();
    let original_digest = canonical_package_digest(&package, None).unwrap();
    set_framework_tree_readonly(&package, false).unwrap();
    fs::write(
        package.join(&manifest.entry.command),
        b"replacement-executable",
    )
    .unwrap();
    let digest = canonical_package_digest(&package, None).unwrap();
    let package_root = package.parent().unwrap().parent().unwrap();
    let relative = format!("versions/1.0.0-{}", &digest[..12]);
    fs::copy(
        package_root
            .join("locks")
            .join(format!("{original_digest}.json")),
        package_root.join("locks").join(format!("{digest}.json")),
    )
    .unwrap();
    fs::rename(&package, package_root.join(&relative)).unwrap();
    fs::write(
        package_root.join(FRAMEWORK_ACTIVE_FILE),
        serde_json::to_vec(&serde_json::json!({"active": relative})).unwrap(),
    )
    .unwrap();
    let error = admit(&registry)
        .err()
        .expect("the registry pin cannot follow a substituted activation");
    assert!(error.contains("pinned installation digest"), "{error}");
    assert_eq!(
        fs::read(root.join(FRAMEWORKS_FILE)).unwrap(),
        original_state
    );
    cleanup(&root);
}

#[test]
fn legacy_install_digest_requires_explicit_reinstall_without_losing_state() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    let package = fake_framework_package_zip_with_version("process", "1.0.0");
    registry
        .install_framework_package_from_zip(&package)
        .unwrap();
    let mut states = registry.installation_states().unwrap();
    states.get_mut(ID).unwrap().package_digest = None;
    registry.write_installed(&states).unwrap();
    let legacy = fs::read(root.join(FRAMEWORKS_FILE)).unwrap();
    assert!(registry.is_installed(ID));
    assert!(admit(&registry).err().unwrap().contains("reinstall"));
    assert_eq!(fs::read(root.join(FRAMEWORKS_FILE)).unwrap(), legacy);
    registry.disable(ID).unwrap();
    registry.enable(ID).unwrap();
    assert!(
        admit(&registry).is_err(),
        "enable is not install authorization"
    );
    registry
        .install_framework_package_from_zip(&package)
        .unwrap();
    admit(&registry).unwrap().revalidate().unwrap();
    // A repeated reinstall is idempotent and retains the same verified pin.
    let pin = registry.installation_states().unwrap()[ID]
        .package_digest
        .clone();
    registry
        .install_framework_package_from_zip(&package)
        .unwrap();
    assert_eq!(
        registry.installation_states().unwrap()[ID].package_digest,
        pin
    );
    cleanup(&root);
}

#[test]
fn upgrade_and_rollback_persist_the_selected_install_digest() {
    let root = temp_root();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
            "process", "1.0.0",
        ))
        .unwrap();
    let first = registry.installation_states().unwrap()[ID]
        .package_digest
        .clone();
    registry
        .upgrade_framework_package(
            ID,
            &fake_framework_package_zip_with_version("process", "2.0.0"),
        )
        .unwrap();
    assert_ne!(
        registry.installation_states().unwrap()[ID].package_digest,
        first
    );
    admit(&registry).unwrap();
    registry.rollback(ID).unwrap();
    assert_eq!(
        registry.installation_states().unwrap()[ID].package_digest,
        first
    );
    admit(&registry).unwrap();
    cleanup(&root);
}
