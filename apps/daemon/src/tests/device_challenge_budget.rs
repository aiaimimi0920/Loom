// 公开挑战的容量拒绝不得驱逐、刷新或破坏已有合法签发。
fn request_budget_challenge(port: u16, id: &str) -> (u16, Value) {
    wall_http::public(
        port,
        "POST",
        "/v1/device-sessions/challenges",
        Some(json!({"deviceId": id})),
    )
}

fn challenge_budget_snapshot(store: &DeviceRegistryStore) -> Vec<(String, String, String, u64)> {
    store
        .challenges
        .values()
        .map(|c| {
            (
                c.challenge_id.clone(),
                c.device_id.clone(),
                c.challenge.clone(),
                c.expires_at_ms,
            )
        })
        .collect()
}

#[test]
fn device_challenge_budget_preserves_existing_signed_challenge() {
    use ed25519_dalek::{Signer as _, SigningKey};
    let f = Fixture::new();
    let key = SigningKey::generate(&mut OsRng);
    let (id, _) = wall_http::pair_with_key(f.port, "Challenge budget", &key);
    let mut first = Value::Null;
    for index in 0..4 {
        let (status, challenge) = request_budget_challenge(f.port, &id);
        assert_eq!(status, 201);
        if index == 0 {
            first = challenge;
        }
    }
    let before = challenge_budget_snapshot(&f.devices.lock().unwrap());
    for _ in 0..8 {
        let (status, error) = request_budget_challenge(f.port, &id);
        assert_eq!(status, 429);
        assert_eq!(error["error"]["code"], "device_challenge_device_capacity");
    }
    assert_eq!(
        challenge_budget_snapshot(&f.devices.lock().unwrap()),
        before
    );
    let (other, _) = wall_http::pair(f.port, "Other challenge device");
    assert_eq!(request_budget_challenge(f.port, &other).0, 201);
    let nonce = Uuid::new_v4().to_string();
    let message = device_session_signature_message(
        &id,
        first["challengeId"].as_str().unwrap(),
        first["challenge"].as_str().unwrap(),
        &nonce,
    );
    let (status, session) = wall_http::public(
        f.port,
        "POST",
        "/v1/device-sessions",
        Some(json!({
            "deviceId": id, "challengeId": first["challengeId"], "clientNonce": nonce,
            "signature": BASE64.encode(key.sign(message.as_bytes()).to_bytes()),
        })),
    );
    assert_eq!(status, 201);
    assert!(session["token"].as_str().is_some());
    assert_eq!(request_budget_challenge(f.port, &id).0, 201);
    {
        let mut store = f.devices.lock().unwrap();
        let expired = store
            .challenges
            .values_mut()
            .find(|c| c.device_id == id)
            .unwrap();
        expired.expires_at_ms = 0;
    }
    assert_eq!(request_budget_challenge(f.port, &id).0, 201);
    assert_eq!(request_budget_challenge(f.port, &id).0, 429);
}

#[test]
fn device_challenge_budget_global_limit_and_authorization_precedence() {
    let f = Fixture::new();
    let (id, _) = wall_http::pair(f.port, "Global challenge fixture");
    {
        let mut store = f.devices.lock().unwrap();
        let template = store.devices[&id].clone();
        for index in 0..65 {
            let mut device = template.clone();
            device.id = format!("budget-device-{index}");
            let device_id = device.id.clone();
            store.devices.insert(device_id.clone(), device);
            if index < 64 {
                for _ in 0..4 {
                    store.create_session_challenge(&device_id).unwrap();
                }
            }
        }
        assert_eq!(store.challenges.len(), 256);
    }
    let before = challenge_budget_snapshot(&f.devices.lock().unwrap());
    let (status, error) = request_budget_challenge(f.port, "budget-device-64");
    assert_eq!(status, 429);
    assert_eq!(error["error"]["code"], "device_challenge_capacity");
    assert_eq!(
        challenge_budget_snapshot(&f.devices.lock().unwrap()),
        before
    );
    for (state, expected_status, expected_code) in [
        ("disabled", 403, "device_disabled"),
        ("pending", 403, "device_not_authorized"),
        ("keyless", 409, "device_key_missing"),
        ("missing", 404, "device_not_found"),
    ] {
        {
            let mut store = f.devices.lock().unwrap();
            let device = store.devices.get_mut(&id).unwrap();
            device.enabled = state != "disabled";
            device.approval = if state == "disabled" || state == "pending" {
                "pending"
            } else {
                "approved"
            }
            .into();
            if state == "keyless" {
                device.public_key = None;
            }
        }
        let request_id = if state == "missing" {
            "not-present"
        } else {
            &id
        };
        let (status, error) = request_budget_challenge(f.port, request_id);
        assert_eq!(
            (status, error["error"]["code"].as_str().unwrap()),
            (expected_status, expected_code)
        );
    }
    assert_eq!(
        challenge_budget_snapshot(&f.devices.lock().unwrap()),
        before
    );
    {
        let mut store = f.devices.lock().unwrap();
        store.challenges.values_mut().next().unwrap().expires_at_ms = 0;
    }
    let statuses = concurrent_budget_challenges(f.port, "budget-device-64");
    assert_eq!(statuses.iter().filter(|&&s| s == 201).count(), 1);
    assert!(statuses.iter().all(|s| [201, 429].contains(s)));
    assert_eq!(f.devices.lock().unwrap().challenges.len(), 256);
}

fn concurrent_budget_challenges(port: u16, id: &str) -> Vec<u16> {
    let start = Arc::new(std::sync::Barrier::new(16));
    let workers: Vec<_> = (0..16)
        .map(|_| {
            let start = Arc::clone(&start);
            let id = id.to_owned();
            thread::spawn(move || {
                start.wait();
                request_budget_challenge(port, &id).0
            })
        })
        .collect();
    workers.into_iter().map(|w| w.join().unwrap()).collect()
}

#[test]
fn device_challenge_budget_concurrent_public_requests_cannot_exceed_device_limit() {
    let f = Fixture::new();
    let (id, _) = wall_http::pair(f.port, "Concurrent challenge fixture");
    let statuses = concurrent_budget_challenges(f.port, &id);
    assert_eq!(statuses.iter().filter(|&&s| s == 201).count(), 4);
    assert!(statuses.iter().all(|s| [201, 429].contains(s)));
    assert_eq!(f.devices.lock().unwrap().challenges.len(), 4);
}
