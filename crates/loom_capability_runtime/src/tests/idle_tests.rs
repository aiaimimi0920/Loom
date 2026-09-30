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
