use std::fs;

use loom_protocol::{SurfaceResourceKind, SurfaceResourceTransport, SurfaceResourceTransportKind};
use uuid::Uuid;

use super::super::*;
use crate::unix_time_millis;

#[test]
fn resources_are_content_addressed_reused_and_verified_after_restart() {
    let root = std::env::temp_dir().join(format!("loom-surface-resources-{}", Uuid::new_v4()));
    let first = {
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        let first = store
            .register(
                SurfaceResourceKind::Image,
                "image/png",
                b"fixture-image",
                Some(2),
                Some(3),
                None,
            )
            .expect("register resource");
        let second = store
            .register(
                SurfaceResourceKind::Image,
                "image/png",
                b"fixture-image",
                Some(2),
                Some(3),
                None,
            )
            .expect("reuse resource");
        assert_eq!(first.resource, second.resource);
        assert_ne!(first.lease_id, second.lease_id);
        first
    };
    let mut reloaded = SurfaceResourceStore::new(&root).expect("reload store");
    let digest = first
        .resource
        .resource_id
        .strip_prefix("sha256:")
        .expect("digest");
    let payload = reloaded
        .get_with_lease(digest, &first.lease_id)
        .expect("read leased resource after restart");
    assert_eq!(payload.bytes, b"fixture-image");
    assert_eq!(payload.descriptor, first.resource);
    reloaded
        .validate_references(
            std::slice::from_ref(&first.resource),
            std::slice::from_ref(&first),
        )
        .expect("validate host-issued resource references");
    let mut forged = first.clone();
    forged.expires_at_ms = forged.expires_at_ms.saturating_add(1);
    assert!(matches!(
        reloaded.validate_references(&[], &[forged]),
        Err(SurfaceResourceStoreError::LeaseRejected(_))
    ));
    let mut forged_descriptor = first.resource.clone();
    forged_descriptor.mime = "image/webp".to_owned();
    assert!(matches!(
        reloaded.validate_references(&[forged_descriptor], &[]),
        Err(SurfaceResourceStoreError::Invalid(_))
    ));
    assert!(matches!(
        reloaded.replace_lease_transport(
            &first.lease_id,
            SurfaceResourceTransport {
                kind: SurfaceResourceTransportKind::Stream,
                handle: None,
                path: None,
                stream_id: Some("stream:forged".to_owned()),
            },
        ),
        Err(SurfaceResourceStoreError::Invalid(_))
    ));
    assert!(matches!(
        reloaded.get_with_lease(digest, "lease:missing"),
        Err(SurfaceResourceStoreError::LeaseRejected(_))
    ));
    assert!(reloaded
        .release(&first.lease_id)
        .expect("release lease")
        .is_some());
    assert!(matches!(
        reloaded.get_with_lease(digest, &first.lease_id),
        Err(SurfaceResourceStoreError::LeaseRejected(_))
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn persisted_resource_can_receive_a_fresh_lease_after_the_old_lease_expires() {
    let root = std::env::temp_dir().join(format!("loom-surface-renew-{}", Uuid::new_v4()));
    let mut store = SurfaceResourceStore::new(&root).expect("open store");
    let expired = store
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            b"persistent-resource",
            None,
            None,
            None,
        )
        .expect("register resource");
    store
        .leases
        .get_mut(&expired.lease_id)
        .expect("stored lease")
        .expires_at_ms = 0;
    store.persist_leases().expect("persist expired lease");

    let renewed = store
        .renew_loom_resource_lease(&expired)
        .expect("renew persisted resource lease");
    assert_ne!(renewed.lease_id, expired.lease_id);
    assert_eq!(renewed.resource, expired.resource);
    assert!(renewed.expires_at_ms > unix_time_millis());
    let digest = renewed
        .resource
        .resource_id
        .strip_prefix("sha256:")
        .expect("digest");
    assert_eq!(
        store
            .get_with_lease(digest, &renewed.lease_id)
            .expect("read renewed resource")
            .bytes,
        b"persistent-resource"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn descriptor_validation_trusts_an_unchanged_payload_and_re_verifies_a_changed_one() {
    let root = std::env::temp_dir().join(format!("loom-surface-stamp-{}", Uuid::new_v4()));
    let mut store = SurfaceResourceStore::new(&root).expect("open store");
    let lease = store
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            b"stamped-payload",
            None,
            None,
            None,
        )
        .expect("register resource");
    let digest = lease
        .resource
        .resource_id
        .strip_prefix("sha256:")
        .expect("digest")
        .to_owned();
    let payload_path = root.join(format!("{digest}.bin"));

    store
        .validate_descriptor(&lease.resource)
        .expect("registration stamps the payload it just wrote");

    // Same length and restored mtime exercise the deliberately cheap stamp fast path.
    let times = fs::FileTimes::new()
        .set_accessed(
            fs::metadata(&payload_path)
                .expect("payload metadata")
                .accessed()
                .expect("accessed time"),
        )
        .set_modified(
            fs::metadata(&payload_path)
                .expect("payload metadata")
                .modified()
                .expect("modified time"),
        );
    assert_eq!(b"tampered-paylod".len(), b"stamped-payload".len());
    fs::write(&payload_path, b"tampered-paylod").expect("tamper with the payload");
    fs::File::options()
        .write(true)
        .open(&payload_path)
        .expect("reopen payload")
        .set_times(times)
        .expect("restore the payload timestamps");

    store
        .validate_descriptor(&lease.resource)
        .expect("an unchanged stamp is trusted without a re-hash");
    assert!(matches!(
        store.get(&digest),
        Err(SurfaceResourceStoreError::Invalid(_))
    ));
    assert!(matches!(
        store.validate_descriptor(&lease.resource),
        Err(SurfaceResourceStoreError::Invalid(_))
    ));
    let _ = fs::remove_dir_all(root);
}
