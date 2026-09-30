use std::sync::Arc;

use serde_json::json;

use super::*;

#[test]
fn idle_maintenance_stops_and_lazily_restarts_a_persistent_runtime() {
    let root = temp_root("idle-maintenance");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits {
        idle_timeout: Duration::from_millis(20),
        ..RuntimeHostLimits::default()
    });
    host.activate(package(&root, &executable, &[]))
        .expect("activate persistent runtime");
    assert_eq!(host.process_ids().len(), 1);

    thread::sleep(Duration::from_millis(40));
    assert_eq!(host.prune_idle().expect("prune idle runtime"), 1);
    assert!(host.process_ids().is_empty());

    invoke_fixture(&host, None, None).expect("lazy restart after idle shutdown");
    assert_eq!(host.process_ids().len(), 1);
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn idle_maintenance_leaves_an_in_flight_invocation_alone() {
    let root = temp_root("idle-inflight");
    let executable = compile_fixture(&root);
    let host = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits {
        idle_timeout: Duration::from_millis(20),
        ..RuntimeHostLimits::default()
    }));
    host.activate(package(&root, &executable, &["hang"]))
        .expect("activate runtime");
    let invoking = Arc::clone(&host);
    let primary = thread::spawn(move || {
        invoking.invoke(CapabilityInvocation {
            request_id: "idle-inflight-primary".to_owned(),
            command_id: "publisher.example/fixture.run".to_owned(),
            input: json!({}),
            target: None,
            resource_refs: Vec::new(),
            unit_attachments: Vec::new(),
            staged_resources: Vec::new(),
            user_gesture_token: None,
            timeout: Some(Duration::from_secs(30)),
        })
    });
    for _ in 0..100 {
        if host.has_inflight_request("idle-inflight-primary") {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(host.has_inflight_request("idle-inflight-primary"));

    // The command outlives the idle timeout by design; reaping its runtime here would kill a
    // request that is still being answered.
    thread::sleep(Duration::from_millis(40));
    assert_eq!(
        host.prune_idle().expect("prune with an in-flight request"),
        0
    );
    assert_eq!(host.process_ids().len(), 1);

    assert!(host
        .cancel_request("idle-inflight-primary")
        .expect("cancel primary"));
    assert!(primary.join().expect("join primary").is_err());
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn idle_maintenance_skips_a_plugin_that_is_still_initializing() {
    let root = temp_root("idle-startup");
    let executable = compile_fixture(&root);
    // These coordination files are outside the signed package bytes.
    let trigger = root.with_extension("slow");
    let started = trigger.with_extension("started");
    let host = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits {
        idle_timeout: Duration::from_millis(20),
        ..RuntimeHostLimits::default()
    }));
    host.activate(package(
        &root,
        &executable,
        &["slow-init", trigger.to_str().unwrap()],
    ))
    .unwrap();
    thread::sleep(Duration::from_millis(40));
    assert_eq!(host.prune_idle().unwrap(), 1);
    fs::write(&trigger, b"slow").unwrap();
    let invoking = Arc::clone(&host);
    let invocation = thread::spawn(move || invoke_fixture(&invoking, None, None));
    for _ in 0..300 {
        if started.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let entered = started.exists();
    let before = std::time::Instant::now();
    let result = host.prune_idle();
    let elapsed = before.elapsed();
    let outcome = invocation.join().unwrap();
    host.deactivate_all();
    cleanup(&root);
    let _ = fs::remove_file(trigger);
    let _ = fs::remove_file(started);
    assert!(entered, "fixture must enter initialize before maintenance");
    assert_eq!(result.unwrap(), 0);
    assert!(
        elapsed < Duration::from_millis(500),
        "maintenance took {elapsed:?}"
    );
    assert!(outcome.is_ok(), "{outcome:?}");
}
