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

    let external_root = temp_root("external-url-effect");
    let external_executable = compile_fixture(&external_root);
    let external_host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    external_host
        .activate(package_with_contract(
            &external_root,
            &external_executable,
            &["external-url"],
            true,
            None,
            None,
            &["hook.external.open"],
            1,
        ))
        .expect("activate external URL fixture");
    let target = UserGestureTarget {
        unit_id: "unit-1".to_owned(),
        revision: 1,
    };
    let token = external_host
        .issue_user_gesture("publisher.example/fixture.run", Some(target))
        .expect("external URL gesture token");
    external_host
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
        .expect("gesture-authorized external URL effect");
    external_host.deactivate_all();
    cleanup(&external_root);
}

#[test]
fn opaque_resource_references_require_matching_host_staging() {
    let root = temp_root("resource-staging-required");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package(&root, &executable, &[]))
        .expect("activate resource fixture");
    let digest = "1".repeat(64);
    let mut request = invocation(json!({}), None);
    request
        .resource_refs
        .push(loom_protocol::ExtensionResourceRef {
            resource_id: format!("sha256:{digest}"),
            kind: loom_protocol::ExtensionResourceKind::File,
            digest,
            byte_length: 1,
            lease_id: "lease:fixture-resource".to_owned(),
        });

    let error = host
        .invoke(request.clone())
        .expect_err("unstaged resource must fail closed");
    assert!(matches!(error, CapabilityHostError::Protocol(_)));
    let staged_path = root.with_extension("staged.bin");
    fs::write(&staged_path, b"x").unwrap();
    let mut permissions = fs::metadata(&staged_path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&staged_path, permissions).unwrap();
    request.staged_resources.push(CapabilityStagedResource {
        resource_ref: request.resource_refs[0].clone(),
        staged_path: staged_path.canonicalize().unwrap(),
    });
    host.invoke(request)
        .expect("matching staged resource succeeds");
    host.deactivate_all();
    let mut permissions = fs::metadata(&staged_path).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&staged_path, permissions).unwrap();
    fs::remove_file(staged_path).unwrap();
    cleanup(&root);
}

#[test]
fn shared_images_require_image_read_permission() {
    let denied_root = temp_root("image-resource-permission-denied");
    let denied_executable = compile_fixture(&denied_root);
    let denied_host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    denied_host
        .activate(package(&denied_root, &denied_executable, &[]))
        .expect("activate denied fixture");
    let (denied_request, denied_path) = staged_image_invocation(&denied_root);
    let error = denied_host
        .invoke(denied_request)
        .expect_err("undeclared image read must fail closed");
    assert!(error.to_string().contains("hook.unit.image.read"));
    denied_host.deactivate_all();
    remove_read_only_file(&denied_path);
    cleanup(&denied_root);

    let allowed_root = temp_root("image-resource-permission-allowed");
    let allowed_executable = compile_fixture(&allowed_root);
    let allowed_host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    allowed_host
        .activate(package_with_contract(
            &allowed_root,
            &allowed_executable,
            &[],
            false,
            None,
            None,
            &["hook.unit.image.read"],
            1,
        ))
        .expect("activate permitted fixture");
    let (allowed_request, allowed_path) = staged_image_invocation(&allowed_root);
    allowed_host
        .invoke(allowed_request)
        .expect("declared image read succeeds");
    allowed_host.deactivate_all();
    remove_read_only_file(&allowed_path);
    cleanup(&allowed_root);
}

#[test]
fn unit_attachments_require_attachment_read_permission() {
    let denied_root = temp_root("attachment-read-permission-denied");
    let denied_executable = compile_fixture(&denied_root);
    let denied_host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    denied_host
        .activate(package(&denied_root, &denied_executable, &[]))
        .expect("activate denied fixture");
    let error = denied_host
        .invoke(invocation_with_attachment())
        .expect_err("undeclared attachment read must fail closed");
    assert!(error.to_string().contains("hook.unit.attachments.read"));
    denied_host.deactivate_all();
    cleanup(&denied_root);

    let allowed_root = temp_root("attachment-read-permission-allowed");
    let allowed_executable = compile_fixture(&allowed_root);
    let allowed_host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    allowed_host
        .activate(package_with_contract(
            &allowed_root,
            &allowed_executable,
            &[],
            false,
            None,
            None,
            &["hook.unit.attachments.read"],
            1,
        ))
        .expect("activate permitted fixture");
    allowed_host
        .invoke(invocation_with_attachment())
        .expect("declared attachment read succeeds");
    let mut foreign_attachment = invocation_with_attachment();
    foreign_attachment.unit_attachments[0].plugin_id = "publisher.other/fixture".to_owned();
    let error = allowed_host
        .invoke(foreign_attachment)
        .expect_err("cross-plugin attachment read must fail closed");
    assert!(error
        .to_string()
        .contains("outside the invoking plugin namespace"));
    allowed_host.deactivate_all();
    cleanup(&allowed_root);
}

#[test]
fn attachment_effect_cannot_publish_a_resource_outside_its_invocation() {
    let root = temp_root("attachment-resource-ownership");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package_with_contract(
        &root,
        &executable,
        &["attachment-forged"],
        false,
        None,
        None,
        &["hook.unit.attachments.write"],
        1,
    ))
    .expect("activate attachment fixture");

    let error = host
        .invoke(invocation(json!({}), None))
        .expect_err("forged attachment resource must fail closed");
    assert!(error.to_string().contains("outside its invocation"));
    host.deactivate_all();
    cleanup(&root);
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
        unit_attachments: Vec::new(),
        staged_resources: Vec::new(),
        user_gesture_token,
        timeout: Some(Duration::from_secs(2)),
    }
}

fn staged_image_invocation(root: &Path) -> (CapabilityInvocation, PathBuf) {
    let digest = "2".repeat(64);
    let resource_ref = loom_protocol::ExtensionResourceRef {
        resource_id: format!("sha256:{digest}"),
        kind: loom_protocol::ExtensionResourceKind::SharedImage,
        digest,
        byte_length: 1,
        lease_id: "lease:fixture-image".to_owned(),
    };
    let path = root.with_extension("staged-image.bin");
    fs::write(&path, b"x").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let mut request = invocation(json!({}), None);
    request.resource_refs.push(resource_ref.clone());
    request.staged_resources.push(CapabilityStagedResource {
        resource_ref,
        staged_path: path.canonicalize().unwrap(),
    });
    (request, path)
}

fn invocation_with_attachment() -> CapabilityInvocation {
    let mut request = invocation(json!({}), None);
    request
        .unit_attachments
        .push(loom_protocol::ExtensionUnitAttachment {
            attachment_id: "publisher.example/fixture.result".to_owned(),
            type_id: "publisher.example/fixture.result.v1".to_owned(),
            schema_version: "1".to_owned(),
            revision: 1,
            plugin_id: "publisher.example/fixture".to_owned(),
            plugin_version: "1.0.0".to_owned(),
            renderer_id: None,
            payload: json!({ "text": "fixture" }),
            resource_refs: Vec::new(),
        });
    request
}

fn remove_read_only_file(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).unwrap();
    fs::remove_file(path).unwrap();
}
