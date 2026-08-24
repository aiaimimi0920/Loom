// Timeout, explicit cancellation, and replace-latest regression tests.
    #[test]
    fn action_timeout_is_terminal_and_late_runner_result_never_commits() {
        let root = temp_root("timeout");
        let digest = "c".repeat(64);
        let mut tool = surface_tool(&digest);
        tool.metadata.as_mut().expect("Surface metadata")["capabilities"]["surface"]["actions"]
            [0]["timeoutMs"] = json!(40);
        let (registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool, "hook-node:timeout");
        let runner_finished = Arc::new(AtomicBool::new(false));
        let worker_finished = Arc::clone(&runner_finished);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |_| {
            std::thread::sleep(Duration::from_millis(180));
            worker_finished.store(true, Ordering::Release);
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {
                        "outputs": {"value": {"kind": "value", "value": "late"}},
                        "statePatch": {"value": "late"}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            registry,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            runner,
            1,
            4,
        )
        .expect("start timeout executor");
        let queued = executor
            .submit(
                &instance_id,
                fixture_event(&instance_id, &attachment_id, "event:timeout", Value::Null),
            )
            .expect("queue timed action");
        let started = Instant::now();
        loop {
            let record = instances
                .lock()
                .expect("lock Surface store")
                .get(&instance_id)
                .expect("Surface instance");
            let ack = record
                .event_acks
                .values()
                .find(|ack| ack.request_id == queued.request_id)
                .expect("timed action ack");
            if ack.status == SurfaceActionStatus::Failed {
                assert_eq!(
                    ack.error.as_ref().expect("timeout error").code,
                    "surface_action_timeout"
                );
                assert!(record.pending_events.is_empty());
                assert!(record.latest_result.is_none());
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(1));
            drop(record);
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(220));
        assert!(runner_finished.load(Ordering::Acquire));
        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("Surface instance after late result");
        assert_eq!(record.authoritative_state["value"], 0);
        assert!(record.latest_result.is_none());
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_cancel_is_device_bound_and_stops_a_cancelable_action() {
        let root = temp_root("cancel");
        let digest = "d".repeat(64);
        let mut tool = surface_tool(&digest);
        let action = &mut tool.metadata.as_mut().expect("Surface metadata")["capabilities"]
            ["surface"]["actions"][0];
        action["cancelable"] = json!(true);
        action["timeoutMs"] = json!(2000);
        let (registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool, "hook-node:cancel");
        let runner_started = Arc::new(AtomicBool::new(false));
        let worker_started = Arc::clone(&runner_started);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
            worker_started.store(true, Ordering::Release);
            while !job.cancellation.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {"outputs": {"value": {"kind": "value", "value": "late"}}}
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            registry,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            runner,
            1,
            4,
        )
        .expect("start cancellation executor");
        let queued = executor
            .submit(
                &instance_id,
                fixture_event(&instance_id, &attachment_id, "event:cancel", Value::Null),
            )
            .expect("queue cancelable action");
        for _ in 0..100 {
            if runner_started.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(runner_started.load(Ordering::Acquire));
        let wrong_device = executor.cancel(SurfaceActionCancelRequest {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            request_id: queued.request_id.clone(),
            device_id: "device:other".to_owned(),
        });
        assert!(matches!(wrong_device, Err(SurfaceStoreError::Invalid(_))));
        let requested = executor
            .cancel(SurfaceActionCancelRequest {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                request_id: queued.request_id.clone(),
                device_id: "device-000-local".to_owned(),
            })
            .expect("cancel action from owning device");
        assert_eq!(requested.status, SurfaceActionStatus::CancelRequested);
        let started = Instant::now();
        loop {
            let record = instances
                .lock()
                .expect("lock Surface store")
                .get(&instance_id)
                .expect("Surface instance");
            let ack = record
                .event_acks
                .values()
                .find(|ack| ack.request_id == queued.request_id)
                .expect("cancelled action ack");
            if ack.status == SurfaceActionStatus::Cancelled {
                assert!(record.pending_events.is_empty());
                assert!(record.latest_result.is_none());
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(1));
            drop(record);
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replace_latest_cancels_the_old_request_and_only_commits_the_new_result() {
        let root = temp_root("replace-latest");
        let digest = "e".repeat(64);
        let mut tool = surface_tool(&digest);
        let action = &mut tool.metadata.as_mut().expect("Surface metadata")["capabilities"]
            ["surface"]["actions"][0];
        action["concurrency"] = json!("replace_latest");
        action["timeoutMs"] = json!(2000);
        let (registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool, "hook-node:replace");
        let first_started = Arc::new(AtomicBool::new(false));
        let worker_first_started = Arc::clone(&first_started);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
            let sequence = job
                .invocation
                .payload
                .get("sequence")
                .and_then(Value::as_u64)
                .expect("event sequence");
            if sequence == 1 {
                worker_first_started.store(true, Ordering::Release);
                while !job.cancellation.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {
                        "outputs": {"value": {"kind": "value", "value": sequence}},
                        "statePatch": {"value": sequence}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            registry,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            runner,
            2,
            4,
        )
        .expect("start replace-latest executor");
        let first = executor
            .submit(
                &instance_id,
                fixture_event(
                    &instance_id,
                    &attachment_id,
                    "event:replace-1",
                    json!({"sequence": 1}),
                ),
            )
            .expect("queue first action");
        for _ in 0..100 {
            if first_started.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(first_started.load(Ordering::Acquire));
        let second = executor
            .submit(
                &instance_id,
                fixture_event(
                    &instance_id,
                    &attachment_id,
                    "event:replace-2",
                    json!({"sequence": 2}),
                ),
            )
            .expect("queue replacement action");
        let started = Instant::now();
        loop {
            let record = instances
                .lock()
                .expect("lock Surface store")
                .get(&instance_id)
                .expect("Surface instance");
            let first_ack = record
                .event_acks
                .values()
                .find(|ack| ack.request_id == first.request_id)
                .expect("first ack");
            let second_ack = record
                .event_acks
                .values()
                .find(|ack| ack.request_id == second.request_id)
                .expect("second ack");
            if first_ack.status == SurfaceActionStatus::Cancelled
                && second_ack.status == SurfaceActionStatus::Succeeded
            {
                assert_eq!(record.authoritative_state["value"], 2);
                assert_eq!(
                    record
                        .latest_result
                        .as_ref()
                        .expect("latest result")
                        .outputs["value"],
                    loom_protocol::SurfacePortValue::Value { value: json!(2) }
                );
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            drop(record);
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }
