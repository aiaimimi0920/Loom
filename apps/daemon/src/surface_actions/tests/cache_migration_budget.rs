// Manifest cache, package migration, and response budget regression tests.

    #[test]
    fn a_burst_of_events_reuses_one_parsed_surface_manifest() {
        let root = temp_root("manifest-cache");
        let digest = "a".repeat(64);
        let (tool_registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, surface_tool(&digest), "hook-node:cache");
        let resolves = Arc::new(AtomicUsize::new(0));
        let resolver_resolves = Arc::clone(&resolves);
        let resolver: Arc<SurfaceToolResolver> = Arc::new(move |descriptor| {
            resolver_resolves.fetch_add(1, Ordering::SeqCst);
            let tool = tool_registry
                .get_tool(&descriptor.art_id)
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
                .ok_or_else(|| SurfaceStoreError::NotFound(descriptor.art_id.clone()))?;
            validate_locked_tool(descriptor, &tool)?;
            Ok(tool)
        });
        let executor = SurfaceActionExecutor::new_with_components(
            resolver,
            instances,
            resources,
            hook_bridge,
            unused_runner(),
            1,
            8,
        )
        .expect("build Surface action executor");
        for index in 0..3 {
            executor
                .submit(
                    &instance_id,
                    fixture_event(
                        &instance_id,
                        &attachment_id,
                        &format!("event:cache-{index}"),
                        json!({}),
                    ),
                )
                .expect("submit Surface event");
        }
        assert_eq!(
            resolves.load(Ordering::SeqCst),
            3,
            "the package is still resolved per submit, because that is what re-checks installation and trust"
        );
        let cache = executor
            .manifest_cache
            .lock()
            .expect("lock the manifest cache");
        assert_eq!(
            cache.len(),
            1,
            "three events against one locked package parse one manifest"
        );
    }

    #[test]
    fn a_package_migration_during_resolve_makes_the_submit_prepare_again() {
        let root = temp_root("migrate-under-resolve");
        let first_digest = "a".repeat(64);
        let second_digest = "b".repeat(64);
        let (tool_registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(
                &root,
                surface_tool_at("1.0.0", &first_digest),
                "hook-node:migrate",
            );
        let resolves = Arc::new(AtomicUsize::new(0));
        let resolver_resolves = Arc::clone(&resolves);
        let resolver_instances = Arc::clone(&instances);
        let migrated_digest = second_digest.clone();
        let resolver: Arc<SurfaceToolResolver> = Arc::new(move |descriptor| {
            let tool = tool_registry
                .get_tool(&descriptor.art_id)
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
                .ok_or_else(|| SurfaceStoreError::NotFound(descriptor.art_id.clone()))?;
            validate_locked_tool(descriptor, &tool)?;
            // On the first resolve only, migrate the instance to a different package behind the
            // caller's back. That is exactly the race the re-validation exists for: this resolver
            // runs with no store lock held, so the descriptor it was handed can go stale.
            if resolver_resolves.fetch_add(1, Ordering::SeqCst) == 0 {
                tool_registry
                    .save_tool(surface_tool_at("2.0.0", &migrated_digest))
                    .expect("publish the migrated Art package");
                resolver_instances
                    .lock()
                    .expect("lock Surface store")
                    .migrate_instance(
                        &descriptor.instance_id,
                        None,
                        "2.0.0",
                        &migrated_digest,
                        1,
                        json!({"value": 0}),
                    )
                    .expect("migrate the instance mid-resolve");
            }
            Ok(tool)
        });
        let executor = SurfaceActionExecutor::new_with_components(
            resolver,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            unused_runner(),
            1,
            8,
        )
        .expect("build Surface action executor");
        // The migration bumps the instance generation, so the event this test submits is stale by the
        // time the second prepare reaches `accept_event`. That rejection is the point: it can only
        // come from a prepare that re-read the store after the resolve. A prepare that trusted its
        // first reading would have accepted the event against the package the instance has left.
        let error = executor
            .submit(
                &instance_id,
                fixture_event(&instance_id, &attachment_id, "event:migrate", json!({})),
            )
            .expect_err("the migrated instance rejects an event from the old generation");
        assert_eq!(
            resolves.load(Ordering::SeqCst),
            2,
            "the first prepare is discarded and the submit resolves the package the instance moved to"
        );
        assert!(
            matches!(&error, SurfaceStoreError::Conflict(message) if message.contains("stale")),
            "expected a staleness conflict from the fresh instance, got {error:?}"
        );
        assert_eq!(
            instances
                .lock()
                .expect("lock Surface store")
                .descriptor(&instance_id)
                .expect("instance is still there")
                .package_digest,
            second_digest,
            "the second prepare saw the package the instance ended up on"
        );
        let cache = executor
            .manifest_cache
            .lock()
            .expect("lock the manifest cache");
        assert_eq!(
            cache.len(),
            2,
            "each locked package identity gets its own manifest entry"
        );
    }

    /// Loom's first wire-size budget, and the reason `loom_perf` exists. It measures everything Loom
    /// pushes to a Hook client for one completed declarative action: the queued and succeeded acks,
    /// the committed patch, and the formal result. A regression that re-sends a whole snapshot where
    /// a patch would do, or serialises the same payload several times into one message, shows up
    /// here as a multiple of the budget rather than as a silent slowdown on a remote device.
    ///
    /// The ceiling is deliberately loose. It is not a benchmark of Loom's best case and it should
    /// not be tightened onto the measured number, because a legitimate new field would then break
    /// the build for a few dozen bytes.
    #[test]
    fn one_surface_action_stays_within_its_response_byte_budget() {
        // Measured at 1,665 bytes on 2026-08-22. The budget is about three times that, so ordinary
        // growth of the envelope is fine and an order-of-magnitude regression is not.
        const BUDGET_BYTES: u64 = 5_120;

        let root = temp_root("perf-response-bytes");
        let digest = "e".repeat(64);
        let (tool_registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, surface_tool(&digest), "hook-node:perf");
        let (rx, _subscription) = register_hook_bridge_subscription(
            &hook_bridge.lock().expect("lock bridge").broadcast_hub,
            loom_protocol::SURFACE_EVENT_METHODS
                .iter()
                .map(|method| (*method).to_owned())
                .collect(),
        );
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |_job| {
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "patches": [{
                        "operations": [{
                            "op": "set",
                            "nodeId": "refresh",
                            "path": "/props/label",
                            "value": "done"
                        }],
                        "statePatch": {"value": 1}
                    }],
                    "result": {
                        "outputs": {"value": {"kind": "value", "value": 1}},
                        "statePatch": {"value": 1}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            tool_registry,
            Arc::clone(&instances),
            resources,
            Arc::clone(&hook_bridge),
            runner,
            1,
            4,
        )
        .expect("start executor");
        executor
            .submit(
                &instance_id,
                fixture_event(&instance_id, &attachment_id, "event:perf-1", json!({})),
            )
            .expect("queue action");

        let mut broadcast_bytes = 0u64;
        let mut saw_patch = false;
        let mut saw_result = false;
        let mut saw_success = false;
        for _ in 0..8 {
            let message = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("Surface action broadcast");
            broadcast_bytes += message.len() as u64;
            let parsed: Value = serde_json::from_str(&message).expect("broadcast JSON");
            match parsed["method"].as_str() {
                Some(SURFACE_EVENT_PATCH) => saw_patch = true,
                Some(SURFACE_EVENT_RESULT) => saw_result = true,
                Some(SURFACE_EVENT_ACTION_ACK) if parsed["params"]["status"] == "succeeded" => {
                    saw_success = true;
                }
                _ => {}
            }
            if saw_patch && saw_result && saw_success {
                break;
            }
        }
        assert!(
            saw_patch && saw_result && saw_success,
            "the action has to run to completion before its wire cost means anything"
        );
        loom_perf::assert_within(
            "surface_action_response_bytes",
            "bytes",
            broadcast_bytes,
            BUDGET_BYTES,
        );

        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }
