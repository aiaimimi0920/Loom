// Real TLS ingress with two independently signed device identities; no desktop state is used.
#[test]
fn projection_lan_directory_invitation_accept_and_updates() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let root = ProjectionRoot::new();
    let (port, mut server) = start(&root.0);
    let source = Identity::pair(port, "LAN sender");
    let target = Identity::pair(port, "LAN receiver");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let lan_port = listener.local_addr().unwrap().port();
    drop(listener);
    let settings = json!({"enabled":true,"host":"127.0.0.1","port":lan_port});
    let configured = response(http_request(
        port,
        "PUT",
        "/v1/otter/connections/lan",
        Some(&settings.to_string()),
    ));
    assert_eq!(configured.0, 200, "{:?}", configured);
    let info = response(http_request_without_auth(
        port,
        "GET",
        "/v1/otter/identity",
        None,
    ))
    .1;
    let origin = info["endpoint"].as_str().unwrap();
    let certificate = BASE64
        .decode(info["certificate"].as_str().unwrap())
        .unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .add_root_certificate(reqwest::Certificate::from_der(&certificate).unwrap())
        .build()
        .unwrap();
    let post = |identity: &Identity, operation: &str, body: Value| {
        let result = client
            .post(format!("{origin}/v1/projections/{operation}"))
            .header("Authorization", format!("Device {}", identity.token))
            .header("X-Loom-Device-Nonce", Uuid::new_v4().to_string())
            .json(&body)
            .send()
            .unwrap();
        let status = result.status();
        let value: Value = result.json().unwrap();
        assert!(status.is_success(), "{operation}: {status} {value}");
        value
    };
    post(&source, "inbox", json!({"policy":"confirm"}));
    post(&target, "inbox", json!({"policy":"confirm"}));
    assert_eq!(
        post(&source, "targets", json!({}))["targets"][0]["deviceId"],
        target.id
    );
    assert_eq!(
        post(&target, "targets", json!({}))["targets"][0]["deviceId"],
        source.id
    );
    let snapshot = png(55);
    let mut envelope = source.invitation(&snapshot, unix_time_millis() + 270_000);
    envelope.server_origin = origin.into();
    envelope.signature.value = BASE64_URL.encode(
        source
            .key
            .sign(envelope.signature_message().as_bytes())
            .to_bytes(),
    );
    post(
        &source,
        "create",
        json!({"envelope":envelope,"snapshot":snapshot,"targetDeviceId":target.id}),
    );
    assert_eq!(
        post(&target, "inbox", json!({"policy":"confirm"}))["invitations"][0]["envelope"],
        json!(envelope)
    );
    assert_eq!(
        post(&target, "inspect", json!({"envelope":envelope}))["snapshot"],
        json!(snapshot)
    );
    post(&target, "accept", acceptance(&envelope, "received-unit"));
    let changed = png(99);
    thread::sleep(Duration::from_millis(550)); // Preserve the production update rate limit.
    post(&source, "update", update(&envelope, 1, &changed));
    assert_eq!(
        post(
            &target,
            "read",
            json!({"projectionId":envelope.projection_id,"knownRevision":1})
        )["snapshot"],
        json!(changed)
    );
    post(
        &target,
        "unlink",
        json!({"projectionId":envelope.projection_id}),
    );
    server.finish().unwrap();
}
