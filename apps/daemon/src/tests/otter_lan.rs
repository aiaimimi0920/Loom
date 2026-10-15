// Real HTTPS, certificate rejection, public approval and signed device activation.
#[test]
fn otter_lan_rejected_settings_return_actionable_errors() {
    let f = Fixture::new();
    for host in ["not-an-ip", "198.18.0.1"] {
        let (status, body) = wall_http::admin(
            f.port,
            "PUT",
            "/v1/otter/connections/lan",
            Some(json!({"enabled":true,"host":host,"port":8766})),
        );
        assert_eq!(status, 400, "{body}");
        assert!(body["error"]["message"].as_str().unwrap().contains("IPv4"));
    }
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();
    let (status, body) = wall_http::admin(
        f.port,
        "PUT",
        "/v1/otter/connections/lan",
        Some(json!({"enabled":true,"host":"127.0.0.1","port":port})),
    );
    assert_eq!(status, 400, "{body}");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("端口是否被占用"));
    let (_, view) = wall_http::admin(f.port, "GET", "/v1/otter/connections/lan", None);
    assert_eq!(view["listening"], false);
    assert_eq!(view["enabled"], false);
}

#[test]
fn otter_lan_tls_pairing_approval_activation_and_shutdown() {
    use ed25519_dalek::{Signer, SigningKey};
    let f = Fixture::new();
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert_eq!(
        wall_http::public(f.port, "GET", "/v1/otter/connections/lan", None).0,
        401
    );
    let settings = json!({"enabled":true,"host":"127.0.0.1","port":port});
    let (status, view) = wall_http::admin(
        f.port,
        "PUT",
        "/v1/otter/connections/lan",
        Some(settings.clone()),
    );
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["listening"], true);
    let origin = view["endpoint"].as_str().unwrap();
    let pair = {
        let store = f.devices.lock().unwrap();
        store.lan.announcement(&store.connection_info()).unwrap()
    };
    let cert = BASE64
        .decode(pair["certificate"].as_str().unwrap())
        .unwrap();
    let untrusted = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert!(untrusted
        .get(format!("{origin}/v1/otter/identity"))
        .send()
        .is_err());
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .add_root_certificate(reqwest::Certificate::from_der(&cert).unwrap())
        .build()
        .unwrap();
    let info: Value = client
        .get(format!("{origin}/v1/otter/identity"))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(info["loomId"], pair["loomId"]);
    assert_eq!(info["endpoint"], origin);
    assert!(!client
        .get(format!("{origin}/status"))
        .send()
        .unwrap()
        .status()
        .is_success());
    assert!(!client
        .get(format!("{origin}/v1/otter/identity"))
        .bearer_auth("admin-not-forwarded")
        .send()
        .unwrap()
        .status()
        .is_success());
    let key = SigningKey::generate(&mut OsRng);
    let post = |path: &str, body: Value| -> Value {
        let response = client
            .post(format!("{origin}{path}"))
            .json(&body)
            .send()
            .unwrap();
        let status = response.status();
        let value: Value = response.json().unwrap();
        assert!(status.is_success(), "{status}: {value}");
        value
    };
    let pending = post(
        "/v1/otter/connections/request",
        json!({"name":"LAN Otter", "publicKey": BASE64.encode(key.verifying_key().as_bytes())}),
    );
    assert_eq!(pending["connection"]["phase"], "pending");
    let id = pending["connection"]["deviceId"].as_str().unwrap();
    assert_eq!(
        wall_http::admin(
            f.port,
            "POST",
            "/v1/otter/connections/decision",
            Some(json!({"requestId":pending["connection"]["id"], "approve":true}))
        )
        .0,
        200
    );
    let challenge = post("/v1/device-sessions/challenges", json!({"deviceId":id}));
    let nonce = Uuid::new_v4().to_string();
    let message = device_session_signature_message(
        id,
        challenge["challengeId"].as_str().unwrap(),
        challenge["challenge"].as_str().unwrap(),
        &nonce,
    );
    let session = post(
        "/v1/device-sessions",
        json!({"deviceId":id,"challengeId":challenge["challengeId"],"clientNonce":nonce,"signature":BASE64.encode(key.sign(message.as_bytes()).to_bytes())}),
    );
    let response = client
        .post(format!("{origin}/v1/otter/connections/activate"))
        .header(
            "Authorization",
            format!("Device {}", session["token"].as_str().unwrap()),
        )
        .header("X-Loom-Device-Nonce", Uuid::new_v4().to_string())
        .json(&json!({"requestId":pending["connection"]["id"],"receipt":pending["receipt"]}))
        .send()
        .unwrap();
    assert!(response.status().is_success());
    let active: Value = response.json().unwrap();
    assert_eq!(active["connection"]["phase"], "connected");
    let (status, stopped) = wall_http::admin(
        f.port,
        "PUT",
        "/v1/otter/connections/lan",
        Some(json!({"enabled":false,"host":"127.0.0.1","port":port})),
    );
    assert_eq!(status, 200);
    assert_eq!(stopped["listening"], false);
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    let (_, restarted) =
        wall_http::admin(f.port, "PUT", "/v1/otter/connections/lan", Some(settings));
    assert_eq!(restarted["endpoint"], view["endpoint"]);
    let after = {
        let store = f.devices.lock().unwrap();
        store.lan.announcement(&store.connection_info()).unwrap()
    };
    assert_eq!(after["certificate"], pair["certificate"]);
}
