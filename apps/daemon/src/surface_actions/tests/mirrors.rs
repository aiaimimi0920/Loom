#[test]
fn wall_mirror_actions_update_the_source_and_mounted_views_without_racing_new_attachments() {
    for from_mirror in [false, true] {
        let root = temp_root("wall-mirror-fanout");
        let tool = surface_tool(&"a".repeat(64));
        let (registry, instances, resources, bridge, id, source_id) =
            setup_action_fixture(&root, tool, "source");
        let mirror_id = {
            let mut store = instances.lock().unwrap();
            let original = store.get(&id).unwrap().attachments[&source_id]
                .snapshot
                .clone()
                .unwrap();
            let mirror = store
                .attach_ephemeral(&id, "tile-one", "tile-device", host_capabilities())
                .unwrap();
            let mut snapshot = original.clone();
            snapshot.attachment_id = mirror.descriptor.attachment_id.clone();
            store.put_snapshot(&id, snapshot).unwrap();
            mirror.descriptor.attachment_id
        };
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let finish_rx = Mutex::new(finish_rx);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |_| {
            started_tx.send(()).unwrap();
            finish_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok(
                json!({"surfaceAction": {"protocolVersion":SURFACE_PROTOCOL_VERSION,
                "patches":[{"operations":[{"op":"set", "nodeId":"refresh", "path":"/props/label", "value":"Updated"}],
                    "statePatch":{"value":8}}]}}),
            )
        });
        let (rx, _subscription) = register_hook_bridge_subscription(
            &bridge.lock().unwrap().broadcast_hub,
            vec![SURFACE_EVENT_ACTION_ACK.to_owned()],
        );
        let executor = SurfaceActionExecutor::new_with_runner(
            registry,
            Arc::clone(&instances),
            resources,
            bridge,
            runner,
            1,
            4,
        )
        .unwrap();
        let event = fixture_event(
            &id,
            if from_mirror { &mirror_id } else { &source_id },
            "mirror-update",
            Value::Null,
        );
        executor.submit(&id, event).unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        // Attach while the action is running, before mount publishes its snapshot.
        let unmounted = instances
            .lock()
            .unwrap()
            .attach_ephemeral(&id, "tile-two", "tile-device", host_capabilities())
            .unwrap();
        // A later ordinary view must not redirect already pinned mirrors or receive their patches.
        let other_id = {
            let mut store = instances.lock().unwrap();
            let other = store
                .attach(
                    &id,
                    "independent-view",
                    "other-device",
                    Some(host_capabilities()),
                )
                .unwrap();
            let mut snapshot = store.get(&id).unwrap().attachments[&source_id]
                .snapshot
                .clone()
                .unwrap();
            snapshot.attachment_id = other.descriptor.attachment_id.clone();
            store.put_snapshot(&id, snapshot).unwrap();
            other.descriptor.attachment_id
        };
        finish_tx.send(()).unwrap();
        loop {
            let message: Value =
                serde_json::from_str(&rx.recv_timeout(Duration::from_secs(3)).unwrap()).unwrap();
            let status = message["params"]["status"].as_str().unwrap();
            assert!(!matches!(status, "failed" | "cancelled"), "{message}");
            if status == "succeeded" {
                break;
            }
        }
        let record = instances.lock().unwrap().get(&id).unwrap();
        for target in [&source_id, &mirror_id] {
            let snapshot = record.attachments[target].snapshot.as_ref().unwrap();
            assert_eq!(snapshot.revision, 2);
            assert_eq!(snapshot.scene.children[0].props["label"], "Updated");
        }
        assert_eq!(
            record.attachments[&other_id]
                .snapshot
                .as_ref()
                .unwrap()
                .revision,
            1
        );
        assert!(record.attachments[&unmounted.descriptor.attachment_id]
            .snapshot
            .is_none());
        assert_eq!(record.authoritative_state["value"], 8);
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn wall_mirror_confirmation_approval_and_cancellation_preserve_the_original_instance() {
    let root = temp_root("wall-mirror-cancel");
    let mut tool = surface_tool(&"b".repeat(64));
    let action = &mut tool.metadata.as_mut().unwrap()["capabilities"]["surface"]["actions"][0];
    action["confirmation"] = json!(true);
    action["cancelable"] = json!(true);
    let (registry, instances, resources, bridge, id, source_id) =
        setup_action_fixture(&root, tool, "source");
    let mirror_id = {
        let mut store = instances.lock().unwrap();
        let mirror = store
            .attach_ephemeral(&id, "tile-one", "tile-device", host_capabilities())
            .unwrap();
        let mut snapshot = store.get(&id).unwrap().attachments[&source_id]
            .snapshot
            .clone()
            .unwrap();
        snapshot.attachment_id = mirror.descriptor.attachment_id.clone();
        store.put_snapshot(&id, snapshot).unwrap();
        mirror.descriptor.attachment_id
    };
    let (started_tx, started_rx) = mpsc::channel();
    let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
        started_tx.send(()).unwrap();
        while !job.cancellation.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(
            json!({"surfaceAction":{"protocolVersion":SURFACE_PROTOCOL_VERSION,
            "result":{"outputs":{"value":{"kind":"value","value":"must-not-commit"}}}}}),
        )
    });
    let (rx, _subscription) = register_hook_bridge_subscription(
        &bridge.lock().unwrap().broadcast_hub,
        vec![SURFACE_EVENT_ACTION_ACK.to_owned()],
    );
    let executor = SurfaceActionExecutor::new_with_runner(
        registry,
        Arc::clone(&instances),
        resources,
        bridge,
        runner,
        1,
        4,
    )
    .unwrap();
    let ack = executor
        .submit(
            &id,
            fixture_event(&id, &mirror_id, "wall-confirm-cancel", Value::Null),
        )
        .unwrap();
    assert_eq!(ack.status, SurfaceActionStatus::AwaitingConfirmation);
    assert!(started_rx.try_recv().is_err());
    let confirmation_id = instances
        .lock()
        .unwrap()
        .get(&id)
        .unwrap()
        .pending_confirmations
        .keys()
        .next()
        .unwrap()
        .clone();
    executor
        .confirm(SurfaceConfirmationDecision {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: id.clone(),
            attachment_id: mirror_id.clone(),
            device_id: "tile-device".into(),
            confirmation_id,
            approved: true,
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(instances
        .lock()
        .unwrap()
        .remove_ephemeral_attachment(&id, &mirror_id)
        .is_err());
    executor
        .cancel(SurfaceActionCancelRequest {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: id.clone(),
            device_id: "tile-device".into(),
            request_id: ack.request_id,
        })
        .unwrap();
    loop {
        let message: Value =
            serde_json::from_str(&rx.recv_timeout(Duration::from_secs(3)).unwrap()).unwrap();
        if message["params"]["status"] == "cancelled" {
            break;
        }
        assert_ne!(message["params"]["status"], "succeeded");
    }
    let mut store = instances.lock().unwrap();
    store.remove_ephemeral_attachment(&id, &mirror_id).unwrap();
    let record = store.get(&id).unwrap();
    assert!(record.attachments.contains_key(&source_id));
    assert_eq!(record.authoritative_state["value"], 0);
    assert!(record.latest_result.is_none());
    drop(store);
    drop(executor);
    let _ = std::fs::remove_dir_all(root);
}
