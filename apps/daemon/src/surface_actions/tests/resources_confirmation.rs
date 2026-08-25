// Resource brokerage and device-bound confirmation regression tests.
    #[test]
    fn action_response_accepts_only_host_brokered_resource_references() {
        let root = temp_root("resource-broker");
        let resources = Arc::new(Mutex::new(
            SurfaceResourceStore::new(root.join("surface-resources"))
                .expect("open Surface resource store"),
        ));
        let forged: SurfaceActionResponse = serde_json::from_value(json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "patches": [{
                "resources": [{
                    "resourceId": format!("sha256:{}", "a".repeat(64)),
                    "kind": "image",
                    "mime": "image/png",
                    "size": 4
                }]
            }]
        }))
        .expect("parse forged action response");
        let error = validate_action_response_resources(&forged, &resources)
            .expect_err("reject resource not registered by Loom");
        assert_eq!(error.code, "surface_resource_not_found");

        let lease = resources
            .lock()
            .expect("lock Surface resource store")
            .register(
                loom_protocol::SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"brokered",
                None,
                None,
                None,
            )
            .expect("register brokered resource");
        let accepted: SurfaceActionResponse = serde_json::from_value(json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "patches": [{
                "resources": [lease.resource],
                "resourceLeases": [lease]
            }]
        }))
        .expect("parse brokered action response");
        validate_action_response_resources(&accepted, &resources)
            .expect("accept Loom-issued resource descriptor and lease");

        let upload: SurfaceActionResponse = serde_json::from_value(json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "resourceUploads": [{
                "id": "chart",
                "kind": "image",
                "mime": "image/png",
                "dataBase64": BASE64.encode(b"prototype-image"),
                "width": 1,
                "height": 1
            }],
            "patches": [{
                "operations": [{
                    "op": "set",
                    "nodeId": "chart",
                    "path": "/props/resourceId",
                    "value": "surface-upload:chart"
                }]
            }]
        }))
        .expect("parse resource upload response");
        let brokered = broker_action_resource_uploads(upload, &resources)
            .expect("broker package resource upload");
        assert_eq!(brokered.patches[0].resources.len(), 1);
        assert_eq!(brokered.patches[0].resource_leases.len(), 1);
        let brokered_json = serde_json::to_value(&brokered).expect("brokered response JSON");
        let resource_id = brokered_json["patches"][0]["operations"][0]["value"]
            .as_str()
            .expect("resolved resource id");
        assert!(resource_id.starts_with("sha256:"));
        assert_ne!(resource_id, "surface-upload:chart");
        validate_action_response_resources(&brokered, &resources)
            .expect("accept brokered resource upload response");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn action_response_rejects_excessive_nesting_before_recursive_processing() {
        let mut nested = json!("leaf");
        for _ in 0..=loom_security::json::MAX_PROCESS_RESPONSE_DEPTH {
            nested = json!([nested]);
        }
        let error = parse_surface_action_response(json!({
            "surfaceAction": {
                "protocolVersion": SURFACE_PROTOCOL_VERSION,
                "result": {
                    "outputs": {},
                    "statePatch": {"nested": nested}
                }
            }
        }))
        .expect_err("reject an action response deeper than the host JSON budget");
        assert_eq!(error.code, "surface_action_response_limit");
        assert!(error.message.contains("nesting limit"));
    }

    #[test]
    fn action_response_preserves_structured_art_runtime_error() {
        let error = parse_surface_action_response(json!({
            "status": "error",
            "error": {
                "code": "surface_prototype_failed",
                "message": "action is not declared by the prototype"
            }
        }))
        .expect_err("runtime error must not parse as a success response");

        assert_eq!(error.code, "surface_art_runtime_error");
        assert_eq!(
            error.message,
            "Art runtime error `surface_prototype_failed`: action is not declared by the prototype"
        );
    }

    #[test]
    fn confirmed_action_never_executes_before_device_bound_host_approval() {
        let root = temp_root("confirmation");
        let digest = "b".repeat(64);
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let mut tool = surface_tool(&digest);
        let metadata = tool.metadata.as_mut().expect("Surface metadata");
        metadata["capabilities"]["surface"]["actions"][0]["confirmation"] = json!(true);
        metadata["capabilities"]["surface"]["actions"][0]["risk"] = json!("high");
        tool_registry
            .save_tool(tool)
            .expect("save confirmed Surface tool");
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
                    "hook-node:confirmation",
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
                        runtime: SurfaceRuntimeKind::Declarative,
                        entry_resource_id: None,
                        view_id: None,
                        scene: SurfaceNode {
                            id: "root".to_owned(),
                            node_type: "column".to_owned(),
                            children: vec![SurfaceNode {
                                id: "refresh".to_owned(),
                                node_type: "button".to_owned(),
                                events: BTreeMap::from([(
                                    "click".to_owned(),
                                    "refresh_price".to_owned(),
                                )]),
                                ..SurfaceNode::default()
                            }],
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
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |_| {
            runner_executions.fetch_add(1, Ordering::SeqCst);
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {
                        "outputs": {"ok": {"kind": "value", "value": true}}
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
            attachment_id: attachment_id.clone(),
            event_id: "event:confirmed-1".to_owned(),
            node_id: "refresh".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh_price".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload: json!({"symbol": "MSFT"}),
        };
        let awaiting = executor
            .submit(&instance_id, event)
            .expect("request Host confirmation");
        assert_eq!(awaiting.status, SurfaceActionStatus::AwaitingConfirmation);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let pending = instances
            .lock()
            .expect("lock Surface store")
            .pending_confirmations();
        assert_eq!(pending.len(), 1);
        assert!(instances
            .lock()
            .expect("lock Surface store")
            .pending_events()
            .is_empty());
        let confirmation_push: Value = serde_json::from_str(
            &rx.recv_timeout(Duration::from_secs(1))
                .expect("confirmation broadcast"),
        )
        .expect("confirmation JSON");
        assert_eq!(
            confirmation_push["method"],
            SURFACE_EVENT_CONFIRMATION_REQUEST
        );
        assert_eq!(confirmation_push["params"]["risk"], "high");

        let approved = executor
            .confirm(SurfaceConfirmationDecision {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                confirmation_id: pending[0].confirmation_id.clone(),
                instance_id: instance_id.clone(),
                attachment_id: attachment_id.clone(),
                device_id: "device-000-local".to_owned(),
                approved: true,
            })
            .expect("approve confirmed Surface action");
        assert_eq!(approved.status, SurfaceActionStatus::Queued);
        for _ in 0..20 {
            if executions.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(executions.load(Ordering::SeqCst), 1);

        let rejected_event = SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            attachment_id: attachment_id.clone(),
            event_id: "event:confirmed-2".to_owned(),
            node_id: "refresh".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh_price".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload: json!({}),
        };
        let awaiting_rejection = executor
            .submit(&instance_id, rejected_event)
            .expect("request second confirmation");
        assert_eq!(
            awaiting_rejection.status,
            SurfaceActionStatus::AwaitingConfirmation
        );
        let pending = instances
            .lock()
            .expect("lock Surface store")
            .pending_confirmations();
        let rejected = executor
            .confirm(SurfaceConfirmationDecision {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                confirmation_id: pending[0].confirmation_id.clone(),
                instance_id,
                attachment_id,
                device_id: "device-000-local".to_owned(),
                approved: false,
            })
            .expect("reject confirmed Surface action");
        assert_eq!(rejected.status, SurfaceActionStatus::Cancelled);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }
