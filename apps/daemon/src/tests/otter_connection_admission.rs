fn seed_otter_connection_requests(f: &Fixture, count: usize) {
    let mut store = f.devices.lock().unwrap();
    for index in 0..count {
        let id = format!("seed-{index}");
        store.connection_requests.insert(
            id.clone(),
            OtterConnectionRequest {
                lan: false,
                id,
                device_id: "seed-device".into(),
                name: "Capacity fixture".into(),
                phase: "pending".into(),
                expires_at_ms: unix_time_millis() + OTTER_CONNECTION_TTL_MS,
                receipt_digest: "unused".into(),
                device_epoch: 0,
            },
        );
    }
}

#[test]
fn otter_connection_anonymous_budget_cannot_block_authenticated_admission() {
    let f = Fixture::new();
    let (id, token) = wall_http::pair(f.port, "Known Otter");
    let key = f.devices.lock().unwrap().devices[&id]
        .public_key
        .clone()
        .unwrap();
    {
        let mut store = f.devices.lock().unwrap();
        store.connection_attempts = vec![unix_time_millis(); 20];
    }
    seed_otter_connection_requests(&f, OTTER_CONNECTION_PUBLIC_LIMIT);
    let input = json!({"name":"Known Otter","publicKey":key});
    assert_eq!(
        wall_http::public(
            f.port,
            "POST",
            "/v1/otter/connections/request",
            Some(input.clone())
        )
        .0,
        429
    );
    // A valid session with somebody else's public key still uses the public pool.
    let other = pairing_payload();
    assert_eq!(
        wall_http::device(
            f.port,
            &token,
            "POST",
            "/v1/otter/connections/request",
            Some(json!({"name":"Other Otter","publicKey":other["publicKey"]}))
        )
        .0,
        429
    );
    let (status, response) = wall_http::device(
        f.port,
        &token,
        "POST",
        "/v1/otter/connections/request",
        Some(input),
    );
    assert_eq!(status, 201, "{response}");
    assert_eq!(response["connection"]["phase"], "pending");
    let store = f.devices.lock().unwrap();
    assert_eq!(store.connection_attempts.len(), 20);
    assert_eq!(store.connection_authenticated_attempts.len(), 1);
}

#[test]
fn otter_connection_concurrent_requests_do_not_overfill_public_capacity() {
    let f = Fixture::new();
    let policy = json!({"name":"Test Loom","discoverable":false,"endpoint":null,
        "approval":"manual","password":"test-password-0123"});
    assert_eq!(
        wall_http::admin(f.port, "PUT", "/v1/otter/connections/policy", Some(policy)).0,
        200
    );
    seed_otter_connection_requests(&f, OTTER_CONNECTION_PUBLIC_LIMIT - 1);
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let workers: Vec<_> = (0..8).map(|_| {
        let barrier = Arc::clone(&barrier);
        let devices = Arc::clone(&f.devices);
        let bridge = Arc::clone(&f.bridge);
        thread::spawn(move || {
            let payload = pairing_payload();
            barrier.wait();
            // Exercise the insertion race independently of the HTTP worker admission limit.
            create_otter_connection_request(
                &json!({"name":"Concurrent Otter", "publicKey":payload["publicKey"], "password":"test-password-0123"}).to_string(),
                None, None, &devices, &bridge, None).unwrap().0
        })
    }).collect();
    let statuses: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(
        statuses.iter().filter(|&&status| status == 201).count(),
        1,
        "{statuses:?}"
    );
    assert!(
        statuses.iter().all(|status| [201, 429].contains(status)),
        "{statuses:?}"
    );
    assert_eq!(
        f.devices.lock().unwrap().connection_requests.len(),
        OTTER_CONNECTION_PUBLIC_LIMIT
    );
}

#[test]
fn otter_connection_authenticated_pool_remains_bounded() {
    let f = Fixture::new();
    let (id, token) = wall_http::pair(f.port, "Known Otter");
    let key = f.devices.lock().unwrap().devices[&id]
        .public_key
        .clone()
        .unwrap();
    let input = json!({"name":"Known Otter","publicKey":key});
    f.devices.lock().unwrap().connection_authenticated_attempts = vec![unix_time_millis(); 20];
    assert_eq!(
        wall_http::device(
            f.port,
            &token,
            "POST",
            "/v1/otter/connections/request",
            Some(input.clone())
        )
        .0,
        429
    );
    f.devices
        .lock()
        .unwrap()
        .connection_authenticated_attempts
        .clear();
    seed_otter_connection_requests(&f, OTTER_CONNECTION_LIMIT);
    assert_eq!(
        wall_http::device(
            f.port,
            &token,
            "POST",
            "/v1/otter/connections/request",
            Some(input)
        )
        .0,
        429
    );
    assert_eq!(
        f.devices.lock().unwrap().connection_requests.len(),
        OTTER_CONNECTION_LIMIT
    );
}
