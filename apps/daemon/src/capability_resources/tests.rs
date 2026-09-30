use std::fs;

use loom_protocol::{ExtensionResourceKind, ExtensionResourceRef, SurfaceResourceKind};

use super::*;
use crate::surface_resources::SurfaceResourceStore;

#[test]
fn stages_verified_resource_and_releases_owner_directory() {
    let root = test_root("stage");
    let store = test_store(&root);
    let reference = register_reference(&store, b"capability payload");
    let broker = CapabilityResourceBroker::open(root.join("broker")).expect("broker");

    let lease = broker
        .stage(
            &store,
            "publisher.example/fixture",
            "scope:fixture",
            "request-1",
            std::slice::from_ref(&reference),
        )
        .expect("stage resource");
    let resource = lease.resources().first().expect("staged resource");
    assert_eq!(resource.resource_ref, reference);
    assert_eq!(
        fs::read(&resource.staged_path).unwrap(),
        b"capability payload"
    );
    assert!(fs::metadata(&resource.staged_path)
        .unwrap()
        .permissions()
        .readonly());
    let active = resource.staged_path.parent().unwrap().to_path_buf();
    let journal: serde_json::Value =
        serde_json::from_slice(&fs::read(active.join("lease.json")).unwrap()).unwrap();
    assert_eq!(journal["ownerRequest"], "request-1");
    assert_eq!(journal["resources"][0]["refcount"], 1);

    drop(lease);
    assert!(!active.exists());
    drop(broker);
    cleanup(&root);
}

#[test]
fn rejects_mismatched_metadata_without_leaving_staging() {
    let root = test_root("mismatch");
    let store = test_store(&root);
    let mut reference = register_reference(&store, b"capability payload");
    reference.byte_length += 1;
    let broker = CapabilityResourceBroker::open(root.join("broker")).expect("broker");
    assert!(broker
        .stage(
            &store,
            "publisher.example/fixture",
            "scope:fixture",
            "request-2",
            &[reference]
        )
        .is_err());
    assert_eq!(fs::read_dir(root.join("broker/active")).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(root.join("broker/prepared")).unwrap().count(),
        0
    );
    drop(broker);
    cleanup(&root);
}

#[test]
fn startup_recovers_orphaned_prepared_and_active_directories() {
    let root = test_root("recover");
    fs::create_dir_all(root.join("broker/prepared/orphan")).unwrap();
    fs::create_dir_all(root.join("broker/active/orphan")).unwrap();
    fs::write(root.join("broker/active/orphan/file.bin"), b"orphan").unwrap();

    let broker = CapabilityResourceBroker::open(root.join("broker")).expect("recover broker");
    assert_eq!(fs::read_dir(root.join("broker/active")).unwrap().count(), 0);
    assert_eq!(
        fs::read_dir(root.join("broker/prepared")).unwrap().count(),
        0
    );
    drop(broker);
    cleanup(&root);
}

#[test]
fn rejects_a_second_broker_for_the_same_staging_root() {
    let root = test_root("exclusive-owner");
    let broker = CapabilityResourceBroker::open(root.join("broker")).expect("first broker");

    assert!(matches!(
        CapabilityResourceBroker::open(root.join("broker")),
        Err(CapabilityResourceError::Busy)
    ));

    drop(broker);
    CapabilityResourceBroker::open(root.join("broker")).expect("reopen after owner exits");
    cleanup(&root);
}

fn test_store(root: &Path) -> SharedSurfaceResourceStore {
    Arc::new(Mutex::new(
        SurfaceResourceStore::new(root.join("surface-resources")).expect("surface store"),
    ))
}

fn register_reference(store: &SharedSurfaceResourceStore, bytes: &[u8]) -> ExtensionResourceRef {
    let lease = store
        .lock()
        .unwrap()
        .register(
            SurfaceResourceKind::File,
            "application/octet-stream",
            bytes,
            None,
            None,
            None,
        )
        .unwrap();
    let digest = lease
        .resource
        .resource_id
        .strip_prefix("sha256:")
        .unwrap()
        .to_owned();
    ExtensionResourceRef {
        resource_id: lease.resource.resource_id,
        kind: ExtensionResourceKind::File,
        digest,
        byte_length: lease.resource.size,
        lease_id: lease.lease_id,
    }
}

fn test_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "loom-capability-resource-{label}-{}-{}",
        std::process::id(),
        Uuid::new_v4().simple()
    ))
}

fn cleanup(root: &Path) {
    let _ = fs::remove_dir_all(root);
}
