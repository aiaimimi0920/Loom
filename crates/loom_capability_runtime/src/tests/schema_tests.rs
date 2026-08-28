use super::*;

#[test]
fn signed_input_and_output_schemas_are_enforced() {
    let root = temp_root("schemas");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package_with_contract(
        &root,
        &executable,
        &["bad-output"],
        false,
        Some(json!({
            "type": "object",
            "required": ["value"],
            "properties": { "value": { "type": "integer" } },
            "additionalProperties": false
        })),
        Some(json!({
            "type": "object",
            "required": ["ok"],
            "properties": { "ok": { "type": "boolean" } },
            "additionalProperties": false
        })),
        &[],
        1,
    ))
    .expect("activate schema fixture");
    let bad_input = host
        .invoke(invocation(json!({ "value": "wrong" }), None))
        .expect_err("input schema mismatch");
    assert!(bad_input.to_string().contains("input"));
    let bad_output = host
        .invoke(invocation(json!({ "value": 7 }), None))
        .expect_err("output schema mismatch");
    assert!(bad_output.to_string().contains("output"));
    host.deactivate_all();
    cleanup(&root);
}

#[test]
fn effects_require_command_permission_and_a_real_gesture() {
    let notice_root = temp_root("notice-effect");
    let notice_executable = compile_fixture(&notice_root);
    let notice_host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    notice_host
        .activate(package_with_contract(
            &notice_root,
            &notice_executable,
            &["notice"],
            false,
            None,
            None,
            &["hook.notice.show"],
            1,
        ))
        .expect("activate notice fixture");
    notice_host
        .invoke(invocation(json!({}), None))
        .expect("declared notice effect");
    notice_host.deactivate_all();
    cleanup(&notice_root);

    let clipboard_root = temp_root("clipboard-effect");
    let clipboard_executable = compile_fixture(&clipboard_root);
    let clipboard_host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    clipboard_host
        .activate(package_with_contract(
            &clipboard_root,
            &clipboard_executable,
            &["clipboard"],
            true,
            None,
            None,
            &["hook.clipboard.write"],
            1,
        ))
        .expect("activate clipboard fixture");
    let target = UserGestureTarget {
        unit_id: "unit-1".to_owned(),
        revision: 1,
    };
    let token = clipboard_host
        .issue_user_gesture("publisher.example/fixture.run", Some(target))
        .expect("gesture token");
    clipboard_host
        .invoke(invocation(
            json!({}),
            Some((
                ExtensionTarget {
                    unit_id: "unit-1".to_owned(),
                    revision: 1,
                },
                token,
            )),
        ))
        .expect("gesture-authorized clipboard effect");
    clipboard_host.deactivate_all();
    cleanup(&clipboard_root);
}

fn invocation(
    input: serde_json::Value,
    gesture: Option<(ExtensionTarget, String)>,
) -> CapabilityInvocation {
    let (target, user_gesture_token) = gesture
        .map(|(target, token)| (Some(target), Some(token)))
        .unwrap_or_default();
    CapabilityInvocation {
        request_id: format!("schema-{}", Uuid::new_v4().simple()),
        command_id: "publisher.example/fixture.run".to_owned(),
        input,
        target,
        resource_refs: Vec::new(),
        user_gesture_token,
        timeout: Some(Duration::from_secs(2)),
    }
}
