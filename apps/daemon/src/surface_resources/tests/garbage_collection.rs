use std::collections::BTreeSet;
use std::fs;

use loom_protocol::SurfaceResourceKind;
use uuid::Uuid;

use super::super::content::resource_digest;
use super::super::*;

#[test]
fn gc_keeps_an_object_a_reference_still_names_after_its_lease_is_gone() {
    let root = std::env::temp_dir().join(format!("loom-surface-gc-ref-{}", Uuid::new_v4()));
    let mut store = SurfaceResourceStore::new(&root).expect("open store");
    store.set_gc_min_age_ms(0);
    let lease = store
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            b"referenced-by-an-instance",
            None,
            None,
            None,
        )
        .expect("register resource");
    let digest = resource_digest(&lease.resource.resource_id).expect("content addressed lease");
    let payload = root.join(format!("{digest}.bin"));
    let metadata = root.join(format!("{digest}.json"));
    store.release(&lease.lease_id).expect("release the lease");

    let mut referenced = BTreeSet::new();
    referenced.insert(lease.resource.resource_id.clone());
    let kept = store.collect_garbage(&referenced);
    assert_eq!(kept.removed_objects, 0);
    assert_eq!(kept.removed_orphan_files, 0);
    assert_eq!(kept.retained_objects, 1);
    assert_eq!(kept.failures, 0);
    assert!(payload.is_file() && metadata.is_file());

    let swept = store.collect_garbage(&BTreeSet::new());
    assert_eq!(swept.removed_objects, 1);
    assert_eq!(swept.removed_bytes, lease.resource.size);
    assert_eq!(swept.retained_objects, 0);
    assert_eq!(swept.failures, 0);
    assert!(!payload.exists() && !metadata.exists());
    assert!(store.resources.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn gc_leaves_a_young_unreferenced_object_alone() {
    let root = std::env::temp_dir().join(format!("loom-surface-gc-young-{}", Uuid::new_v4()));
    let mut store = SurfaceResourceStore::new(&root).expect("open store");
    let lease = store
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            b"just-registered",
            None,
            None,
            Some(1),
        )
        .expect("register with the shortest possible lease");
    let digest = resource_digest(&lease.resource.resource_id).expect("content addressed lease");
    store.release(&lease.lease_id).expect("release the lease");

    let outcome = store.collect_garbage(&BTreeSet::new());
    assert_eq!(outcome.removed_objects, 0);
    assert_eq!(outcome.retained_objects, 1);
    assert!(root.join(format!("{digest}.bin")).is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn gc_sweeps_orphan_halves_but_not_the_lease_table_or_a_temporary() {
    let root = std::env::temp_dir().join(format!("loom-surface-gc-orphan-{}", Uuid::new_v4()));
    let mut store = SurfaceResourceStore::new(&root).expect("open store");
    store.set_gc_min_age_ms(0);
    let orphan_payload = root.join(format!("{}.bin", "ab".repeat(32)));
    let orphan_metadata = root.join(format!("{}.json", "cd".repeat(32)));
    let temporary = root.join(format!(".{}.bin.tmp-4242-abc-0", "ef".repeat(32)));
    let unrelated = root.join("notes.txt");
    for path in [&orphan_payload, &orphan_metadata, &temporary, &unrelated] {
        fs::write(path, b"stray").expect("write a stray file");
    }

    let outcome = store.collect_garbage(&BTreeSet::new());
    assert_eq!(outcome.removed_orphan_files, 2);
    assert_eq!(outcome.removed_objects, 0);
    assert_eq!(outcome.failures, 0);
    assert!(!orphan_payload.exists() && !orphan_metadata.exists());
    assert!(
        temporary.is_file(),
        "a write_atomic temporary is not an orphan; its extension is the tmp-... tail"
    );
    assert!(unrelated.is_file(), "the sweep only knows .bin and .json");
    assert!(
        root.join("leases.json").is_file(),
        "the lease table must never be swept"
    );
    let _ = fs::remove_dir_all(root);
}
