// Public registration may identify only the submitted key, never enumerate peers.
#[test]
fn public_pairing_returns_only_submitted_identity_receipt() {
    let f = Fixture::new();
    let other = pairing_payload();
    assert_eq!(
        wall_http::admin(f.port, "POST", "/v1/devices", Some(other)).0,
        200
    );
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
    let mut request = pairing_payload();
    request["name"] = json!("Submitted name");
    let mut expected_id = None;
    for state in ["new", "pending", "approved", "disabled"] {
        if state != "new" {
            let mut store = f.devices.lock().unwrap();
            let device = store
                .devices
                .get_mut(expected_id.as_ref().unwrap())
                .unwrap();
            device.name = "Private administrator label".into();
            device.address = "private://administrator-address".into();
            if state != "pending" {
                device.approval = "approved".into();
                device.session_epoch = 7;
            }
            if state == "disabled" {
                device.enabled = false;
            }
            store.persist().unwrap();
        }
        let before = f.devices.lock().unwrap().devices.len();
        let (status, receipt) = wall_http::public(
            f.port,
            "POST",
            "/v1/devices/requests",
            Some(request.clone()),
        );
        assert_eq!(status, 200);
        let root = receipt.as_object().unwrap();
        assert_eq!(
            root.len(),
            2,
            "public receipt must not carry registry metadata"
        );
        let entries = receipt["pending"]
            .as_array()
            .unwrap()
            .iter()
            .chain(receipt["devices"].as_array().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            entries.len(),
            1,
            "public registration must not enumerate peers"
        );
        let entry = entries[0];
        let mut fields = entry
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        fields.sort_unstable();
        assert_eq!(fields, ["approval", "enabled", "id", "name", "publicKey"]);
        assert_eq!(entry["publicKey"], request["publicKey"]);
        assert_eq!(entry["name"], request["name"]);
        assert_eq!(entry["enabled"], state != "disabled");
        let id = entry["id"].as_str().unwrap().to_owned();
        assert_eq!(
            Uuid::parse_str(id.strip_prefix("device-").unwrap())
                .unwrap()
                .get_version_num(),
            4
        );
        if let Some(expected) = &expected_id {
            assert_eq!(&id, expected);
        }
        expected_id = Some(id.clone());
        let store = f.devices.lock().unwrap();
        assert_eq!(store.devices.len(), before + usize::from(state == "new"));
        if state != "new" {
            assert_eq!(store.devices[&id].name, "Private administrator label");
        }
        if state == "disabled" {
            assert!(!store.devices[&id].enabled);
            assert_eq!(store.devices[&id].session_epoch, 7);
        }
    }
    let (status, registry) = wall_http::admin(f.port, "GET", "/v1/devices", None);
    assert_eq!(status, 200);
    assert!(registry.get("connectedClients").is_some());
    assert_eq!(
        registry["devices"].as_array().unwrap().len()
            + registry["pending"].as_array().unwrap().len(),
        4
    );
    assert!(registry
        .to_string()
        .contains("private://administrator-address"));
}

#[test]
fn managed_device_online_tracks_recent_authenticated_activity_without_losing_history() {
    let f = Fixture::new();
    let (local, local_token) = wall_http::pair(f.port, "Local Otter");
    let (code, _) = wall_http::pair(f.port, "CODE");
    let (old, _) = wall_http::pair(f.port, "Historical Otter");
    let list = || wall_http::admin(f.port, "GET", "/v1/devices", None).1;
    let online = |value: &serde_json::Value| {
        value["devices"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["isLocal"] != true && entry["online"] == true)
            .map(|entry| entry["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    {
        let mut store = f.devices.lock().unwrap();
        for session in store.sessions.values_mut().filter(|s| s.device_id == old) {
            session.last_activity_at_ms = unix_time_millis().saturating_sub(30_001);
        }
    }
    let current = list();
    let active = online(&current);
    assert_eq!(active.len(), 2);
    assert!(active.contains(&local) && active.contains(&code));
    assert_eq!(current["devices"].as_array().unwrap().len(), 4);
    {
        let mut store = f.devices.lock().unwrap();
        for session in store.sessions.values_mut() {
            session.last_activity_at_ms = 0;
        }
    }
    assert!(
        online(&list()).is_empty(),
        "a 15-minute credential is not online evidence"
    );
    assert_eq!(
        wall_http::device(
            f.port,
            &local_token,
            "POST",
            "/v1/projections/targets",
            Some(json!({}))
        )
        .0,
        200
    );
    assert_eq!(online(&list()), vec![local.clone()]);
    f.devices
        .lock()
        .unwrap()
        .devices
        .get_mut(&local)
        .unwrap()
        .session_epoch += 1;
    assert!(
        online(&list()).is_empty(),
        "old-epoch activity must not revive a device"
    );
}
