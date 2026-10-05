// Provenance is sticky for the admitted session and reclaimed with its Arc, not a tombstone map.
fn grant_for(f: &Fixture, owner: &str, token: &str) -> LiveMediaDeviceGrant {
    let hash = sha256_bytes(token.as_bytes());
    let revoked = Arc::clone(&f.devices.lock().unwrap().sessions[&hash].revoked);
    LiveMediaDeviceGrant {
        device_id: owner.to_owned(),
        device_session: Some((Arc::clone(&f.devices), hash, revoked)),
    }
}

#[test]
fn explicit_revoke_provenance_survives_removal_and_device_reenable() {
    let f = Fixture::new();
    let (owner, token) = wall_http::pair(f.port, "Sticky session revoke");
    let grant = grant_for(&f, &owner, &token);
    for enabled in [false, true] {
        let body = json!({"name":"Sticky session revoke", "kind":"computer", "address":"127.0.0.1", "enabled":enabled}).to_string();
        let response = http_request(f.port, "PUT", &format!("/v1/devices/{owner}"), Some(&body));
        assert_eq!(response.split_whitespace().nth(1), Some("200"));
    }
    assert!(!grant.valid());
    assert_eq!(
        grant.revocation_close().unwrap().reason,
        "live_media_device_revoked"
    );
    let mut store = f.devices.lock().unwrap();
    assert!(store.devices[&owner].enabled);
    // Reissuing even the same digest in this isolated fixture cannot revive the admitted grant.
    let epoch = store.devices[&owner].session_epoch;
    store.sessions.insert(
        sha256_bytes(token.as_bytes()),
        ActiveDeviceSession {
            device_id: owner,
            expires_at_ms: unix_time_millis().saturating_add(60_000),
            session_epoch: epoch,
            used_nonces: BTreeSet::new(),
            revoked: Arc::new(AtomicBool::new(false)),
        },
    );
    drop(store);
    assert!(!grant.valid());
    assert!(grant.revocation_close().is_some());
}

#[test]
fn expiry_and_nonce_eviction_do_not_claim_deliberate_device_revocation() {
    for expiry in [true, false] {
        let f = Fixture::new();
        let (owner, token) = wall_http::pair(f.port, "Renewable session");
        let grant = grant_for(&f, &owner, &token);
        let hash = sha256_bytes(token.as_bytes());
        let mut store = f.devices.lock().unwrap();
        let session = store.sessions.get_mut(&hash).unwrap();
        if expiry {
            session.expires_at_ms = unix_time_millis();
            store.cleanup_expired_device_auth();
        } else {
            session.used_nonces = (0..DEVICE_SESSION_MAX_NONCES)
                .map(|i| format!("nonce-{i}"))
                .collect();
            let error = store
                .authenticate_device_session(&token, &Uuid::new_v4().to_string())
                .unwrap_err();
            assert_eq!(error.code, "device_session_nonce_capacity");
        }
        assert!(!store.sessions.contains_key(&hash));
        drop(store);
        assert!(!grant.valid());
        assert!(grant.revocation_close().is_none());
    }
}

#[test]
fn failed_management_persist_preserves_device_sessions_and_revoke_provenance() {
    for method in ["PUT", "DELETE"] {
        let f = Fixture::new();
        let (owner, token) = wall_http::pair(f.port, "Persistence rollback");
        let grant = grant_for(&f, &owner, &token);
        let blocker = f.root.join("not-a-directory");
        fs::write(&blocker, b"fixture").unwrap();
        let original_path = {
            let mut store = f.devices.lock().unwrap();
            std::mem::replace(&mut store.path, blocker.join("devices.json"))
        };
        let body = json!({"name":"Persistence rollback", "kind":"computer", "address":"127.0.0.1", "enabled":false}).to_string();
        let response = http_request(
            f.port,
            method,
            &format!("/v1/devices/{owner}"),
            if method == "PUT" { Some(&body) } else { None },
        );
        f.devices.lock().unwrap().path = original_path;
        assert_eq!(response.split_whitespace().nth(1), Some("500"));
        assert!(
            grant.valid(),
            "failed management operation revoked a live grant"
        );
        assert!(grant.revocation_close().is_none());
    }
}
