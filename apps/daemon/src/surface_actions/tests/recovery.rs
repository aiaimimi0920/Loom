// Daemon-restart recovery regression tests for persisted Surface actions.

    fn assert_running_action_recovers(confirmation: bool) {
        let case = if confirmation { "confirmed" } else { "plain" };
        let root = temp_root(&format!("recover-running-{case}"));
        let digest = "7".repeat(64);
        let mut tool = surface_tool(&digest);
        tool.metadata.as_mut().expect("Surface metadata")["capabilities"]["surface"]["actions"]
            [0]["confirmation"] = json!(confirmation);
        let (tool_registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(
                &root,
                tool,
                &format!("hook-node:recover-running-{case}"),
            );
        let event = fixture_event(
            &instance_id,
            &attachment_id,
            &format!("event:recover-running-{case}"),
            json!({}),
        );
        {
            let mut store = instances.lock().expect("lock Surface store");
            // A confirmed event moves to pending_events only after approval. Persisting that exact
            // post-approval shape models a daemon that exits after the worker records Running.
            let mut running = store
                .accept_event(&instance_id, event.clone())
                .expect("persist queued action");
            running.status = SurfaceActionStatus::Running;
            store
                .update_event_ack(running, false)
                .expect("persist pre-restart running ack");
        }

        let executions = Arc::new(AtomicUsize::new(0));
        let runner_executions = Arc::clone(&executions);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |_| {
            runner_executions.fetch_add(1, Ordering::SeqCst);
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {
                        "outputs": {
                            "value": {"kind": "value", "value": "recovered"}
                        }
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            tool_registry,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            runner,
            1,
            4,
        )
        .expect("start recovery executor");
        executor.recover_pending();

        let started = Instant::now();
        loop {
            let ack = instances
                .lock()
                .expect("lock Surface store")
                .event_ack(&instance_id, &event.event_id)
                .expect("recovered action ack");
            if ack.status == SurfaceActionStatus::Succeeded {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "persisted Running action never reached a terminal state after recovery"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert!(instances
            .lock()
            .expect("lock Surface store")
            .pending_events()
            .is_empty());
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_running_action_is_requeued_after_daemon_recovery() {
        assert_running_action_recovers(false);
    }

    #[test]
    fn an_approved_running_action_recovers_without_requesting_confirmation_again() {
        assert_running_action_recovers(true);
    }
