// Execution, commit, and shared-instance fan-out regression tests.
    #[test]
    fn declared_action_executes_once_and_commits_patch_and_formal_result() {
        let root = temp_root("commit");
        let digest = "a".repeat(64);
        let tool_registry = ToolRegistry::new(root.join("tools"));
        tool_registry
            .save_tool(surface_tool(&digest))
            .expect("save Surface tool");
        let instances = Arc::new(Mutex::new(
            SurfaceInstanceStore::new(root.join("surface-instances.json"))
                .expect("open Surface store"),
        ));
        let (instance_id, attachment_id) = {
            let mut store = instances.lock().expect("lock Surface store");
            let instance = store
                .create(
                    "surface-action-test",
                    "1.0.0",
                    &digest,
                    1,
                    SurfaceInstancePersistence::Persistent,
                    loom_protocol::SurfaceInstanceMode::Independent,
                )
                .expect("create instance");
            let attachment = store
                .attach(
                    &instance.descriptor.instance_id,
                    "hook-node:test",
                    "device-000-local",
                    Some(host_capabilities()),
                )
                .expect("attach");
            store
                .put_snapshot(
                    &instance.descriptor.instance_id,
                    SurfaceSnapshot {
                        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                        instance_id: instance.descriptor.instance_id.clone(),
                        attachment_id: attachment.descriptor.attachment_id.clone(),
                        art_id: instance.descriptor.art_id,
                        art_version: instance.descriptor.art_version,
                        revision: 1,
                        runtime: loom_protocol::SurfaceRuntimeKind::Declarative,
                        entry_resource_id: None,
                        view_id: None,
                        scene: SurfaceNode {
                            id: "root".to_owned(),
                            node_type: "column".to_owned(),
                            children: vec![
                                SurfaceNode {
                                    id: "price".to_owned(),
                                    node_type: "text".to_owned(),
                                    props: json!({"text": "100"}),
                                    ..SurfaceNode::default()
                                },
                                SurfaceNode {
                                    id: "refresh".to_owned(),
                                    node_type: "button".to_owned(),
                                    events: BTreeMap::from([(
                                        "click".to_owned(),
                                        "refresh_price".to_owned(),
                                    )]),
                                    ..SurfaceNode::default()
                                },
                            ],
                            ..SurfaceNode::default()
                        },
                        authoritative_state: json!({"price": 100}),
                        resources: Vec::new(),
                        resource_leases: Vec::new(),
                    },
                )
                .expect("mount snapshot");
            (
                instance.descriptor.instance_id,
                attachment.descriptor.attachment_id,
            )
        };
        let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
        let (rx, _subscription) = register_hook_bridge_subscription(
            &hook_bridge.lock().expect("lock bridge").broadcast_hub,
            loom_protocol::SURFACE_EVENT_METHODS
                .iter()
                .map(|method| (*method).to_owned())
                .collect(),
        );
        let executions = Arc::new(AtomicUsize::new(0));
        let runner_executions = Arc::clone(&executions);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
            runner_executions.fetch_add(1, Ordering::SeqCst);
            assert_eq!(job.invocation.action_id, "refresh_price");
            assert_eq!(job.invocation.authoritative_state["price"], 100);
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "patches": [{
                        "operations": [{
                            "op": "set",
                            "nodeId": "price",
                            "path": "/props/text",
                            "value": "101"
                        }],
                        "statePatch": {"price": 101}
                    }],
                    "result": {
                        "outputs": {
                            "price": {"kind": "value", "value": 101}
                        },
                        "statePatch": {"price": 101}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            tool_registry,
            Arc::clone(&instances),
            Arc::new(Mutex::new(
                SurfaceResourceStore::new(root.join("surface-resources"))
                    .expect("open Surface resource store"),
            )),
            Arc::clone(&hook_bridge),
            runner,
            1,
            4,
        )
        .expect("start executor");
        let event = SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            attachment_id,
            event_id: "event:refresh-1".to_owned(),
            node_id: "refresh".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh_price".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload: json!({}),
        };
        let queued = executor
            .submit(&instance_id, event.clone())
            .expect("queue action");
        assert_eq!(queued.status, SurfaceActionStatus::Queued);

        let mut saw_patch = false;
        let mut saw_result = false;
        let mut saw_success = false;
        for _ in 0..8 {
            let message = rx
                .recv_timeout(Duration::from_secs(2))
                .expect("Surface action broadcast");
            let message: Value = serde_json::from_str(&message).expect("broadcast JSON");
            match message["method"].as_str() {
                Some(SURFACE_EVENT_PATCH) => saw_patch = true,
                Some(SURFACE_EVENT_RESULT) => saw_result = true,
                Some(SURFACE_EVENT_ACTION_ACK) if message["params"]["status"] == "succeeded" => {
                    saw_success = true;
                }
                _ => {}
            }
            if saw_patch && saw_result && saw_success {
                break;
            }
        }
        assert!(saw_patch && saw_result && saw_success);
        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("instance record");
        assert!(record.pending_events.is_empty());
        assert_eq!(record.authoritative_state["price"], 101);
        assert_eq!(
            record
                .attachments
                .values()
                .next()
                .and_then(|attachment| attachment.snapshot.as_ref())
                .expect("snapshot")
                .scene
                .children[0]
                .props["text"],
            "101"
        );
        assert_eq!(
            record.latest_result.expect("formal result").outputs["price"],
            loom_protocol::SurfacePortValue::Value { value: json!(101) }
        );

        let duplicate = executor
            .submit(&instance_id, event)
            .expect("deduplicate completed action");
        assert_eq!(duplicate.status, SurfaceActionStatus::Succeeded);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn shared_instance_patch_fans_out_to_every_mounted_attachment() {
        let root = temp_root("shared-fanout");
        let digest = "f".repeat(64);
        let tool = surface_tool(&digest);
        let instances = Arc::new(Mutex::new(
            SurfaceInstanceStore::new(root.join("surface-instances.json"))
                .expect("open Surface store"),
        ));
        let (instance_id, attachment_ids) = {
            let mut store = instances.lock().expect("lock Surface store");
            let instance = store
                .create(
                    "surface-action-test",
                    "1.0.0",
                    &digest,
                    1,
                    SurfaceInstancePersistence::Persistent,
                    SurfaceInstanceMode::Shared,
                )
                .expect("create shared instance");
            let mut attachment_ids = Vec::new();
            for suffix in ["one", "two"] {
                let attachment = store
                    .attach(
                        &instance.descriptor.instance_id,
                        &format!("hook-node:{suffix}"),
                        "device-000-local",
                        Some(host_capabilities()),
                    )
                    .expect("attach shared Surface");
                store
                    .put_snapshot(
                        &instance.descriptor.instance_id,
                        SurfaceSnapshot {
                            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                            instance_id: instance.descriptor.instance_id.clone(),
                            attachment_id: attachment.descriptor.attachment_id.clone(),
                            art_id: instance.descriptor.art_id.clone(),
                            art_version: instance.descriptor.art_version.clone(),
                            revision: 1,
                            runtime: SurfaceRuntimeKind::Declarative,
                            entry_resource_id: None,
                            view_id: None,
                            scene: SurfaceNode {
                                id: "root".to_owned(),
                                node_type: "column".to_owned(),
                                children: vec![SurfaceNode {
                                    id: "status".to_owned(),
                                    node_type: "text".to_owned(),
                                    props: json!({"text": "idle"}),
                                    ..SurfaceNode::default()
                                }],
                                ..SurfaceNode::default()
                            },
                            authoritative_state: json!({"status": "idle"}),
                            resources: Vec::new(),
                            resource_leases: Vec::new(),
                        },
                    )
                    .expect("mount shared snapshot");
                attachment_ids.push(attachment.descriptor.attachment_id);
            }
            (instance.descriptor.instance_id, attachment_ids)
        };
        let resources = Arc::new(Mutex::new(
            SurfaceResourceStore::new(root.join("surface-resources"))
                .expect("open Surface resources"),
        ));
        let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
        let (rx, _subscription) = register_hook_bridge_subscription(
            &hook_bridge.lock().expect("lock bridge").broadcast_hub,
            loom_protocol::SURFACE_EVENT_METHODS
                .iter()
                .map(|method| (*method).to_owned())
                .collect(),
        );
        let event = fixture_event(
            &instance_id,
            &attachment_ids[0],
            "event:shared-refresh",
            Value::Null,
        );
        let action = tool
            .surface_manifest()
            .expect("parse Surface manifest")
            .expect("Surface manifest")
            .actions
            .into_iter()
            .find(|action| action.id == "refresh_price")
            .expect("refresh action");
        let job = SurfaceActionJob {
            invocation: SurfaceActionInvocation {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                attachment_id: attachment_ids[0].clone(),
                request_id: "request:shared-refresh".to_owned(),
                event_id: event.event_id.clone(),
                action_id: action.id.clone(),
                event_class: event.class.clone(),
                generation: 0,
                base_revision: 1,
                payload: Value::Null,
                authoritative_state: json!({"status": "idle"}),
            },
            ack: SurfaceActionAck {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                event_id: event.event_id.clone(),
                request_id: "request:shared-refresh".to_owned(),
                accepted: true,
                status: SurfaceActionStatus::Running,
                error: None,
            },
            event,
            action,
            tool,
            cancellation: Arc::new(AtomicBool::new(false)),
        };
        let response = serde_json::from_value::<SurfaceActionResponse>(json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "patches": [{
                "operations": [{
                    "op": "set",
                    "nodeId": "status",
                    "path": "/props/text",
                    "value": "ready"
                }],
                "statePatch": {"status": "ready"}
            }]
        }))
        .expect("shared action response");
        apply_action_response(&job, response, &instances, &resources, &hook_bridge)
            .expect("apply shared action response");

        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("shared instance");
        for attachment_id in &attachment_ids {
            let snapshot = record.attachments[attachment_id]
                .snapshot
                .as_ref()
                .expect("shared snapshot");
            assert_eq!(snapshot.revision, 2);
            assert_eq!(snapshot.scene.children[0].props["text"], "ready");
        }
        let mut hook_nodes = BTreeSet::new();
        for _ in 0..2 {
            let message: Value = serde_json::from_str(
                &rx.recv_timeout(Duration::from_secs(1))
                    .expect("shared patch broadcast"),
            )
            .expect("shared patch JSON");
            assert_eq!(message["method"], SURFACE_EVENT_PATCH);
            hook_nodes.insert(
                message["params"]["hookNodeId"]
                    .as_str()
                    .expect("Hook node id")
                    .to_owned(),
            );
        }
        assert_eq!(
            hook_nodes,
            BTreeSet::from(["hook-node:one".to_owned(), "hook-node:two".to_owned()])
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stale_generation_rejects_preview_and_formal_result_commits() {
        let root = temp_root("stale-generation-commits");
        let digest = "9".repeat(64);
        let tool = surface_tool(&digest);
        let (_registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool.clone(), "hook-node:stale-generation");
        let event = fixture_event(
            &instance_id,
            &attachment_id,
            "event:stale-generation",
            Value::Null,
        );
        let action = tool
            .surface_manifest()
            .expect("parse Surface manifest")
            .expect("Surface manifest")
            .actions
            .into_iter()
            .find(|action| action.id == "refresh_price")
            .expect("refresh action");
        let request_id = "request:stale-generation".to_owned();
        let job = SurfaceActionJob {
            invocation: SurfaceActionInvocation {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                attachment_id: attachment_id.clone(),
                request_id: request_id.clone(),
                event_id: event.event_id.clone(),
                action_id: action.id.clone(),
                event_class: event.class.clone(),
                generation: event.generation,
                base_revision: event.base_revision,
                payload: event.payload.clone(),
                authoritative_state: json!({"value": 0}),
            },
            ack: SurfaceActionAck {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                event_id: event.event_id.clone(),
                request_id,
                accepted: true,
                status: SurfaceActionStatus::Running,
                error: None,
            },
            event,
            action,
            tool,
            cancellation: Arc::new(AtomicBool::new(false)),
        };
        instances
            .lock()
            .expect("lock Surface store")
            .begin_generation(&instance_id, Some(0))
            .expect("advance Surface generation");

        let responses = [
            json!({
                "protocolVersion": SURFACE_PROTOCOL_VERSION,
                "preview": {
                    "portId": "preview",
                    "value": {"kind": "value", "value": "stale-preview"}
                }
            }),
            json!({
                "protocolVersion": SURFACE_PROTOCOL_VERSION,
                "result": {
                    "outputs": {
                        "value": {"kind": "value", "value": "stale-result"}
                    },
                    "statePatch": {"value": "stale-result"}
                }
            }),
        ];
        for response in responses {
            let response = serde_json::from_value::<SurfaceActionResponse>(response)
                .expect("parse stale action response");
            let error = apply_action_response(
                &job,
                response,
                &instances,
                &resources,
                &hook_bridge,
            )
            .expect_err("reject a commit from an older Surface generation");
            assert_eq!(error.code, "surface_action_stale_generation");
        }

        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("Surface instance");
        assert!(record.latest_preview.is_none());
        assert!(record.latest_result.is_none());
        assert_eq!(record.authoritative_state, json!({"value": 0}));
        let _ = std::fs::remove_dir_all(root);
    }
