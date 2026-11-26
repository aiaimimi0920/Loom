//! Real-process confirmation chain on the native runtime; approval never bypasses schema validation or pre-cancellation, and re-registering the same package cannot reuse a prior approval.
use super::*;
fn invocation() -> CapabilityInvocation {
    CapabilityInvocation {
        request_id: Uuid::new_v4().to_string(),
        command_id: "publisher.example/fixture.run".into(),
        input: json!({"text":"confirmed"}),
        target: None,
        resource_refs: vec![],
        unit_attachments: vec![],
        staged_resources: vec![],
        user_gesture_token: None,
        timeout: Some(Duration::from_secs(2)),
    }
}
#[test]
fn confirmed_command_keeps_original_gesture_gate_and_single_use_control() {
    let root = temp_root("confirmed-command");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package_with_gesture(&root, &executable, &[], true))
        .unwrap();
    assert!(host.invoke(invocation()).is_err());
    let scope = host.command_scope("publisher.example/fixture.run").unwrap();
    assert!(scope.requires_user_gesture);
    let control = Arc::new(InvocationControl::new());
    assert_eq!(
        host.invoke_confirmed_controlled(invocation(), control.clone(), &scope)
            .unwrap()
            .status,
        CapabilityRuntimeStatus::Succeeded
    );
    assert!(host
        .invoke_confirmed_controlled(invocation(), control, &scope)
        .is_err());
    let cancelled = Arc::new(InvocationControl::new());
    cancelled.cancel();
    assert!(matches!(
        host.invoke_confirmed_controlled(invocation(), cancelled, &scope),
        Err(CapabilityHostError::Cancelled)
    ));
    host.deactivate_all();
    cleanup(&root);
}
#[test]
fn confirmed_command_rejects_same_digest_new_scope_and_foreign_command() {
    let root = temp_root("confirmed-scope");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package_with_gesture(&root, &executable, &[], true))
        .unwrap();
    let old = host.command_scope("publisher.example/fixture.run").unwrap();
    host.deactivate(&old.plugin_id).unwrap();
    host.activate(package_with_gesture(&root, &executable, &[], true))
        .unwrap();
    let new = host.command_scope(&old.command_id).unwrap();
    assert_eq!(old.package_digest, new.package_digest);
    assert_ne!(old.scope_id, new.scope_id);
    assert!(host
        .invoke_confirmed_controlled(invocation(), Arc::new(InvocationControl::new()), &old)
        .is_err());
    let mut foreign = new.clone();
    foreign.command_id = "other".into();
    assert!(host
        .invoke_confirmed_controlled(invocation(), Arc::new(InvocationControl::new()), &foreign)
        .is_err());
    assert!(host
        .invoke_confirmed_controlled(invocation(), Arc::new(InvocationControl::new()), &new)
        .is_ok());
    host.deactivate_all();
    cleanup(&root);
}
