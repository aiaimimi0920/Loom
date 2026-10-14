#[test]
fn otter_connection_discovery_is_opt_in_and_contains_no_password_material() {
    let f = Fixture::new();
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(150)))
        .unwrap();
    let announcer = connection_discovery::start_test_announcer(
        Arc::clone(&f.devices),
        receiver.local_addr().unwrap(),
    )
    .unwrap();
    let mut packet = [0; 2048];
    assert!(receiver.recv(&mut packet).is_err());
    {
        let mut store = f.devices.lock().unwrap();
        let update = serde_json::from_value(json!({"name":"Public Loom","discoverable":true,
            "endpoint":"https://loom.example:443","approval":"password","password":"private-test-password"})).unwrap();
        store.connection_policy = store.connection_policy.prepare_update(update).unwrap();
    }
    receiver
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let size = receiver.recv(&mut packet).unwrap();
    assert!(size <= 1024);
    let info: serde_json::Value = serde_json::from_slice(&packet[..size]).unwrap();
    assert_eq!(info["protocolVersion"], "loom.connection.v1");
    assert_eq!(info["passwordRequired"], true);
    assert_eq!(
        info["loomId"],
        f.devices
            .lock()
            .unwrap()
            .connection_policy
            .public_info()
            .loom_id
    );
    for secret in ["password", "salt", "digest", "token"] {
        assert!(info.get(secret).is_none());
    }
    assert!(!String::from_utf8_lossy(&packet[..size]).contains("private-test-password"));
    drop(announcer);
}

#[test]
fn otter_connection_discovery_shutdown_does_not_wait_for_registry_lock() {
    let f = Fixture::new();
    let registry = f.devices.lock().unwrap();
    let announcer = connection_discovery::Announcer::start(Arc::clone(&f.devices)).unwrap();
    let start = Instant::now();
    drop(announcer);
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(registry);
}
