// Loom daemon tests fragment 29; included into the shared crate test module.
#[test]
fn surface_attach_replaces_prior_binding_and_recovery_selects_one_instance() {
    let root = unique_temp_dir("surface-attachment-rebind");
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
            "children": [{ "id": "price", "type": "text", "props": { "text": "101.20" } }]
        },
        "authoritativeState": { "price": 101.2 }
    });
    loom_tool_registry::install::install_art_from_zip(
        &surface_art_package_zip("surface-rebind", "1.0.0", &scene, "independent"),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install Surface Art");

    let mount_without_rebind = || {
        let (status, created) = route_with_runtime(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/surfaces/instances",
                &[],
                Some(
                    &json!({
                        "artId": "surface-rebind",
                        "expectedVersion": "1.0.0"
                    })
                    .to_string(),
                ),
            ),
        )
        .expect("create Surface instance");
        assert_eq!(status, 201, "{created}");
        let created: Value = serde_json::from_str(&created).expect("created JSON");
        let instance_id = created["descriptor"]["instanceId"]
            .as_str()
            .expect("instance id")
            .to_owned();
        let attach_path = format!("/v1/surfaces/instances/{instance_id}/attachments");
        let (status, attached) = route_with_runtime(
            &runtime,
            &parsed_request(
                "POST",
                &attach_path,
                &[],
                Some(
                    &json!({
                        "hookNodeId": "hook-node:rebind",
                        "deviceId": "device-000-local",
                        "capabilities": default_declarative_surface_host_capabilities()
                    })
                    .to_string(),
                ),
            ),
        )
        .expect("attach Surface instance");
        assert_eq!(status, 201, "{attached}");
        let attached: Value = serde_json::from_str(&attached).expect("attached JSON");
        let attachment_id = attached["descriptor"]["attachmentId"]
            .as_str()
            .expect("attachment id")
            .to_owned();
        let mount_path = format!("/v1/surfaces/instances/{instance_id}/mount");
        let (status, mounted) = route_with_runtime(
            &runtime,
            &parsed_request(
                "POST",
                &mount_path,
                &[],
                Some(&json!({ "attachmentId": attachment_id }).to_string()),
            ),
        )
        .expect("mount Surface instance");
        assert_eq!(status, 200, "{mounted}");
        (instance_id, attachment_id)
    };

    let (first_instance_id, first_attachment_id) = mount_without_rebind();
    let (second_instance_id, second_attachment_id) = mount_without_rebind();
    let expected_recovery_instance = runtime
        .surface_instances
        .lock()
        .expect("Surface store")
        .list()
        .into_iter()
        .filter(|instance| {
            instance.attachments.values().any(|attachment| {
                attachment.descriptor.hook_node_id == "hook-node:rebind"
                    && attachment.descriptor.device_id == "device-000-local"
                    && attachment.lifecycle != loom_protocol::SurfaceLifecycleState::Disposed
                    && attachment.snapshot.is_some()
            })
        })
        .map(|instance| (instance.created_at_ms, instance.descriptor.instance_id))
        .max()
        .expect("latest recovery instance")
        .1;
    let recovery = surface_snapshot_recovery_messages(&runtime.surface_instances);
    assert_eq!(
        recovery.len(),
        1,
        "duplicate recovery snapshots: {recovery:?}"
    );
    let recovered: Value = serde_json::from_str(&recovery[0]).expect("recovery JSON");
    assert_eq!(
        recovered["params"]["snapshot"]["instanceId"],
        expected_recovery_instance
    );

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
    let body = json!({
        "artId": "surface-rebind",
        "expectedVersion": "1.0.0",
        "hookNodeId": "hook-node:rebind",
        "deviceId": "device-000-local",
        "capabilities": default_declarative_surface_host_capabilities()
    })
    .to_string();
    let (status, mounted) = route_with_runtime(
        &runtime,
        &parsed_request("POST", "/v1/surfaces/attach", &[], Some(&body)),
    )
    .expect("replace Surface binding");
    assert_eq!(status, 200, "{mounted}");
    let mounted: Value = serde_json::from_str(&mounted).expect("mounted JSON");
    let current_instance_id = mounted["instance"]["descriptor"]["instanceId"]
        .as_str()
        .expect("current instance id")
        .to_owned();
    let current_attachment_id = mounted["instance"]["attachments"]
        .as_object()
        .expect("current attachments")
        .keys()
        .next()
        .expect("current attachment id")
        .to_owned();

    let instances = runtime
        .surface_instances
        .lock()
        .expect("Surface store")
        .list();
    let bindings = instances
        .iter()
        .flat_map(|instance| {
            instance.attachments.values().filter_map(|attachment| {
                (attachment.descriptor.hook_node_id == "hook-node:rebind"
                    && attachment.descriptor.device_id == "device-000-local")
                    .then_some((
                        instance.descriptor.instance_id.as_str(),
                        attachment.descriptor.attachment_id.as_str(),
                        &attachment.lifecycle,
                        attachment.snapshot.is_some(),
                    ))
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 3);
    assert_eq!(
        bindings
            .iter()
            .filter(|(_, _, lifecycle, _)| {
                **lifecycle != loom_protocol::SurfaceLifecycleState::Disposed
            })
            .count(),
        1
    );
    assert!(bindings
        .iter()
        .any(|(instance_id, attachment_id, lifecycle, snapshot)| {
            *instance_id == current_instance_id
                && *attachment_id == current_attachment_id
                && **lifecycle == loom_protocol::SurfaceLifecycleState::Mounted
                && *snapshot
        }));
    for (instance_id, attachment_id) in [
        (&first_instance_id, &first_attachment_id),
        (&second_instance_id, &second_attachment_id),
    ] {
        assert!(bindings.iter().any(
            |(candidate_instance, candidate_attachment, lifecycle, snapshot)| {
                *candidate_instance == instance_id
                    && *candidate_attachment == attachment_id
                    && **lifecycle == loom_protocol::SurfaceLifecycleState::Disposed
                    && !*snapshot
            }
        ));
    }

    let broadcasts = (0..6)
        .map(|_| {
            surface_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("Surface rebind broadcast")
        })
        .collect::<Vec<_>>();
    let disposed_instances = broadcasts
        .iter()
        .filter_map(|message| serde_json::from_str::<Value>(message).ok())
        .filter(|message| message["method"] == SURFACE_EVENT_DISPOSE)
        .filter_map(|message| message["params"]["instanceId"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        disposed_instances,
        BTreeSet::from([first_instance_id, second_instance_id])
    );
    let recovery = surface_snapshot_recovery_messages(&runtime.surface_instances);
    assert_eq!(recovery.len(), 1);
    let recovered: Value = serde_json::from_str(&recovery[0]).expect("recovery JSON");
    assert_eq!(
        recovered["params"]["snapshot"]["instanceId"],
        current_instance_id
    );
    let _ = fs::remove_dir_all(root);
}
