// Loom daemon tests fragment 28; included into the shared crate test module.
#[test]
fn surface_confirmation_route_requires_the_bound_approved_device() {
    let root = unique_temp_dir("surface-confirmation-route");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let (instance_id, attachment_id, request) = {
        let mut store = runtime
            .surface_instances
            .lock()
            .expect("lock Surface store");
        let instance = store
            .create(
                "neuro.official/confirmation-test",
                "1.0.0",
                &"c".repeat(64),
                1,
                SurfaceInstancePersistence::Persistent,
                SurfaceInstanceMode::Independent,
            )
            .expect("create Surface instance");
        let attachment = store
            .attach(
                &instance.descriptor.instance_id,
                "hook-node:confirmation-route",
                "device-000-local",
                None,
            )
            .expect("attach Surface instance");
        store
            .put_snapshot(
                &instance.descriptor.instance_id,
                SurfaceSnapshot {
                    protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: instance.descriptor.instance_id.clone(),
                    attachment_id: attachment.descriptor.attachment_id.clone(),
                    art_id: instance.descriptor.art_id,
                    art_version: instance.descriptor.art_version,
                    revision: 1,
                    runtime: SurfaceRuntimeKind::Declarative,
                    entry_resource_id: None,
                    view_id: None,
                    scene: SurfaceNode {
                        id: "root".to_owned(),
                        node_type: "column".to_owned(),
                        children: vec![SurfaceNode {
                            id: "submit".to_owned(),
                            node_type: "button".to_owned(),
                            events: BTreeMap::from([(
                                "click".to_owned(),
                                "submit_form".to_owned(),
                            )]),
                            ..SurfaceNode::default()
                        }],
                        ..SurfaceNode::default()
                    },
                    authoritative_state: json!({}),
                    resources: Vec::new(),
                    resource_leases: Vec::new(),
                },
            )
            .expect("mount Surface snapshot");
        let (_, request) = store
            .await_confirmation(
                &instance.descriptor.instance_id,
                SurfaceEvent {
                    protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: instance.descriptor.instance_id.clone(),
                    attachment_id: attachment.descriptor.attachment_id.clone(),
                    event_id: "event:confirmation-route".to_owned(),
                    node_id: "submit".to_owned(),
                    event: "click".to_owned(),
                    action: Some("submit_form".to_owned()),
                    class: loom_protocol::SurfaceEventClass::Discrete,
                    generation: 0,
                    base_revision: 1,
                    payload: json!({}),
                },
                loom_protocol::SurfaceActionRisk::High,
            )
            .expect("create pending confirmation");
        (
            instance.descriptor.instance_id,
            attachment.descriptor.attachment_id,
            request,
        )
    };
    let (status, unauthorized) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            "/v1/surfaces/confirmations/decision",
            &[],
            Some(
                &json!({
                    "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
                    "confirmationId": request.confirmation_id,
                    "instanceId": instance_id,
                    "attachmentId": attachment_id,
                    "deviceId": "device-000-other",
                    "approved": false
                })
                .to_string(),
            ),
        ),
    )
    .expect("reject unapproved confirmation device");
    assert_eq!(status, 403, "{unauthorized}");
    assert_eq!(
        runtime
            .surface_instances
            .lock()
            .expect("lock Surface store")
            .pending_confirmations()
            .len(),
        1
    );

    let (status, rejected) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            "/v1/surfaces/confirmations/decision",
            &[],
            Some(
                &json!({
                    "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
                    "confirmationId": request.confirmation_id,
                    "instanceId": instance_id,
                    "attachmentId": attachment_id,
                    "deviceId": "device-000-local",
                    "approved": false
                })
                .to_string(),
            ),
        ),
    )
    .expect("reject confirmation on bound device");
    assert_eq!(status, 200, "{rejected}");
    let rejected: Value = serde_json::from_str(&rejected).expect("rejected ack JSON");
    assert_eq!(rejected["status"], "cancelled");
    assert!(runtime
        .surface_instances
        .lock()
        .expect("lock Surface store")
        .pending_confirmations()
        .is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn surface_cancel_route_rejects_an_unapproved_device_before_action_lookup() {
    let root = unique_temp_dir("surface-cancel-route-auth");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let body = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "instanceId": "instance:missing",
        "requestId": "request:missing",
        "deviceId": "device-000-other"
    })
    .to_string();
    let (status, response) = route_with_runtime(
        &runtime,
        &parsed_request("POST", "/v1/surfaces/actions/cancel", &[], Some(&body)),
    )
    .expect("reject Surface cancellation from unapproved device");
    assert_eq!(status, 403, "{response}");
    let response: Value = serde_json::from_str(&response).expect("cancel error JSON");
    assert_eq!(response["error"]["code"], "surface_device_not_authorized");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn creating_surface_instance_requires_an_installed_package() {
    let root = unique_temp_dir("surface-instance-package-required");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let body = json!({ "artId": "neuro.official/missing" }).to_string();
    let (status, response) = route_with_runtime(
        &runtime,
        &parsed_request("POST", "/v1/surfaces/instances", &[], Some(&body)),
    )
    .expect("create missing Surface instance");
    assert_eq!(status, 404);
    let response: Value = serde_json::from_str(&response).expect("response JSON");
    assert_eq!(response["error"]["code"], "surface_art_not_found");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn installed_declarative_surface_package_mounts_and_pushes_snapshot() {
    let root = unique_temp_dir("surface-package-mount");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    runtime
        .framework_registry
        .install_framework_package_from_zip(&framework_package_zip("process", "1.0.0"))
        .expect("install process framework");
    let scene = json!({
        "protocolVersion": "loom.surface.v1",
        "scene": {
            "id": "root",
            "type": "column",
            "children": [
                { "id": "price", "type": "text", "props": { "text": "¥101.20" } },
                {
                    "id": "refresh",
                    "type": "button",
                    "props": { "label": "刷新" },
                    "events": { "click": "refresh_price" }
                }
            ]
        },
        "authoritativeState": { "price": 101.2 }
    });
    loom_tool_registry::install::install_art_from_zip(
        &surface_art_package_zip("surface-stock", "1.0.0", &scene, "independent"),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install Surface Art");

    let create_body = json!({
        "artId": "surface-stock",
        "expectedVersion": "1.0.0"
    })
    .to_string();
    let (status, created) = route_with_runtime(
        &runtime,
        &parsed_request("POST", "/v1/surfaces/instances", &[], Some(&create_body)),
    )
    .expect("create Surface instance");
    assert_eq!(status, 201, "{created}");
    let created: Value = serde_json::from_str(&created).expect("created JSON");
    let instance_id = created["descriptor"]["instanceId"]
        .as_str()
        .expect("instance id")
        .to_owned();

    let attach_body = json!({
        "hookNodeId": "hook-node:surface-stock",
        "deviceId": "device-000-local",
        "capabilities": default_declarative_surface_host_capabilities()
    })
    .to_string();
    let attach_path = format!("/v1/surfaces/instances/{instance_id}/attachments");
    let (status, attached) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &attach_path, &[], Some(&attach_body)),
    )
    .expect("attach Surface instance");
    assert_eq!(status, 201, "{attached}");
    let attached: Value = serde_json::from_str(&attached).expect("attached JSON");
    let attachment_id = attached["descriptor"]["attachmentId"]
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
    let mount_body = json!({ "attachmentId": attachment_id }).to_string();
    let mount_path = format!("/v1/surfaces/instances/{instance_id}/mount");
    let (status, mounted) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &mount_path, &[], Some(&mount_body)),
    )
    .expect("mount Surface instance");
    assert_eq!(status, 200, "{mounted}");
    let mounted: Value = serde_json::from_str(&mounted).expect("mounted JSON");
    assert_eq!(mounted["runtime"], "declarative");
    assert_eq!(mounted["entry"], "surface/main.json");
    assert_eq!(
        mounted["instance"]["attachments"][&attachment_id]["snapshot"]["viewId"],
        "full"
    );
    assert_eq!(
        mounted["instance"]["attachments"][&attachment_id]["snapshot"]["scene"]["children"][0]
            ["props"]["text"],
        "¥101.20"
    );

    let push: Value = serde_json::from_str(
        &surface_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Surface mount push"),
    )
    .expect("Surface push JSON");
    assert_eq!(push["method"], SURFACE_EVENT_SNAPSHOT);
    assert_eq!(push["params"]["hookNodeId"], "hook-node:surface-stock");
    assert_eq!(push["params"]["snapshot"]["revision"], 1);
    let lifecycle: Value = serde_json::from_str(
        &surface_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Surface mounted lifecycle push"),
    )
    .expect("Surface lifecycle JSON");
    assert_eq!(lifecycle["method"], SURFACE_EVENT_LIFECYCLE);
    assert_eq!(lifecycle["params"]["event"]["state"], "mounted");

    let generation_path = format!("/v1/surfaces/instances/{instance_id}/generation");
    let (status, _) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &generation_path, &[], Some("{}")),
    )
    .expect("begin Surface generation");
    assert_eq!(status, 200);
    let _ = surface_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("generation push");

    let event_path = format!("/v1/surfaces/instances/{instance_id}/events");
    let event_body = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "instanceId": instance_id,
        "attachmentId": attachment_id,
        "eventId": "event:refresh-price-1",
        "nodeId": "refresh",
        "event": "click",
        "action": "refresh_price",
        "class": "discrete",
        "generation": 1,
        "baseRevision": 1,
        "payload": {}
    })
    .to_string();
    let (status, ack) = route_with_runtime(
        &runtime,
        &parsed_request("POST", &event_path, &[], Some(&event_body)),
    )
    .expect("accept declared Surface action");
    assert_eq!(status, 202, "{ack}");
    let ack: Value = serde_json::from_str(&ack).expect("action ack JSON");
    assert_eq!(ack["status"], "queued");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn shared_surface_attach_reuses_declared_instance_across_attachments() {
    let root = unique_temp_dir("shared-surface-instance");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    runtime
        .framework_registry
        .install_framework_package_from_zip(&framework_package_zip("process", "1.0.0"))
        .expect("install process framework");
    let scene = json!({
        "protocolVersion": "loom.surface.v1",
        "scene": {
            "id": "root",
            "type": "column",
            "children": [
                { "id": "status", "type": "text", "props": { "text": "ready" } },
                {
                    "id": "refresh",
                    "type": "button",
                    "props": { "label": "Refresh" },
                    "events": { "click": "refresh_price" }
                }
            ]
        },
        "authoritativeState": { "refreshes": 0 }
    });
    loom_tool_registry::install::install_art_from_zip(
        &surface_art_package_zip("surface-shared", "1.0.0", &scene, "shared"),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install shared Surface Art");

    let attach = |hook_node_id: &str| {
        let body = json!({
            "artId": "surface-shared",
            "hookNodeId": hook_node_id,
            "deviceId": "device-000-local",
            "capabilities": default_declarative_surface_host_capabilities()
        })
        .to_string();
        let (status, mounted) = route_with_runtime(
            &runtime,
            &parsed_request("POST", "/v1/surfaces/attach", &[], Some(&body)),
        )
        .expect("attach shared Surface");
        assert_eq!(status, 200, "{mounted}");
        serde_json::from_str::<Value>(&mounted).expect("mounted JSON")
    };

    let first = attach("hook-node:shared-one");
    let second = attach("hook-node:shared-two");
    let first_id = first["instance"]["descriptor"]["instanceId"]
        .as_str()
        .expect("first shared instance id");
    let second_id = second["instance"]["descriptor"]["instanceId"]
        .as_str()
        .expect("second shared instance id");
    assert_eq!(first_id, second_id);
    assert_eq!(second["instance"]["descriptor"]["instanceMode"], "shared");
    assert_eq!(
        second["instance"]["attachments"]
            .as_object()
            .expect("shared attachments")
            .len(),
        2
    );
    assert_eq!(
        runtime
            .surface_instances
            .lock()
            .expect("Surface store")
            .list()
            .len(),
        1
    );
    let _ = fs::remove_dir_all(root);
}
