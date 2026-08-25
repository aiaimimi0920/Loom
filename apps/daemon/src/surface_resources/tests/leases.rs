use std::fs;

use loom_protocol::SurfaceResourceKind;
use uuid::Uuid;

use super::super::*;
use crate::unix_time_millis;

#[test]
fn lease_registration_is_capped_and_debounced_but_still_survives_a_restart() {
    let root = std::env::temp_dir().join(format!("loom-surface-leases-{}", Uuid::new_v4()));
    let (first_id, second_id) = {
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        let first = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"lease-payload-one",
                None,
                None,
                None,
            )
            .expect("register the first resource");
        // Keep the assertion independent of scheduler and filesystem latency under parallel CI.
        store.leases_persisted_at_ms =
            unix_time_millis().saturating_add(LEASE_PERSIST_DEBOUNCE_MILLIS);
        let second = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"lease-payload-two",
                None,
                None,
                None,
            )
            .expect("register the second resource");
        assert!(
            store.leases_dirty,
            "the second registration inside the debounce window must not have written the table"
        );

        let template = second.clone();
        while store.leases.len() < MAX_ACTIVE_RESOURCE_LEASES {
            let mut filler = template.clone();
            filler.lease_id = format!("lease:{}", Uuid::new_v4());
            store.leases.insert(filler.lease_id.clone(), filler);
        }
        let error = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"lease-payload-three",
                None,
                None,
                None,
            )
            .expect_err("a full lease table must refuse a new grant");
        assert!(matches!(error, SurfaceResourceStoreError::LeaseRejected(_)));
        let refused_digest = super::super::content::hex_digest(b"lease-payload-three");
        assert!(
            !root.join(format!("{refused_digest}.bin")).exists()
                && !root.join(format!("{refused_digest}.json")).exists(),
            "a rejected lease must not leave a durable orphan object"
        );
        assert!(matches!(
            store.duplicate_loom_resource_lease(&second),
            Err(SurfaceResourceStoreError::LeaseRejected(_))
        ));

        store
            .leases
            .retain(|lease_id, _| *lease_id == first.lease_id || *lease_id == second.lease_id);
        (first.lease_id, second.lease_id)
    };

    let reloaded = SurfaceResourceStore::new(&root).expect("reload store");
    assert!(
        reloaded.leases.contains_key(&first_id),
        "dropping the store must flush the debounced lease table"
    );
    assert!(reloaded.leases.contains_key(&second_id));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_duplicated_lease_gets_a_fresh_ttl_without_shortening_a_longer_one() {
    let root = std::env::temp_dir().join(format!("loom-surface-duplicate-{}", Uuid::new_v4()));
    let mut store = SurfaceResourceStore::new(&root).expect("open store");
    let short = store
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            b"almost-expired",
            None,
            None,
            Some(2_000),
        )
        .expect("register with a short lease");
    let duplicated = store
        .duplicate_loom_resource_lease(&short)
        .expect("duplicate the short lease");
    assert_ne!(duplicated.lease_id, short.lease_id);
    assert_eq!(duplicated.resource, short.resource);
    assert!(
        duplicated.expires_at_ms
            >= unix_time_millis().saturating_add(DEFAULT_RESOURCE_LEASE_MILLIS) - 5_000,
        "a duplicate must not inherit an almost-expired grant"
    );

    let long = store
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            b"long-lived",
            None,
            None,
            Some(MAX_RESOURCE_LEASE_MILLIS),
        )
        .expect("register with the longest allowed lease");
    let duplicated_long = store
        .duplicate_loom_resource_lease(&long)
        .expect("duplicate the long lease");
    assert_eq!(duplicated_long.expires_at_ms, long.expires_at_ms);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn an_expired_lease_cannot_be_duplicated_before_another_request_cleans_it_up() {
    let root = std::env::temp_dir().join(format!("loom-surface-expired-{}", Uuid::new_v4()));
    let mut store = SurfaceResourceStore::new(&root).expect("open store");
    let mut expired = store
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            b"expired-duplicate",
            None,
            None,
            None,
        )
        .expect("register resource");
    expired.expires_at_ms = 0;
    store
        .leases
        .get_mut(&expired.lease_id)
        .expect("stored lease")
        .expires_at_ms = 0;

    assert!(matches!(
        store.duplicate_loom_resource_lease(&expired),
        Err(SurfaceResourceStoreError::LeaseRejected(_))
    ));
    assert!(!store.leases.contains_key(&expired.lease_id));
    let _ = fs::remove_dir_all(root);
}
