// Reservation guard and runner-thread reaping regression tests.
    fn panic_guard_job(
        tool: ToolDefinition,
        instance_id: &str,
        attachment_id: &str,
        event: &SurfaceEvent,
        action: SurfaceActionDefinition,
        ack: SurfaceActionAck,
        cancellation: Arc<AtomicBool>,
    ) -> SurfaceActionJob {
        SurfaceActionJob {
            event: event.clone(),
            ack: ack.clone(),
            action: action.clone(),
            tool,
            invocation: SurfaceActionInvocation {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.to_owned(),
                attachment_id: attachment_id.to_owned(),
                request_id: ack.request_id,
                event_id: event.event_id.clone(),
                action_id: action.id,
                event_class: event.class.clone(),
                generation: event.generation,
                base_revision: event.base_revision,
                payload: event.payload.clone(),
                authoritative_state: json!({"value": 0}),
            },
            cancellation,
        }
    }

    #[test]
    fn serial_lock_table_reuses_live_locks_and_prunes_inactive_keys() {
        let tool = surface_tool(&"8".repeat(64));
        let action = tool
            .surface_manifest()
            .expect("parse Surface manifest")
            .expect("Surface manifest")
            .actions
            .into_iter()
            .find(|action| action.id == "refresh_price")
            .expect("refresh action");
        let event = fixture_event(
            "instance:serial-lock",
            "attachment:serial-lock",
            "event:serial-lock",
            Value::Null,
        );
        let ack = SurfaceActionAck {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: event.instance_id.clone(),
            event_id: event.event_id.clone(),
            request_id: "request:serial-lock".to_owned(),
            accepted: true,
            status: SurfaceActionStatus::Running,
            error: None,
        };
        let job = panic_guard_job(
            tool,
            &event.instance_id,
            &event.attachment_id,
            &event,
            action,
            ack,
            Arc::new(AtomicBool::new(false)),
        );
        let coordinator = Arc::new(Mutex::new(SurfaceActionCoordinator::default()));

        let first = serial_lock(&coordinator, &job).expect("first serial lock");
        let shared = serial_lock(&coordinator, &job).expect("shared serial lock");
        assert!(Arc::ptr_eq(&first, &shared));
        assert_eq!(
            coordinator
                .lock()
                .expect("lock Surface action coordinator")
                .serial_locks
                .len(),
            1
        );

        let poisoned_lock = Arc::clone(&first);
        assert!(std::thread::spawn(move || {
            let _guard = poisoned_lock.lock().expect("lock serial mutex before panic");
            panic!("poison serial mutex");
        })
        .join()
        .is_err());
        let recovered_guard =
            acquire_serial_guard(Some(&first)).expect("recover poisoned serial mutex guard");
        drop(recovered_guard);
        drop(shared);
        drop(first);

        let mut replacement = job.clone();
        replacement.action.id = "refresh_price_replacement".to_owned();
        replacement.invocation.action_id = replacement.action.id.clone();
        let replacement_lock =
            serial_lock(&coordinator, &replacement).expect("replacement serial lock");
        let state = coordinator
            .lock()
            .expect("lock Surface action coordinator");
        assert_eq!(state.serial_locks.len(), 1);
        assert!(state.serial_locks.contains_key(&action_key(
            &replacement.event.instance_id,
            &replacement.action.id
        )));
        drop(state);
        drop(replacement_lock);

        let poisoned_coordinator = Arc::new(Mutex::new(SurfaceActionCoordinator::default()));
        let panic_coordinator = Arc::clone(&poisoned_coordinator);
        assert!(std::thread::spawn(move || {
            let _state = panic_coordinator
                .lock()
                .expect("lock coordinator before panic");
            panic!("poison Surface action coordinator");
        })
        .join()
        .is_err());
        assert!(
            serial_lock(&poisoned_coordinator, &job).is_some(),
            "a poisoned coordinator cannot silently disable Serial execution"
        );

        let mut reject_action = job.action.clone();
        reject_action.concurrency = SurfaceActionConcurrency::RejectWhileRunning;
        let reservation_request_id = request_id_for_event(&job.event.event_id);
        let cancellation = reserve_action(
            &poisoned_coordinator,
            &job.event.instance_id,
            &reject_action,
            &job.event,
        )
        .expect("reserve through a recovered coordinator");
        release_reservation(
            &poisoned_coordinator,
            &job.event.instance_id,
            &reject_action,
            Some(&reservation_request_id),
        );
        let second_cancellation = reserve_action(
            &poisoned_coordinator,
            &job.event.instance_id,
            &reject_action,
            &job.event,
        )
        .expect("released reservation can be acquired again");
        release_reservation(
            &poisoned_coordinator,
            &job.event.instance_id,
            &reject_action,
            Some(&reservation_request_id),
        );
        drop(cancellation);
        drop(second_cancellation);
    }

    #[test]
    fn a_panicking_job_body_releases_the_reservation_and_persists_a_failed_ack() {
        let root = temp_root("panic-guard");
        let digest = "e".repeat(64);
        let tool = surface_tool(&digest);
        let (_registry, instances, _resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool.clone(), "hook-node:panic");
        let mut action = tool
            .surface_manifest()
            .expect("read Surface manifest")
            .expect("Surface manifest")
            .actions
            .remove(0);
        // `RejectWhileRunning` is the mode a leaked reservation breaks permanently: the key stays in
        // the coordinator and every later invocation of the pair answers "is already running".
        action.concurrency = SurfaceActionConcurrency::RejectWhileRunning;
        let event = fixture_event(&instance_id, &attachment_id, "event:panic-1", json!({}));
        let coordinator = Arc::new(Mutex::new(SurfaceActionCoordinator::default()));
        let cancellation =
            reserve_action(&coordinator, &instance_id, &action, &event).expect("reserve action");
        let ack = instances
            .lock()
            .expect("lock Surface store")
            .accept_event(&instance_id, event.clone())
            .expect("accept event");
        let job = panic_guard_job(
            tool,
            &instance_id,
            &attachment_id,
            &event,
            action.clone(),
            ack,
            cancellation,
        );

        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = SurfaceActionJobGuard::new(&job, &instances, &hook_bridge, &coordinator);
            panic!("surface action body exploded");
        }));
        std::panic::set_hook(previous_hook);
        assert!(outcome.is_err(), "the guarded body panicked");

        assert!(
            coordinator
                .lock()
                .expect("lock coordinator")
                .reject_reservations
                .is_empty(),
            "an unwind must not leave the action reserved"
        );
        // A second invocation of the same pair is accepted again, which is what the leaked key used
        // to make impossible for the rest of the daemon's life.
        let next_event = fixture_event(&instance_id, &attachment_id, "event:panic-2", json!({}));
        reserve_action(&coordinator, &instance_id, &action, &next_event)
            .expect("reserve the action again after the panic");

        let persisted = instances
            .lock()
            .expect("lock Surface store")
            .event_ack(&instance_id, &event.event_id)
            .expect("persisted ack");
        assert_eq!(persisted.status, SurfaceActionStatus::Failed);
        assert_eq!(
            persisted.error.expect("failure error").code,
            "surface_action_panicked"
        );
        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("instance record");
        assert!(
            record.pending_events.is_empty(),
            "the panicked event is no longer pending"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_settled_job_guard_releases_without_synthesizing_a_failure() {
        let root = temp_root("settled-guard");
        let digest = "f".repeat(64);
        let tool = surface_tool(&digest);
        let (_registry, instances, _resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool.clone(), "hook-node:settled");
        let mut action = tool
            .surface_manifest()
            .expect("read Surface manifest")
            .expect("Surface manifest")
            .actions
            .remove(0);
        action.concurrency = SurfaceActionConcurrency::RejectWhileRunning;
        let event = fixture_event(&instance_id, &attachment_id, "event:settled-1", json!({}));
        let coordinator = Arc::new(Mutex::new(SurfaceActionCoordinator::default()));
        let cancellation =
            reserve_action(&coordinator, &instance_id, &action, &event).expect("reserve action");
        let ack = instances
            .lock()
            .expect("lock Surface store")
            .accept_event(&instance_id, event.clone())
            .expect("accept event");
        let job = panic_guard_job(
            tool,
            &instance_id,
            &attachment_id,
            &event,
            action,
            ack,
            cancellation,
        );
        {
            let mut guard =
                SurfaceActionJobGuard::new(&job, &instances, &hook_bridge, &coordinator);
            finish_cancelled(&job, &instances, &hook_bridge);
            guard.settle();
        }
        let persisted = instances
            .lock()
            .expect("lock Surface store")
            .event_ack(&instance_id, &event.event_id)
            .expect("persisted ack");
        assert_eq!(
            persisted.status,
            SurfaceActionStatus::Cancelled,
            "a settled guard leaves the terminal ack the body already wrote"
        );
        assert!(coordinator
            .lock()
            .expect("lock coordinator")
            .reject_reservations
            .is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_abandoned_runner_thread_is_joined_when_it_finishes_and_given_up_on_when_it_does_not() {
        // A runner that finishes shortly after the worker stopped waiting: the result channel is the
        // signal, and the join that follows it must actually run to completion.
        let (result_tx, result_rx) = mpsc::sync_channel::<Result<Value, SurfaceExecutionError>>(1);
        let ran_to_completion = Arc::new(AtomicBool::new(false));
        let thread_flag = Arc::clone(&ran_to_completion);
        let thread = std::thread::Builder::new()
            .name("loom-surface-runner-test-late".to_owned())
            .spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                let _ = result_tx.send(Ok(json!({})));
                thread_flag.store(true, Ordering::SeqCst);
            })
            .expect("spawn a late runner");
        reap_runner_thread(
            Some(thread),
            &result_rx,
            "request:late",
            true,
            Duration::from_millis(2_000),
        );
        assert!(
            ran_to_completion.load(Ordering::SeqCst),
            "a reclaimed runner is joined, not merely observed"
        );

        // A runner that ignores its budget cannot be stopped, so the wait has to end. It must last
        // at least the grace window and no longer than that plus a poll.
        let (result_tx, result_rx) = mpsc::sync_channel::<Result<Value, SurfaceExecutionError>>(1);
        let release = Arc::new(AtomicBool::new(false));
        let thread_release = Arc::clone(&release);
        let thread = std::thread::Builder::new()
            .name("loom-surface-runner-test-stuck".to_owned())
            .spawn(move || {
                while !thread_release.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let _ = result_tx.send(Ok(json!({})));
            })
            .expect("spawn a stuck runner");
        let started = Instant::now();
        reap_runner_thread(
            Some(thread),
            &result_rx,
            "request:stuck",
            true,
            Duration::from_millis(120),
        );
        let waited = started.elapsed();
        release.store(true, Ordering::SeqCst);
        assert!(
            waited >= Duration::from_millis(120),
            "the grace window is honoured: {waited:?}"
        );
        assert!(
            waited < Duration::from_secs(2),
            "giving up on a stuck runner is bounded: {waited:?}"
        );

        // The normal path already holds the result, so no grace window is spent at all.
        let (result_tx, result_rx) = mpsc::sync_channel::<Result<Value, SurfaceExecutionError>>(1);
        let ran_to_completion = Arc::new(AtomicBool::new(false));
        let thread_flag = Arc::clone(&ran_to_completion);
        let thread = std::thread::Builder::new()
            .name("loom-surface-runner-test-done".to_owned())
            .spawn(move || {
                let _ = result_tx.send(Ok(json!({})));
                thread_flag.store(true, Ordering::SeqCst);
            })
            .expect("spawn a finished runner");
        let started = Instant::now();
        reap_runner_thread(
            Some(thread),
            &result_rx,
            "request:done",
            false,
            Duration::from_millis(2_000),
        );
        assert!(ran_to_completion.load(Ordering::SeqCst));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "a runner that already returned is joined immediately"
        );
    }

    /// A runner for tests that only exercise the submit prologue. The job never has to produce a
    /// result, so failing immediately keeps the worker out of the way of the assertions.
    fn unused_runner() -> Arc<SurfaceActionRunner> {
        Arc::new(|_job| {
            Err(execution_error(
                "test_runner_unused",
                "this test does not exercise the runner",
            ))
        })
    }
