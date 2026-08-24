use std::fs;

use loom_protocol::{SurfaceResourceKind, SurfaceResourceLease};
use uuid::Uuid;

use super::super::content::resource_digest;
use super::super::*;

#[test]
fn production_gc_age_rejects_values_below_the_safety_floor() {
    let root = std::env::temp_dir().join(format!("loom-surface-gc-floor-{}", Uuid::new_v4()));
    let error =
        match SurfaceResourceStore::new_with_gc_min_age(&root, MIN_RESOURCE_GC_AGE_MILLIS - 1) {
            Ok(_) => panic!("unsafe GC age must be rejected"),
            Err(error) => error,
        };
    assert!(error.to_string().contains("must be at least"));
    assert!(!root.exists());
}

#[test]
fn production_gc_age_accepts_a_stricter_explicit_value() {
    let root = std::env::temp_dir().join(format!("loom-surface-gc-config-{}", Uuid::new_v4()));
    let configured = MIN_RESOURCE_GC_AGE_MILLIS + 60_000;
    let store = SurfaceResourceStore::new_with_gc_min_age(&root, configured)
        .expect("open store with stricter GC age");
    assert_eq!(store.gc_min_age_ms, configured);
    drop(store);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reopening_an_unchanged_store_does_not_rewrite_the_lease_table() {
    let root = std::env::temp_dir().join(format!("loom-surface-reopen-{}", Uuid::new_v4()));
    drop(SurfaceResourceStore::new(&root).expect("create store"));
    let leases_path = root.join("leases.json");
    let modified_before = fs::metadata(&leases_path)
        .expect("lease metadata")
        .modified()
        .expect("lease modification time");
    std::thread::sleep(std::time::Duration::from_millis(50));

    let reopened = SurfaceResourceStore::new(&root).expect("reopen unchanged store");
    assert!(reopened.leases_persisted_at_ms > 0);
    drop(reopened);
    let modified_after = fs::metadata(&leases_path)
        .expect("lease metadata after reopen")
        .modified()
        .expect("lease modification time after reopen");
    assert_eq!(modified_after, modified_before);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reopening_trims_a_valid_lease_table_to_the_runtime_cap() {
    let root = std::env::temp_dir().join(format!("loom-surface-lease-cap-{}", Uuid::new_v4()));
    let mut store = SurfaceResourceStore::new(&root).expect("create store");
    let template = store
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            b"lease-cap-payload",
            None,
            None,
            None,
        )
        .expect("register resource");
    store.leases.clear();
    let base_expiration = crate::unix_time_millis().saturating_add(60_000);
    for index in 0..(MAX_ACTIVE_RESOURCE_LEASES + 3) {
        let mut lease = template.clone();
        lease.lease_id = format!("lease:loaded-{index:04}");
        lease.expires_at_ms = base_expiration.saturating_add(index as u64);
        store.leases.insert(lease.lease_id.clone(), lease);
    }
    store
        .persist_leases()
        .expect("persist oversized lease table");
    drop(store);

    let reloaded = SurfaceResourceStore::new(&root).expect("normalize oversized lease table");
    assert_eq!(reloaded.leases.len(), MAX_ACTIVE_RESOURCE_LEASES);
    assert!(!reloaded.leases.contains_key("lease:loaded-0000"));
    assert!(reloaded.leases.contains_key(&format!(
        "lease:loaded-{:04}",
        MAX_ACTIVE_RESOURCE_LEASES + 2
    )));
    drop(reloaded);
    let reloaded_again = SurfaceResourceStore::new(&root).expect("reload normalized table");
    assert_eq!(reloaded_again.leases.len(), MAX_ACTIVE_RESOURCE_LEASES);
    let _ = fs::remove_dir_all(root);
}

/// One missing or truncated payload must not prevent the daemon from opening intact resources.
#[test]
fn a_damaged_object_is_discarded_at_load_instead_of_failing_the_store() {
    let root = std::env::temp_dir().join(format!("loom-surface-damaged-{}", Uuid::new_v4()));
    let (missing, truncated, intact) = {
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        let missing = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"payload-will-be-deleted",
                None,
                None,
                None,
            )
            .expect("register the object whose payload is deleted");
        let truncated = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"payload-will-be-truncated",
                None,
                None,
                None,
            )
            .expect("register the object whose payload shrinks");
        let intact = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"payload-stays-whole",
                None,
                None,
                None,
            )
            .expect("register the object that survives");
        (missing, truncated, intact)
    };
    let digest_of = |lease: &SurfaceResourceLease| {
        resource_digest(&lease.resource.resource_id).expect("content addressed lease")
    };
    let missing_digest = digest_of(&missing);
    let truncated_digest = digest_of(&truncated);
    fs::remove_file(root.join(format!("{missing_digest}.bin"))).expect("delete a payload");
    fs::write(root.join(format!("{truncated_digest}.bin")), b"x").expect("shrink a payload");

    let mut reloaded = SurfaceResourceStore::new(&root).expect("a damaged object must not fail");
    assert_eq!(
        reloaded.resources.keys().collect::<Vec<_>>(),
        vec![&intact.resource.resource_id],
        "only the intact object may be loaded"
    );
    assert!(
        !reloaded.leases.contains_key(&missing.lease_id)
            && !reloaded.leases.contains_key(&truncated.lease_id),
        "a lease over a discarded object must not survive the load"
    );
    let intact_digest = digest_of(&intact);
    assert_eq!(
        reloaded
            .get_with_lease(&intact_digest, &intact.lease_id)
            .expect("the intact object stays readable")
            .bytes,
        b"payload-stays-whole"
    );
    let _ = fs::remove_dir_all(root);
}
