//! Cancellation, progress, and terminal-state regressions against a real framed runtime, without relying on inflight polling to avoid pre-cancellation races.
use super::*;
use std::time::Instant;

fn invocation() -> CapabilityInvocation {
    CapabilityInvocation {
        request_id: "controlled-call".into(),
        command_id: "publisher.example/fixture.run".into(),
        input: json!({}),
        target: None,
        resource_refs: vec![],
        unit_attachments: vec![],
        staged_resources: vec![],
        user_gesture_token: None,
        timeout: Some(Duration::from_secs(5)),
    }
}

#[test]
fn pre_cancel_is_single_use_and_does_not_dispatch() {
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    let control = Arc::new(InvocationControl::new());
    assert!(control.cancel());
    assert!(matches!(
        host.invoke_controlled(invocation(), control.clone()),
        Err(CapabilityHostError::Cancelled)
    ));
    assert!(matches!(
        host.invoke_controlled(invocation(), control.clone()),
        Err(CapabilityHostError::Protocol(_))
    ));
    assert!(!control.cancel());
    assert!(host.process_ids().is_empty());
}

#[test]
fn pre_cancel_preserves_an_unclaimed_persistent_runtime() {
    let root = temp_root("control-prewarmed");
    let exe = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package(&root, &exe, &["progress"])).unwrap();
    let pids = host.process_ids();
    assert_eq!(pids.len(), 1);
    let control = Arc::new(InvocationControl::new());
    assert!(control.cancel());
    assert!(matches!(
        host.invoke_controlled(invocation(), control.clone()),
        Err(CapabilityHostError::Cancelled)
    ));
    assert!(control.progress().is_none());
    assert_eq!(host.process_ids(), pids);
    // A device-owned cancellation must not kill a shared process it never used.
    let next = Arc::new(InvocationControl::new());
    host.invoke_controlled(invocation(), next.clone()).unwrap();
    assert!(next.progress().is_some());
    assert_eq!(host.process_ids(), pids);
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn cancel_during_lazy_initialize_reaps_only_the_new_process_before_dispatch() {
    let root = temp_root("control-initialize");
    let exe = compile_fixture(&root);
    let trigger = root.with_extension("slow");
    let started = trigger.with_extension("started");
    let command = trigger.with_extension("command");
    let host = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits::default()));
    host.activate(package(
        &root,
        &exe,
        &["cancel-init", trigger.to_str().unwrap()],
    ))
    .unwrap();
    host.cancel("publisher.example/fixture").unwrap();
    assert!(host.process_ids().is_empty());
    fs::write(&trigger, b"slow").unwrap();
    let control = Arc::new(InvocationControl::new());
    let invoking = host.clone();
    let worker_control = control.clone();
    let worker = thread::spawn(move || invoking.invoke_controlled(invocation(), worker_control));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !started.exists() && !worker.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    let entered = started.exists();
    let cancelled = control.cancel();
    let before_join = Instant::now();
    let result = worker.join().unwrap();
    assert!(entered, "must cancel inside real lazy initialization");
    assert!(cancelled);
    assert!(
        matches!(result, Err(CapabilityHostError::Cancelled)),
        "{result:?}"
    );
    assert!(before_join.elapsed() < Duration::from_secs(3));
    assert!(
        !command.exists(),
        "cancelled initialization must not dispatch Command"
    );
    assert!(!host.has_inflight_request("controlled-call"));
    assert!(
        host.process_ids().is_empty(),
        "cancelled initialization retained its child"
    );
    // Explicit cancellation is not a crash or a reason to block the next invocation.
    fs::remove_file(&trigger).unwrap();
    host.invoke_controlled(invocation(), Arc::new(InvocationControl::new()))
        .unwrap();
    assert!(command.exists());
    host.deactivate_all();
    cleanup(&root);
    fs::remove_file(started).unwrap();
    fs::remove_file(command).unwrap();
}

#[test]
fn progress_is_real_and_final_output_remains_authoritative() {
    let root = temp_root("control-progress");
    let exe = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package(&root, &exe, &["progress"])).unwrap();
    let control = Arc::new(InvocationControl::new());
    let result = host
        .invoke_controlled(invocation(), control.clone())
        .unwrap();
    assert_eq!(result.status, CapabilityRuntimeStatus::Succeeded);
    assert_eq!(result.payload, Some(json!({"ok":true})));
    let snapshot = control.progress().unwrap();
    assert_eq!(snapshot.sequence, 2);
    assert_eq!(snapshot.status, CapabilityRuntimeStatus::Progress);
    assert_eq!(snapshot.payload, Some(json!({"step":1})));
    assert!(!control.cancel());
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn cancel_after_progress_reaps_process_and_does_not_trip_crash_circuit() {
    let root = temp_root("control-cancel");
    let exe = compile_fixture(&root);
    let host = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits::default()));
    host.activate(package(&root, &exe, &["progress-hang"]))
        .unwrap();
    // Four explicit cancellations must not be counted as repeated runtime faults.
    for _ in 0..4 {
        let control = Arc::new(InvocationControl::new());
        let worker_host = host.clone();
        let worker_control = control.clone();
        let worker =
            thread::spawn(move || worker_host.invoke_controlled(invocation(), worker_control));
        let deadline = Instant::now() + Duration::from_secs(4);
        while control.progress().is_none() && Instant::now() < deadline && !worker.is_finished() {
            thread::sleep(Duration::from_millis(5));
        }
        let observed = control.progress();
        assert!(control.cancel());
        let result = worker.join().unwrap();
        assert!(
            observed.is_some(),
            "must observe a real runtime progress frame: {result:?}"
        );
        assert!(matches!(result, Err(CapabilityHostError::Cancelled)));
        assert!(host.process_ids().is_empty());
        assert!(!control.cancel());
    }
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn hostile_progress_is_bounded_and_reaps_process() {
    let root = temp_root("control-limits");
    let exe = compile_fixture(&root);
    for mode in ["progress-large", "progress-flood"] {
        let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
        host.activate(package(&root, &exe, &[mode])).unwrap();
        let control = Arc::new(InvocationControl::new());
        assert!(host
            .invoke_controlled(invocation(), control.clone())
            .is_err());
        assert!(host.process_ids().is_empty());
        assert!(control
            .progress()
            .is_none_or(|snapshot| snapshot.sequence <= 1024));
        assert!(!control.cancel());
        host.deactivate_all();
    }
    cleanup(&root);
}

#[test]
fn controller_attachment_covers_cancel_before_and_after_registration() {
    for before in [false, true] {
        let control = InvocationControl::new();
        let _guard = control.begin().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        if before {
            assert!(control.cancel());
            assert!(matches!(
                control.attach(sender),
                Err(CapabilityHostError::Cancelled)
            ));
        } else {
            control.attach(sender).unwrap();
            assert!(control.cancel());
            assert!(matches!(
                receiver.recv_timeout(Duration::from_secs(1)),
                Ok(Err(CapabilityHostError::Cancelled))
            ));
        }
        assert!(matches!(
            control.complete(),
            Err(CapabilityHostError::Cancelled)
        ));
        assert!(!control.cancel());
    }
}

#[test]
fn latest_progress_slot_is_bounded_and_frozen_at_completion() {
    let control = InvocationControl::new();
    let _guard = control.begin().unwrap();
    for step in 0..1024 {
        assert!(control.publish(CapabilityRuntimeStatus::Progress, &Some(json!(step))));
    }
    assert!(!control.publish(
        CapabilityRuntimeStatus::Progress,
        &Some(json!("x".repeat(4097)))
    ));
    assert_eq!(control.progress().unwrap().sequence, 1024);
    control.complete().unwrap();
    assert!(control.publish(CapabilityRuntimeStatus::Progress, &Some(json!(9999))));
    assert_eq!(control.progress().unwrap().payload, Some(json!(1023)));
    assert!(!control.cancel());
}
