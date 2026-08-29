// Capability lifecycle broadcasts are the live-refresh boundary used by Hook.
#[test]
fn capability_install_broadcasts_inventory_update_before_enablement() {
    let root = unique_temp_dir("capability-install-event");
    fs::create_dir_all(&root).expect("control root");
    let key = loom_plugin_security::generate_signing_key("release-1");
    let mut trust = loom_plugin_security::TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    trust
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("trust store");
    let archive = capability_api_fixture(&root, &key);
    let daemon = test_daemon_runtime(&root, None);
    let (events, _subscription) = register_hook_bridge_subscription(
        &daemon.hook_bridge.lock().unwrap().broadcast_hub,
        vec![loom_protocol::HOOK_EVENT_CAPABILITIES_UPDATED.to_owned()],
    );
    let request = ParsedHttpRequest {
        method: "POST".to_owned(),
        path: "/v1/capability-plugins/install".to_owned(),
        headers: Vec::new(),
        body: json!({
            "zipBase64": format!(
                "data:application/zip;base64,{}",
                BASE64.encode(archive)
            )
        })
        .to_string(),
    };

    let (status, response) = route_capability_plugins(
        &request,
        &request.path,
        &root,
        &daemon.capability_runtime,
        &daemon.capability_resources,
        &daemon.hook_bridge,
    )
    .expect("capability route")
    .expect("install response");
    assert_eq!(status, 200, "{response}");
    let event: Value = serde_json::from_str(
        &events
            .recv_timeout(Duration::from_secs(2))
            .expect("capability inventory update"),
    )
    .expect("capability event JSON");
    assert_eq!(
        event["method"],
        loom_protocol::HOOK_EVENT_CAPABILITIES_UPDATED
    );
    assert_eq!(
        daemon
            .capability_runtime
            .contribution_snapshot()
            .unwrap()
            .generation,
        0,
        "install is visible without pretending the disabled plugin contributes"
    );
    fs::remove_dir_all(root).expect("cleanup");
}
