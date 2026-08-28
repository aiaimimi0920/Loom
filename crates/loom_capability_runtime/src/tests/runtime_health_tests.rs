use std::sync::Arc;
use std::thread;
use std::time::Duration;

use loom_protocol::CapabilityRuntimeStatus;
use serde_json::json;

use super::*;

#[test]
fn health_roundtrip_uses_the_owned_runtime_identity() {
    let root = temp_root("health");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    let package = package(&root, &executable, &[]);
    let digest = package.digest.clone();
    host.activate(package).expect("activate runtime");

    let health = host
        .health("publisher.example/fixture")
        .expect("runtime health");
    assert_eq!(health.plugin_id, "publisher.example/fixture");
    assert_eq!(health.package_digest, digest);
    assert_eq!(health.payload, Some(json!({ "ok": true })));

    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn concurrent_plugin_invocation_fails_fast_at_the_configured_limit() {
    let root = temp_root("admission");
    let executable = compile_fixture(&root);
    let host = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits {
        max_global_inflight: 1,
        max_plugin_inflight: 1,
        ..RuntimeHostLimits::default()
    }));
    host.activate(package(&root, &executable, &["hang"]))
        .expect("activate runtime");
    let invoking = Arc::clone(&host);
    let primary = thread::spawn(move || {
        invoking.invoke(CapabilityInvocation {
            request_id: "admission-primary".to_owned(),
            command_id: "publisher.example/fixture.run".to_owned(),
            input: json!({}),
            target: None,
            resource_refs: Vec::new(),
            staged_resources: Vec::new(),
            user_gesture_token: None,
            timeout: Some(Duration::from_secs(30)),
        })
    });
    for _ in 0..100 {
        if host.has_inflight_request("admission-primary") {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(host.has_inflight_request("admission-primary"));

    let error = invoke_fixture(&host, None, None).expect_err("limit must fail fast");
    assert!(matches!(error, CapabilityHostError::Busy));
    assert!(host
        .cancel_request("admission-primary")
        .expect("cancel primary"));
    assert!(primary.join().expect("join primary").is_err());
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn runtime_error_details_are_redacted_before_leaving_the_host() {
    let root = temp_root("redaction");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package(&root, &executable, &["fail-secret"]))
        .expect("activate runtime");

    let output = invoke_fixture(&host, None, None).expect("failed result envelope");
    assert_eq!(output.status, CapabilityRuntimeStatus::Failed);
    let error = output.error.expect("runtime error");
    assert_eq!(error.message, "capability runtime reported an error");
    assert!(!error.message.contains("fixture-secret"));

    host.deactivate_all();
    cleanup(&root);
}
