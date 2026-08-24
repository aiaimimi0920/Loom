// Loom daemon tests fragment 3; included into the shared crate test module.
#[test]
fn device_pairing_issues_short_lived_session_rejects_replay_and_revokes_on_disable() {
    use ed25519_dalek::{Signer as _, SigningKey};

    let root = unique_temp_dir("device-session-security");
    let runtime = test_daemon_runtime_from_config(
        &root,
        DaemonConfig::localhost(0).with_bearer_token("loom-admin-test"),
    );
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
    let pairing_body = json!({
        "name": "Paired Hook",
        "kind": "computer",
        "address": "192.168.10.20",
        "publicKey": public_key,
    })
    .to_string();
    let (status, response) = route_with_runtime(
        &runtime,
        &parsed_request("POST", "/v1/devices/requests", &[], Some(&pairing_body)),
    )
    .expect("create public pairing request");
    assert_eq!(status, 200, "{response}");
    let device_id = runtime
        .device_registry
        .lock()
        .expect("device registry")
        .devices
        .values()
        .find(|device| device.name == "Paired Hook")
        .expect("pending paired device")
        .id
        .clone();

    let challenge_body = json!({"deviceId": device_id}).to_string();
    let (status, _) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            "/v1/device-sessions/challenges",
            &[],
            Some(&challenge_body),
        ),
    )
    .expect("reject challenge before approval");
    assert_eq!(status, 403);

    let approve_path = format!("/v1/devices/{device_id}/approve");
    let (status, response) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &approve_path,
            &[("Authorization", "Bearer loom-admin-test")],
            None,
        ),
    )
    .expect("approve paired device");
    assert_eq!(status, 200, "{response}");
    let (status, challenge_json) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            "/v1/device-sessions/challenges",
            &[],
            Some(&challenge_body),
        ),
    )
    .expect("create approved device challenge");
    assert_eq!(status, 201, "{challenge_json}");
    let challenge: Value = serde_json::from_str(&challenge_json).expect("challenge JSON");
    let challenge_id = challenge["challengeId"].as_str().expect("challenge id");
    let challenge_value = challenge["challenge"].as_str().expect("challenge value");
    let client_nonce = "client_nonce_0000000000000001";
    let signature_message =
        device_session_signature_message(&device_id, challenge_id, challenge_value, client_nonce);
    let signature = BASE64.encode(signing_key.sign(signature_message.as_bytes()).to_bytes());
    let issue_body = json!({
        "deviceId": device_id,
        "challengeId": challenge_id,
        "clientNonce": client_nonce,
        "signature": signature,
    })
    .to_string();
    let (status, session_json) = route_with_runtime(
        &runtime,
        &parsed_request("POST", "/v1/device-sessions", &[], Some(&issue_body)),
    )
    .expect("issue signed device session");
    assert_eq!(status, 201, "{session_json}");
    let session: Value = serde_json::from_str(&session_json).expect("session JSON");
    let token = session["token"].as_str().expect("device session token");
    let authorization = format!("Device {token}");
    let nonce = "request_nonce_000000000000001";
    let (status, response) = route_with_runtime(
        &runtime,
        &parsed_request(
            "GET",
            "/v1/capabilities",
            &[
                ("Authorization", authorization.as_str()),
                ("X-Loom-Device-Nonce", nonce),
            ],
            None,
        ),
    )
    .expect("use device session");
    assert_eq!(status, 200, "{response}");
    let (status, replay) = route_with_runtime(
        &runtime,
        &parsed_request(
            "GET",
            "/v1/capabilities",
            &[
                ("Authorization", authorization.as_str()),
                ("X-Loom-Device-Nonce", nonce),
            ],
            None,
        ),
    )
    .expect("reject replayed device nonce");
    assert_eq!(status, 409, "{replay}");
    let resource_payload = b"remote Surface resource";
    let resource_lease = runtime
        .surface_resources
        .lock()
        .expect("Surface resource store")
        .register(
            SurfaceResourceKind::Binary,
            "application/octet-stream",
            resource_payload,
            None,
            None,
            None,
        )
        .expect("register remote Surface resource");
    let resource_digest = resource_lease
        .resource
        .resource_id
        .trim_start_matches("sha256:");
    match route_request(
        &runtime,
        &parsed_request(
            "GET",
            &format!("/v1/surfaces/resources/{resource_digest}"),
            &[
                ("Authorization", authorization.as_str()),
                ("X-Loom-Device-Nonce", "request_nonce_000000000000002"),
                ("X-Loom-Surface-Lease", resource_lease.lease_id.as_str()),
            ],
            None,
        ),
    ) {
        RouteResponse::Binary {
            status,
            content_type,
            body,
        } => {
            assert_eq!(status, 200);
            assert_eq!(content_type, "application/octet-stream");
            assert_eq!(body, resource_payload);
        }
        RouteResponse::Text { status, body }
        | RouteResponse::TextWithHeaders { status, body, .. } => {
            panic!("expected device-authenticated Surface resource, got {status}: {body}")
        }
    }
    let (status, denied) = route_with_runtime(
        &runtime,
        &parsed_request(
            "GET",
            "/v1/devices",
            &[
                ("Authorization", authorization.as_str()),
                ("X-Loom-Device-Nonce", "request_nonce_000000000000003"),
            ],
            None,
        ),
    )
    .expect("deny device session access to administrator routes");
    assert_eq!(status, 403, "{denied}");

    let update_path = format!("/v1/devices/{device_id}");
    let update_body = json!({
        "name": "Paired Hook",
        "kind": "computer",
        "address": "192.168.10.20",
        "enabled": false,
    })
    .to_string();
    let (status, response) = route_with_runtime(
        &runtime,
        &parsed_request(
            "PUT",
            &update_path,
            &[("Authorization", "Bearer loom-admin-test")],
            Some(&update_body),
        ),
    )
    .expect("disable paired device");
    assert_eq!(status, 200, "{response}");
    let (status, revoked) = route_with_runtime(
        &runtime,
        &parsed_request(
            "GET",
            "/v1/capabilities",
            &[
                ("Authorization", authorization.as_str()),
                ("X-Loom-Device-Nonce", "request_nonce_000000000000004"),
            ],
            None,
        ),
    )
    .expect("reject revoked device session");
    assert_eq!(status, 401, "{revoked}");
    assert!(!session_json.contains(&sha256_bytes(token.as_bytes())));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stale_device_credential_does_not_mask_a_valid_administrator_bearer() {
    let root = unique_temp_dir("stale-device-credential");
    let runtime = test_daemon_runtime(&root, Some("loom-admin-test"));
    let stale_device_headers = [
        ("Authorization", "Bearer loom-admin-test"),
        ("Authorization", "Device loom_device_session_stale_token"),
        ("X-Loom-Device-Nonce", "request_nonce_000000000000010"),
    ];
    // A desktop client that was re-paired keeps sending its previous device credential, so the
    // same request legitimately carries both headers. The administrator bearer must win.
    let (status, response) = route_with_runtime(
        &runtime,
        &parsed_request("GET", "/v1/devices", &stale_device_headers, None),
    )
    .expect("administrator bearer outranks a stale device credential");
    assert_eq!(status, 200, "{response}");

    // Without the administrator bearer the stale credential is still reported in full rather
    // than being silently downgraded to an anonymous request.
    let (status, rejected) = route_with_runtime(
        &runtime,
        &parsed_request("GET", "/v1/devices", &stale_device_headers[1..], None),
    )
    .expect("reject the stale device credential on its own");
    assert_eq!(status, 401, "{rejected}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn surface_stream_history_is_ordered_bounded_and_reports_cursor_reset() {
    let hub = HookBridgeBroadcastHub::new();
    broadcast_hook_bridge_messages(
        &hub,
        &[
            json!({"method": SURFACE_EVENT_PATCH, "params": {"revision": 1}}).to_string(),
            json!({"method": loom_protocol::SURFACE_EVENT_RESULT, "params": {"resultRevision": 1}})
                .to_string(),
        ],
    );
    let (next, reset, entries) = hub.wait_after(0, Duration::ZERO);
    assert!(!reset);
    assert_eq!(entries.len(), 2);
    assert_eq!(next, entries[1].sequence);
    assert!(entries[0].sequence < entries[1].sequence);

    for revision in 0..=HOOK_BRIDGE_HISTORY_CAPACITY {
        broadcast_hook_bridge_messages(
            &hub,
            &[json!({
                "method": SURFACE_EVENT_PATCH,
                "params": {"revision": revision}
            })
            .to_string()],
        );
    }
    let (_, reset, entries) = hub.wait_after(1, Duration::ZERO);
    assert!(reset);
    assert_eq!(entries.len(), HOOK_BRIDGE_POLL_MAX_MESSAGES);
    assert!(entries[0].sequence > 1);
}

#[test]
fn initial_surface_recovery_advances_without_skipping_the_first_broadcast() {
    let root = unique_temp_dir("surface-stream-recovery-cursor");
    let surface_instances = Arc::new(Mutex::new(
        SurfaceInstanceStore::new(root.join("instances.json")).expect("Surface store"),
    ));
    let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));

    let (_, initial_body) = poll_surface_stream(
        "/v1/surfaces/stream?after=0&timeoutMs=0",
        &hook_bridge,
        &surface_instances,
        None,
    )
    .expect("initial recovery poll");
    let initial: Value = serde_json::from_str(&initial_body).expect("initial stream JSON");
    assert_eq!(initial["next"], HOOK_BRIDGE_RECOVERY_CURSOR);
    assert!(initial["messages"].as_array().is_some_and(Vec::is_empty));

    let hub = hook_bridge
        .lock()
        .expect("Hook bridge")
        .broadcast_hub
        .clone();
    broadcast_hook_bridge_messages(
        &hub,
        &[json!({"method": SURFACE_EVENT_PATCH, "params": {"revision": 1}}).to_string()],
    );
    let (_, next_body) = poll_surface_stream(
        &format!("/v1/surfaces/stream?after={HOOK_BRIDGE_RECOVERY_CURSOR}&timeoutMs=0"),
        &hook_bridge,
        &surface_instances,
        None,
    )
    .expect("post-recovery poll");
    let next: Value = serde_json::from_str(&next_body).expect("next stream JSON");
    assert_eq!(next["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        next["messages"][0]["params"]["revision"], 1,
        "the reserved recovery cursor must not skip the first real broadcast"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn surface_stream_isolated_by_authenticated_attachment_device() {
    let root = unique_temp_dir("surface-stream-device-isolation");
    let surface_instances = Arc::new(Mutex::new(
        SurfaceInstanceStore::new(root.join("instances.json")).expect("Surface store"),
    ));
    let (instance_id, attachment_a, attachment_b) = {
        let mut store = surface_instances.lock().expect("Surface store");
        let instance = store
            .create(
                "surface-stream-fixture",
                "1.0.0",
                &"a".repeat(64),
                1,
                loom_protocol::SurfaceInstancePersistence::Persistent,
                loom_protocol::SurfaceInstanceMode::Independent,
            )
            .expect("create instance");
        let attachment_a = store
            .attach(
                &instance.descriptor.instance_id,
                "hook-node:a",
                "device-a",
                None,
            )
            .expect("attach device A");
        let attachment_b = store
            .attach(
                &instance.descriptor.instance_id,
                "hook-node:b",
                "device-b",
                None,
            )
            .expect("attach device B");
        (
            instance.descriptor.instance_id,
            attachment_a.descriptor.attachment_id,
            attachment_b.descriptor.attachment_id,
        )
    };
    let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
    let hub = hook_bridge
        .lock()
        .expect("Hook bridge")
        .broadcast_hub
        .clone();
    broadcast_hook_bridge_messages(
        &hub,
        &[
            json!({
                "method": SURFACE_EVENT_PATCH,
                "params": {
                    "hookNodeId": "hook-node:a",
                    "patch": {
                        "instanceId": instance_id,
                        "attachmentId": attachment_a,
                    }
                }
            })
            .to_string(),
            json!({
                "method": SURFACE_EVENT_PATCH,
                "params": {
                    "hookNodeId": "hook-node:b",
                    "patch": {
                        "instanceId": instance_id,
                        "attachmentId": attachment_b,
                    }
                }
            })
            .to_string(),
        ],
    );

    let (status, body) = poll_surface_stream(
        "/v1/surfaces/stream?after=0&timeoutMs=0",
        &hook_bridge,
        &surface_instances,
        Some("device-a"),
    )
    .expect("poll device A stream");
    assert_eq!(status, 200, "{body}");
    let response: Value = serde_json::from_str(&body).expect("stream JSON");
    // Hook carries its own copy of this string and rejects an envelope without it, so the wire
    // value is spelled out here rather than read from the constant: changing it has to break a
    // test in this repository, not only in Hook's.
    assert_eq!(response["protocolVersion"], "loom.surface-stream.v1");
    assert_eq!(response["next"], 3);
    assert_eq!(response["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        response["messages"][0]["params"]["patch"]["attachmentId"],
        attachment_a
    );
    assert!(!body.contains(&attachment_b), "{body}");
    let _ = fs::remove_dir_all(root);
}
