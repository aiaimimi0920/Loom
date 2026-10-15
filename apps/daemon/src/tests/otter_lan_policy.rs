// Admission passwords, trusted transport origin and approval remain independent.
#[test]
fn otter_lan_policy_password_history_and_shutdown() {
    let f = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let settings = json!({"enabled":true,"host":"127.0.0.1","port":port});
    let (_, view) = wall_http::admin(f.port, "PUT", "/v1/otter/connections/lan", Some(settings));
    let origin = view["endpoint"].as_str().unwrap();
    let packet = {
        let store = f.devices.lock().unwrap();
        store.lan.announcement(&store.connection_info()).unwrap()
    };
    let cert = BASE64
        .decode(packet["certificate"].as_str().unwrap())
        .unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .add_root_certificate(reqwest::Certificate::from_der(&cert).unwrap())
        .build()
        .unwrap();
    let set_policy = |approval: &str| {
        assert_eq!(wall_http::admin(f.port, "PUT", "/v1/otter/connections/policy", Some(json!({
            "name":"Loom", "discoverable":false,"endpoint":null,"approval":approval,"password":"test-password-0123"
        }))).0, 200);
    };
    set_policy("lan");
    let (id, token) = wall_http::pair(f.port, "LAN policy Otter");
    let key = f.devices.lock().unwrap().devices[&id]
        .public_key
        .clone()
        .unwrap();
    let input = json!({"name":"LAN policy Otter","publicKey":key,"password":"test-password-0123"});
    let post = |path: &str, body: Value, authenticated: bool| {
        let mut call = client.post(format!("{origin}{path}")).json(&body);
        if authenticated {
            call = call
                .header("Authorization", format!("Device {token}"))
                .header("X-Loom-Device-Nonce", Uuid::new_v4().to_string());
        }
        let response = call.send().unwrap();
        (
            response.status().as_u16(),
            response.json::<Value>().unwrap(),
        )
    };
    let mut wrong = input.clone();
    wrong["password"] = "wrong".into();
    assert_eq!(
        post("/v1/otter/connections/request", wrong.clone(), false).0,
        403
    );
    let (_, local) = wall_http::public(
        f.port,
        "POST",
        "/v1/otter/connections/request",
        Some(input.clone()),
    );
    assert_eq!(local["connection"]["phase"], "pending");
    let forged = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!(
            "http://127.0.0.1:{}/v1/otter/connections/request",
            f.port
        ))
        .header("X-Loom-Lan-Ingress", "fake")
        .json(&input)
        .send()
        .unwrap();
    assert_eq!(forged.status().as_u16(), 403);
    let (status, approved) = post("/v1/otter/connections/request", input.clone(), true);
    assert_eq!(status, 201);
    assert_eq!(approved["connection"]["phase"], "approved");
    assert!(!f.devices.lock().unwrap().devices[&id].otter_connected);
    let receipt = json!({"requestId":approved["connection"]["id"],"receipt":approved["receipt"]});
    assert_eq!(post("/v1/otter/connections/activate", receipt, true).0, 200);
    {
        let store = f.devices.lock().unwrap();
        assert!(store.devices[&id].otter_connected);
        let saved: DeviceRegistryDocument =
            serde_json::from_slice(&fs::read(&store.path).unwrap()).unwrap();
        assert!(
            saved
                .devices
                .iter()
                .find(|d| d.id == id)
                .unwrap()
                .otter_connected
        );
    }
    set_policy("authenticated");
    assert_eq!(post("/v1/otter/connections/request", wrong, true).0, 403);
    let (_, known) = post("/v1/otter/connections/request", input.clone(), true);
    assert_eq!(known["connection"]["phase"], "approved");
    let (_, unsigned) = post("/v1/otter/connections/request", input.clone(), false);
    assert_eq!(unsigned["connection"]["phase"], "pending");
    set_policy("manual");
    let (_, manual) = post("/v1/otter/connections/request", input, true);
    assert_eq!(manual["connection"]["phase"], "pending");
    assert_eq!(
        wall_http::admin(
            f.port,
            "PUT",
            "/v1/otter/connections/lan",
            Some(json!({"enabled":false,"host":"127.0.0.1","port":port}))
        )
        .0,
        200
    );
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    let store = f.devices.lock().unwrap();
    assert!(!store
        .connection_requests
        .values()
        .any(|request| request.lan));
    assert!(store
        .sessions
        .values()
        .filter(|session| session.device_id == id)
        .all(|session| session.revoked.load(Ordering::SeqCst)));
    assert!(!store.connection_info().discoverable);
    assert!(store.lan.announcement(&store.connection_info()).is_none());
}
