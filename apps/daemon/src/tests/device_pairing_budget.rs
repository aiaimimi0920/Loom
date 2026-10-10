fn pairing_payload() -> Value {
    let key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    json!({"name":"Bounded pairing", "kind":"computer", "address":"hook://fixture",
        "publicKey":BASE64.encode(key.verifying_key().to_bytes())})
}

fn load_pairing_registry_snapshot(path: &Path, port: u16) -> DeviceRegistryStore {
    // Isolate the other registry-owned stores: the live fixture holds their DB locks.
    let root = path
        .parent()
        .unwrap()
        .join(format!("pairing-reload-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let snapshot = root.join("devices.json");
    fs::copy(path, &snapshot).unwrap();
    DeviceRegistryStore::new(snapshot, format!("127.0.0.1:{port}").parse().unwrap()).unwrap()
}

#[test]
fn public_pairing_capacity_is_persistent_and_duplicate_requests_do_not_write() {
    let f = Fixture::new();
    let first = pairing_payload();
    for index in 0..128 {
        let payload = if index == 0 {
            first.clone()
        } else {
            pairing_payload()
        };
        assert_eq!(
            wall_http::public(f.port, "POST", "/v1/devices/requests", Some(payload)).0,
            200
        );
    }
    let path = f.devices.lock().unwrap().path.clone();
    let before = fs::read(&path).unwrap();
    let (status, error) = wall_http::public(
        f.port,
        "POST",
        "/v1/devices/requests",
        Some(pairing_payload()),
    );
    assert_eq!(status, 429);
    assert_eq!(error["error"]["code"], "device_pairing_capacity");
    assert_eq!(
        wall_http::public(f.port, "POST", "/v1/devices/requests", Some(first)).0,
        200
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    let loaded = load_pairing_registry_snapshot(&path, f.port);
    assert_eq!(
        loaded
            .devices
            .values()
            .filter(|d| d.approval == "pending")
            .count(),
        128
    );
    assert_eq!(
        wall_http::admin(f.port, "POST", "/v1/devices", Some(pairing_payload())).0,
        200
    );
    {
        let mut store = f.devices.lock().unwrap();
        let id = store
            .devices
            .values()
            .find(|d| d.approval == "pending")
            .unwrap()
            .id
            .clone();
        store.devices.get_mut(&id).unwrap().created_at = 1;
        store.persist().unwrap();
    }
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/devices/requests",
            Some(pairing_payload())
        )
        .0,
        200
    );
    assert_eq!(
        f.devices
            .lock()
            .unwrap()
            .devices
            .values()
            .filter(|d| d.approval == "pending")
            .count(),
        128
    );
}

#[test]
fn public_pairing_expiry_is_durable_on_reload_and_blocks_stale_approval() {
    let f = Fixture::new();
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/devices/requests",
            Some(pairing_payload())
        )
        .0,
        200
    );
    let (id, path) = {
        let mut store = f.devices.lock().unwrap();
        let id = store
            .devices
            .values()
            .find(|d| d.approval == "pending")
            .unwrap()
            .id
            .clone();
        store.devices.get_mut(&id).unwrap().created_at = 1;
        store.persist().unwrap();
        (id, store.path.clone())
    };
    let loaded = load_pairing_registry_snapshot(&path, f.port);
    assert!(!loaded.devices.contains_key(&id));
    let persisted: DeviceRegistryDocument =
        serde_json::from_slice(&fs::read(&loaded.path).unwrap()).unwrap();
    assert!(!persisted.devices.iter().any(|d| d.id == id));
    assert_eq!(
        wall_http::admin(
            f.port,
            "POST",
            &format!("/v1/devices/{id}/approve"),
            Some(json!({}))
        )
        .0,
        404
    );
}

#[test]
fn public_pairing_expiry_failure_restores_memory_and_obeys_exact_deadline() {
    let f = Fixture::new();
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/devices/requests",
            Some(pairing_payload())
        )
        .0,
        200
    );
    let mut store = f.devices.lock().unwrap();
    let id = store
        .devices
        .values()
        .find(|d| d.approval == "pending")
        .unwrap()
        .id
        .clone();
    store.devices.get_mut(&id).unwrap().created_at = 1;
    let original_path = store.path.clone();
    let invalid_path = f.root.join("pairing-persistence-denied");
    fs::create_dir(&invalid_path).unwrap();
    fs::write(invalid_path.join("keep"), b"unchanged").unwrap();
    store.path = invalid_path.clone();
    assert!(store
        .expire_pending_pairings(DEVICE_PAIRING_TTL_MILLIS + 1)
        .is_err());
    assert!(store.devices.contains_key(&id));
    assert_eq!(fs::read(invalid_path.join("keep")).unwrap(), b"unchanged");
    store.path = original_path;
    store
        .expire_pending_pairings(DEVICE_PAIRING_TTL_MILLIS)
        .unwrap();
    assert!(store.devices.contains_key(&id));
    store
        .expire_pending_pairings(DEVICE_PAIRING_TTL_MILLIS + 1)
        .unwrap();
    assert!(!store.devices.contains_key(&id));
}

#[test]
fn public_pairing_expiry_preserves_disabled_and_previously_revoked_identities() {
    let f = Fixture::new();
    for _ in 0..4 {
        assert_eq!(
            wall_http::public(
                f.port,
                "POST",
                "/v1/devices/requests",
                Some(pairing_payload())
            )
            .0,
            200
        );
    }
    let (expired, disabled, revoked, approved, path) = {
        let mut store = f.devices.lock().unwrap();
        let ids: Vec<_> = store
            .devices
            .values()
            .filter(|d| !d.is_local)
            .map(|d| d.id.clone())
            .collect();
        for id in &ids {
            store.devices.get_mut(id).unwrap().created_at = 1;
        }
        store.devices.get_mut(&ids[1]).unwrap().enabled = false;
        store.devices.get_mut(&ids[2]).unwrap().session_epoch = 2;
        store.devices.get_mut(&ids[3]).unwrap().approval = "approved".into();
        store.persist().unwrap();
        (
            ids[0].clone(),
            ids[1].clone(),
            ids[2].clone(),
            ids[3].clone(),
            store.path.clone(),
        )
    };
    assert_eq!(wall_http::admin(f.port, "GET", "/v1/devices", None).0, 200);
    let store = f.devices.lock().unwrap();
    assert!(!store.devices.contains_key(&expired));
    assert!(!store.devices[&disabled].enabled);
    assert_eq!(store.devices[&revoked].session_epoch, 2);
    assert_eq!(store.devices[&approved].approval, "approved");
    let persisted: DeviceRegistryDocument =
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(!persisted.devices.iter().any(|d| d.id == expired));
}
