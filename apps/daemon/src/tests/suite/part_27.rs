// Loom daemon tests fragment 27; included into the shared crate test module.
#[test]
fn surface_instance_routes_keep_preview_and_formal_results_separate() {
    let root = unique_temp_dir("surface-instance-routes");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let instance = runtime
        .surface_instances
        .lock()
        .expect("lock Surface store")
        .create(
            "neuro.official/stock-price",
            "1.0.0",
            &"a".repeat(64),
            1,
            SurfaceInstancePersistence::Persistent,
            SurfaceInstanceMode::Independent,
        )
        .expect("create Surface instance");
    let instance_id = instance.descriptor.instance_id;

    let (status, body) = route_with_runtime(
        &runtime,
        &parsed_request("GET", "/v1/surfaces/instances", &[], None),
    )
    .expect("list Surface instances");
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).expect("list JSON");
    assert_eq!(
        body["instances"][0]["descriptor"]["instanceId"],
        instance_id
    );

    let attach_path = format!("/v1/surfaces/instances/{instance_id}/attachments");
    let attach_body = json!({
        "hookNodeId": "hook-node:stock",
        "deviceId": "device-000-local",
    })
    .to_string();
    let (status, body) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &attach_path, &[], Some(&attach_body)),
    )
    .expect("attach Surface instance");
    assert_eq!(status, 201);
    let attachment: Value = serde_json::from_str(&body).expect("attachment JSON");
    let attachment_id = attachment["descriptor"]["attachmentId"]
        .as_str()
        .expect("attachment id")
        .to_owned();
    let (surface_rx, _surface_subscription) = register_hook_bridge_subscription(
        &runtime
            .hook_bridge
            .lock()
            .expect("lock hook bridge")
            .broadcast_hub,
        loom_protocol::SURFACE_EVENT_METHODS
            .iter()
            .map(|method| (*method).to_owned())
            .collect(),
    );

    let snapshot_path = format!("/v1/surfaces/instances/{instance_id}/snapshot");
    let snapshot_body = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "instanceId": instance_id,
        "attachmentId": attachment_id,
        "artId": "neuro.official/stock-price",
        "artVersion": "1.0.0",
        "revision": 1,
        "scene": {
            "id": "root",
            "type": "text",
            "props": { "text": "100" },
            "events": { "click": "refresh" }
        },
        "authoritativeState": { "price": 100 },
        "resources": [],
    })
    .to_string();
    let (status, patch_response) = route_with_runtime(
        &runtime,
        &parsed_request("PUT", &snapshot_path, &[], Some(&snapshot_body)),
    )
    .expect("put Surface snapshot");
    assert_eq!(status, 200, "{patch_response}");
    let snapshot_push: Value = serde_json::from_str(
        &surface_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Surface snapshot push"),
    )
    .expect("Surface snapshot push JSON");
    assert_eq!(snapshot_push["method"], SURFACE_EVENT_SNAPSHOT);
    assert_eq!(snapshot_push["params"]["hookNodeId"], "hook-node:stock");
    let mounted_push: Value = serde_json::from_str(
        &surface_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Surface mounted lifecycle push"),
    )
    .expect("Surface mounted lifecycle JSON");
    assert_eq!(mounted_push["method"], SURFACE_EVENT_LIFECYCLE);
    assert_eq!(mounted_push["params"]["event"]["state"], "mounted");

    let lifecycle_path = format!("/v1/surfaces/instances/{instance_id}/lifecycle");
    let lifecycle_body = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "instanceId": instance_id,
        "attachmentId": attachment_id,
        "state": "active",
        "revision": 2,
    })
    .to_string();
    let (status, lifecycle_response) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &lifecycle_path, &[], Some(&lifecycle_body)),
    )
    .expect("activate Surface attachment");
    assert_eq!(status, 200, "{lifecycle_response}");
    let lifecycle_push: Value = serde_json::from_str(
        &surface_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Surface lifecycle push"),
    )
    .expect("Surface lifecycle JSON");
    assert_eq!(lifecycle_push["method"], SURFACE_EVENT_LIFECYCLE);
    assert_eq!(lifecycle_push["params"]["event"]["state"], "active");

    let patch_path = format!("/v1/surfaces/instances/{instance_id}/patch");
    let patch_body = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "instanceId": instance_id,
        "attachmentId": attachment_id,
        "baseRevision": 1,
        "revision": 2,
        "operations": [{
            "op": "set",
            "nodeId": "root",
            "path": "/props/text",
            "value": "101"
        }]
    })
    .to_string();
    let (status, patch_response) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &patch_path, &[], Some(&patch_body)),
    )
    .expect("apply Surface patch");
    assert_eq!(status, 200, "{patch_response}");
    let patch_push: Value = serde_json::from_str(
        &surface_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Surface patch push"),
    )
    .expect("Surface patch push JSON");
    assert_eq!(patch_push["method"], SURFACE_EVENT_PATCH);
    assert_eq!(patch_push["params"]["patch"]["revision"], 2);

    let generation_path = format!("/v1/surfaces/instances/{instance_id}/generation");
    let (status, body) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &generation_path, &[], Some("{}")),
    )
    .expect("begin Surface generation");
    assert_eq!(status, 200);
    let generation: Value = serde_json::from_str(&body).expect("generation JSON");
    assert_eq!(generation["generation"], 1);
    let generation_push: Value = serde_json::from_str(
        &surface_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Surface generation push"),
    )
    .expect("Surface generation push JSON");
    assert_eq!(generation_push["method"], SURFACE_EVENT_GENERATION);
    assert_eq!(generation_push["params"]["generation"], 1);

    let preview_path = format!("/v1/surfaces/instances/{instance_id}/preview");
    let preview_body = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "instanceId": instance_id,
        "requestId": "request:stock-1",
        "generation": 1,
        "previewRevision": 1,
        "portId": "preview",
        "value": { "kind": "value", "value": { "price": 101 } },
    })
    .to_string();
    let (status, body) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &preview_path, &[], Some(&preview_body)),
    )
    .expect("commit Surface preview");
    assert_eq!(status, 200);
    let preview_record: Value = serde_json::from_str(&body).expect("preview record JSON");
    assert!(preview_record["latestResult"].is_null());
    assert_eq!(preview_record["latestPreview"]["previewRevision"], 1);

    let result_path = format!("/v1/surfaces/instances/{instance_id}/result");
    let result_body = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "instanceId": instance_id,
        "requestId": "request:stock-1",
        "generation": 1,
        "resultRevision": 1,
        "outputs": {
            "output": { "kind": "value", "value": { "price": 101, "currency": "CNY" } }
        },
        "statePatch": { "price": 101 },
    })
    .to_string();
    let (status, body) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &result_path, &[], Some(&result_body)),
    )
    .expect("commit Surface result");
    assert_eq!(status, 200);
    let formal_record: Value = serde_json::from_str(&body).expect("formal record JSON");
    assert_eq!(formal_record["latestPreview"]["portId"], "preview");
    assert_eq!(
        formal_record["latestResult"]["outputs"]["output"]["value"]["currency"],
        "CNY"
    );

    let (status, body) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &result_path, &[], Some(&result_body)),
    )
    .expect("reject duplicate formal result");
    assert_eq!(status, 409);
    let conflict: Value = serde_json::from_str(&body).expect("conflict JSON");
    assert_eq!(conflict["error"]["code"], "surface_conflict");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn surface_resource_route_is_content_addressed_and_returns_original_bytes() {
    let root = unique_temp_dir("surface-resource-route");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let bytes = b"surface-resource-fixture";
    let body = json!({
        "kind": "binary",
        "mime": "application/octet-stream",
        "dataBase64": BASE64.encode(bytes),
        "leaseMillis": 60_000,
    })
    .to_string();
    let (status, lease) = route_with_runtime(
        &runtime,
        &parsed_request("POST", "/v1/surfaces/resources", &[], Some(&body)),
    )
    .expect("register Surface resource");
    assert_eq!(status, 201, "{lease}");
    let lease: Value = serde_json::from_str(&lease).expect("resource lease JSON");
    let resource_id = lease["resource"]["resourceId"]
        .as_str()
        .expect("resource id");
    assert!(resource_id.starts_with("sha256:"));
    let digest = resource_id.trim_start_matches("sha256:");
    let lease_id = lease["leaseId"].as_str().expect("lease id");
    match route_request(
        &runtime,
        &parsed_request(
            "GET",
            &format!("/v1/surfaces/resources/{digest}"),
            &[("X-Loom-Surface-Lease", lease_id)],
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
            assert_eq!(body, bytes);
        }
        RouteResponse::Text { status, body }
        | RouteResponse::TextWithHeaders { status, body, .. } => {
            panic!("expected resource bytes, got {status}: {body}")
        }
    }
    let (status, _) = route_with_runtime(
        &runtime,
        &parsed_request(
            "DELETE",
            &format!("/v1/surfaces/resource-leases/{lease_id}"),
            &[],
            None,
        ),
    )
    .expect("release Surface resource lease");
    assert_eq!(status, 204);

    let shared_body = json!({
        "kind": "image",
        "mime": "image/png",
        "dataBase64": BASE64.encode(test_png_bytes()),
        "leaseMillis": 60_000,
        "preferredTransport": "shared_memory",
    })
    .to_string();
    let (status, shared_lease) = route_with_runtime(
        &runtime,
        &parsed_request("POST", "/v1/surfaces/resources", &[], Some(&shared_body)),
    )
    .expect("register shared-memory Surface resource");
    assert_eq!(status, 201, "{shared_lease}");
    let shared_lease: Value =
        serde_json::from_str(&shared_lease).expect("shared resource lease JSON");
    assert_eq!(shared_lease["transport"]["kind"], "shared_memory");
    assert_eq!(
        shared_lease["resource"]["mime"],
        "application/x-neuro-rgba8"
    );
    assert!(shared_lease["transport"]["handle"]
        .as_str()
        .is_some_and(|handle| handle.starts_with("Loom_Buffer_")));
    assert_eq!(
        runtime
            .shared_images
            .lock()
            .expect("shared images")
            .list()
            .len(),
        1
    );
    let shared_lease_id = shared_lease["leaseId"].as_str().expect("shared lease id");
    let (status, _) = route_with_runtime(
        &runtime,
        &parsed_request(
            "DELETE",
            &format!("/v1/surfaces/resource-leases/{shared_lease_id}"),
            &[],
            None,
        ),
    )
    .expect("release shared-memory Surface resource");
    assert_eq!(status, 204);
    assert!(runtime
        .shared_images
        .lock()
        .expect("shared images")
        .list()
        .is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn surface_resource_base64_limit_is_checked_before_decode() {
    assert_eq!(
        decode_surface_resource_base64("AAAA", 3).expect("three decoded bytes"),
        [0, 0, 0]
    );
    let encoded_error = decode_surface_resource_base64("AAAAA", 2)
        .expect_err("encoded input over the bound must be rejected before decode");
    assert!(encoded_error.contains("encoded Surface resource exceeds"));
    let decoded_error = decode_surface_resource_base64("AAAA", 2)
        .expect_err("padding can still decode beyond the exact byte limit");
    assert!(decoded_error.contains("decoded Surface resource exceeds"));
}
