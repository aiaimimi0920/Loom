mod wall_http {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};

    include!("wall_image_http.rs");
    include!("wall_live_http.rs");
    include!("wall_input_http.rs");
    include!("wall_presentation_http.rs");
    include!("wall_identification_http.rs");
    include!("wall_surface_fixture.rs");
    include!("wall_surface_http.rs");

    struct Root(PathBuf);

    impl Root {
        fn new() -> Self {
            Self(unique_temp_dir(&format!("wall-http-{}", Uuid::new_v4())))
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("clean wall HTTP fixture");
        }
    }

    fn start(root: &Path) -> (u16, ConcurrencyTestFixture) {
        let daemon = LoomDaemon::bind(
            DaemonConfig::localhost(0)
                .with_control_plane_root(root)
                .with_bounded_request_executor(4, 16),
        )
        .expect("bind wall daemon");
        let port = daemon.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        let server = thread::spawn(move || daemon.serve_until(rx));
        (port, ConcurrencyTestFixture::new(tx, server))
    }

    fn response(raw: String) -> (u16, Value) {
        let status = raw.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, response_json_body(&raw))
    }

    fn admin(port: u16, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        response(http_request(
            port,
            method,
            path,
            body.as_ref().map(Value::to_string).as_deref(),
        ))
    }

    fn public(port: u16, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        response(http_request_without_auth(
            port,
            method,
            path,
            body.as_ref().map(Value::to_string).as_deref(),
        ))
    }

    fn device(
        port: u16,
        token: &str,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let headers = format!(
            "Authorization: Device {token}\r\nX-Loom-Device-Nonce: {}\r\n",
            Uuid::new_v4()
        );
        response(http_request_with_extra_headers(
            port,
            method,
            path,
            body.as_ref().map(Value::to_string).as_deref(),
            &headers,
        ))
    }

    fn pair(port: u16, name: &str) -> (String, String) {
        let key = SigningKey::generate(&mut OsRng);
        let (status, pending) = public(
            port,
            "POST",
            "/v1/devices/requests",
            Some(json!({
                "name": name, "kind": "computer", "address": "127.0.0.1",
                "publicKey": BASE64.encode(key.verifying_key().to_bytes()),
            })),
        );
        assert_eq!(status, 200);
        let id = pending["pending"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            admin(
                port,
                "POST",
                &format!("/v1/devices/{id}/approve"),
                Some(json!({}))
            )
            .0,
            200
        );
        let (status, challenge) = public(
            port,
            "POST",
            "/v1/device-sessions/challenges",
            Some(json!({"deviceId": id})),
        );
        assert_eq!(status, 201);
        let nonce = Uuid::new_v4().to_string();
        let message = device_session_signature_message(
            &id,
            challenge["challengeId"].as_str().unwrap(),
            challenge["challenge"].as_str().unwrap(),
            &nonce,
        );
        let (status, session) = public(
            port,
            "POST",
            "/v1/device-sessions",
            Some(json!({
                "deviceId": id, "challengeId": challenge["challengeId"], "clientNonce": nonce,
                "signature": BASE64.encode(key.sign(message.as_bytes()).to_bytes()),
            })),
        );
        assert_eq!(status, 201);
        (id, session["token"].as_str().unwrap().to_owned())
    }

    #[test]
    fn wall_http_authentication_mapping_registration_and_restart_are_real_daemon_paths() {
        let root = Root::new();
        let (port, mut server) = start(&root.0);
        assert_eq!(public(port, "GET", "/v1/walls/state", None).0, 401);
        let (a, token_a) = pair(port, "Tile A");
        let (b, token_b) = pair(port, "Tile B");
        let (_, token_outsider) = pair(port, "Unassigned tile");
        let mut data: Value = serde_json::from_str(include_str!(
            "../../../../protocol/fixtures/wall-geometry.v1.json"
        ))
        .unwrap();
        data["endpoints"][0]["deviceId"] = json!(a);
        data["endpoints"][1]["deviceId"] = json!(b);
        let registration_a = json!({"baseRevision": 0, "endpoint": data["endpoints"][0]});
        assert_eq!(
            device(
                port,
                &token_a,
                "POST",
                "/v1/walls/endpoints/register",
                Some(registration_a)
            )
            .0,
            200
        );
        let registration_b = json!({"baseRevision": 1, "endpoint": data["endpoints"][1]});
        assert_eq!(
            device(
                port,
                &token_a,
                "POST",
                "/v1/walls/endpoints/register",
                Some(registration_b.clone())
            )
            .0,
            403
        );
        assert_eq!(
            device(
                port,
                &token_b,
                "POST",
                "/v1/walls/endpoints/register",
                Some(registration_b)
            )
            .0,
            200
        );
        let mut unknown = data["endpoints"][0].clone();
        unknown["deviceId"] = json!("unapproved-device");
        assert_eq!(
            admin(
                port,
                "POST",
                "/v1/walls/endpoints/register",
                Some(json!({"baseRevision": 2, "endpoint": unknown}))
            )
            .0,
            403
        );
        data["layout"]["revision"] = json!(3);
        let update = json!({"baseRevision": 2, "layout": data["layout"]});
        assert_eq!(
            device(
                port,
                &token_a,
                "PUT",
                "/v1/walls/layouts",
                Some(update.clone())
            )
            .0,
            403
        );
        assert_eq!(
            admin(port, "PUT", "/v1/walls/layouts", Some(update.clone())).0,
            200
        );
        assert_eq!(admin(port, "PUT", "/v1/walls/layouts", Some(update)).0, 409);
        let (status, own) = device(port, &token_a, "GET", "/v1/walls/state", None);
        assert_eq!(status, 200);
        assert_eq!(own["endpoints"].as_array().unwrap().len(), 1);
        assert_eq!(own["layouts"].as_array().unwrap().len(), 1);
        let (_, outsider) = device(port, &token_outsider, "GET", "/v1/walls/state", None);
        assert!(outsider["endpoints"].as_array().unwrap().is_empty());
        assert!(outsider["layouts"].as_array().unwrap().is_empty());
        assert_eq!(
            device(
                port,
                &token_a,
                "POST",
                "/v1/walls/endpoints/remove",
                Some(json!({"baseRevision": 3, "endpointId": "endpoint-right"}))
            )
            .0,
            403
        );
        assert_eq!(
            device(
                port,
                &token_a,
                "POST",
                "/v1/walls/layouts/remove",
                Some(json!({"baseRevision": 3, "wallId": "living-room"}))
            )
            .0,
            403
        );
        let connect = json!({"endpointId": "endpoint-left"});
        assert_eq!(
            admin(port, "POST", "/v1/walls/connect", Some(connect.clone())).0,
            401
        );
        assert_eq!(
            device(
                port,
                &token_b,
                "POST",
                "/v1/walls/connect",
                Some(connect.clone())
            )
            .0,
            403
        );
        let (status, lease) = device(port, &token_a, "POST", "/v1/walls/connect", Some(connect));
        assert_eq!(status, 200);
        let beat = json!({"endpointId": "endpoint-left", "leaseId": lease["leaseId"], "sequence": 1, "appliedRevision": 3});
        assert_eq!(
            device(
                port,
                &token_a,
                "POST",
                "/v1/walls/heartbeat",
                Some(beat.clone())
            )
            .0,
            200
        );
        assert_eq!(
            device(
                port,
                &token_a,
                "POST",
                "/v1/walls/heartbeat",
                Some(beat.clone())
            )
            .0,
            409
        );
        let (_, state) = admin(port, "GET", "/v1/walls/state", None);
        assert_eq!(state["endpoints"][0]["online"], true);
        assert_eq!(state["endpoints"][0]["appliedRevision"], 3);
        assert!(!state
            .to_string()
            .contains(lease["leaseId"].as_str().unwrap()));
        assert_eq!(
            admin(
                port,
                "PUT",
                &format!("/v1/devices/{a}"),
                Some(json!({
                    "name": "Tile A", "kind": "computer", "address": "127.0.0.1", "enabled": false,
                }))
            )
            .0,
            200
        );
        assert_eq!(
            device(port, &token_a, "POST", "/v1/walls/heartbeat", Some(beat)).0,
            401
        );
        assert_eq!(
            admin(port, "GET", "/v1/walls/state", None).1["endpoints"][0]["online"],
            false
        );
        server.finish().unwrap();

        let (port, mut restarted) = start(&root.0);
        let (status, restored) = admin(port, "GET", "/v1/walls/state", None);
        assert_eq!(status, 200);
        assert_eq!(restored["revision"], 3);
        let restored_layout: loom_protocol::wall::WallLayout =
            serde_json::from_value(restored["layouts"][0].clone()).unwrap();
        let expected_layout: loom_protocol::wall::WallLayout =
            serde_json::from_value(data["layout"].clone()).unwrap();
        assert_eq!(restored_layout, expected_layout);
        assert!(restored["endpoints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["online"] == false));
        assert_eq!(
            device(port, &token_b, "GET", "/v1/walls/state", None).0,
            401
        );
        assert_eq!(
            admin(
                port,
                "POST",
                "/v1/walls/layouts/remove",
                Some(json!({"baseRevision": 3, "wallId": "living-room"}))
            )
            .0,
            200
        );
        assert_eq!(
            admin(
                port,
                "POST",
                "/v1/walls/endpoints/remove",
                Some(json!({"baseRevision": 4, "endpointId": "endpoint-left"}))
            )
            .0,
            200
        );
        restarted.finish().unwrap();
    }

    #[test]
    fn wall_http_rejects_unknown_payload_fields_and_oversized_bodies() {
        let root = Root::new();
        let (port, mut server) = start(&root.0);
        assert_eq!(
            admin(
                port,
                "POST",
                "/v1/walls/connect",
                Some(json!({"endpointId":"x", "unknown":true}))
            )
            .0,
            400
        );
        let oversized = " ".repeat(512 * 1024 + 1);
        let raw = http_request(
            port,
            "POST",
            "/v1/walls/endpoints/register",
            Some(&oversized),
        );
        assert_eq!(response(raw).0, 413);
        assert_eq!(admin(port, "GET", "/v1/walls/state", None).1["revision"], 0);
        server.finish().unwrap();
    }
}
