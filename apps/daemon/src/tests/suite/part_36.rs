// Capability extension resource-upload boundary coverage.
#[test]
fn extension_image_upload_becomes_an_invocation_scoped_resource() {
    let root = unique_temp_dir("extension-image-upload");
    let resources = Arc::new(Mutex::new(
        SurfaceResourceStore::new(root.join("surface-resources")).expect("resource store"),
    ));
    let mut invocation = extension_upload_invocation();
    let snapshot = extension_upload_snapshot(&[EXTENSION_IMAGE_READ_PERMISSION]);
    let upload = ExtensionResourceUpload {
        kind: SurfaceResourceKind::Image,
        mime: "image/png".to_owned(),
        data_base64: format!("data:image/png;base64,{}", BASE64.encode(b"png-fixture")),
    };

    let lease =
        stage_extension_resource_uploads(vec![upload], &mut invocation, &snapshot, &resources)
            .expect("stage image upload");
    assert_eq!(invocation.resource_refs.len(), 1);
    let resource = invocation.resource_refs[0].clone();
    assert_eq!(resource.kind, ExtensionResourceKind::SharedImage);
    resources
        .lock()
        .unwrap()
        .get_with_lease(&resource.digest, &resource.lease_id)
        .expect("lease remains valid while invocation is active");

    drop(lease);
    assert!(resources
        .lock()
        .unwrap()
        .get_with_lease(&resource.digest, &resource.lease_id)
        .is_err());
    remove_test_dir(&root);
}

#[test]
fn extension_image_upload_requires_the_command_permission() {
    let root = unique_temp_dir("extension-image-upload-permission");
    let resources = Arc::new(Mutex::new(
        SurfaceResourceStore::new(root.join("surface-resources")).expect("resource store"),
    ));
    let mut invocation = extension_upload_invocation();
    let upload = ExtensionResourceUpload {
        kind: SurfaceResourceKind::Image,
        mime: "image/png".to_owned(),
        data_base64: BASE64.encode(b"png-fixture"),
    };

    let error = match stage_extension_resource_uploads(
        vec![upload],
        &mut invocation,
        &extension_upload_snapshot(&[]),
        &resources,
    ) {
        Err(error) => error,
        Ok(_) => panic!("undeclared image read must fail closed"),
    };
    assert_eq!(error, ExtensionResourceUploadError::PermissionDenied);
    assert!(invocation.resource_refs.is_empty());
    remove_test_dir(&root);
}

fn extension_upload_invocation() -> ExtensionInvocation {
    ExtensionInvocation {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        request_id: "upload-request-1".to_owned(),
        plugin_id: "publisher.example/plugin".to_owned(),
        command_id: "publisher.example/plugin.read-image".to_owned(),
        snapshot_generation: 1,
        target: ExtensionTarget {
            unit_id: "unit-1".to_owned(),
            revision: 2,
        },
        input: json!({}),
        resource_refs: Vec::new(),
        unit_attachments: Vec::new(),
        user_gesture_token: None,
    }
}

#[test]
fn extension_image_upload_rejects_untrusted_or_ungranted_binding() {
    let invocation = extension_upload_invocation();
    let mut snapshot = extension_upload_snapshot(&[EXTENSION_IMAGE_READ_PERMISSION]);
    assert!(command_allows_image_upload(&snapshot, &invocation));
    snapshot.plugins[0].effective_permissions.clear();
    assert!(!command_allows_image_upload(&snapshot, &invocation));
    snapshot.plugins[0].effective_permissions = vec![EXTENSION_IMAGE_READ_PERMISSION.to_owned()];
    for status in [loom_protocol::ExtensionTrustStatus::Revoked,
        loom_protocol::ExtensionTrustStatus::Untrusted, loom_protocol::ExtensionTrustStatus::UnsignedDeveloper] {
        snapshot.plugins[0].trust_status = status;
        assert!(!command_allows_image_upload(&snapshot, &invocation));
    }
}

fn assert_extension_authorization_lifecycle(root: &Path, runtime: &SharedCapabilityRuntime) {
    let snapshot = runtime.contribution_snapshot().unwrap();
    let plugin = &snapshot.plugins[0];
    let command = &snapshot.contributions.commands[0];
    let mut state = ExtensionConnectionState {
        control_plane_root: Some(root.to_path_buf()), ..ExtensionConnectionState::default()
    };
    state.begin_extension_session("security-session".to_owned(), &[
        loom_protocol::EXTENSION_FEATURE_RESOURCE_AUTHORIZATION.to_owned(),
        loom_protocol::EXTENSION_FEATURE_COMMANDS.to_owned(),
    ]);
    let request = loom_protocol::ExtensionCommandAuthorizeRequest {
        request_id: "security-preflight".to_owned(), session_id: "security-session".to_owned(),
        plugin_id: plugin.id.clone(), command_id: command.id.clone(),
        snapshot_generation: snapshot.generation,
        target: ExtensionTarget { unit_id: "security-unit".to_owned(), revision: 1 }, check_only: false,
    };
    let mut invocation = extension_upload_invocation();
    invocation.plugin_id = plugin.id.clone();
    invocation.command_id = command.id.clone();
    invocation.target = request.target.clone();
    invocation.snapshot_generation = snapshot.generation;
    let authorize = |state: &mut ExtensionConnectionState, request: loom_protocol::ExtensionCommandAuthorizeRequest| {
        let result = handle_extension_authorization(request, state, runtime);
        serde_json::from_str::<ExtensionBridgeResponse>(&result.response).unwrap()
    };
    let first = authorize(&mut state, request.clone());
    assert_eq!(first.status, ExtensionBridgeStatus::Succeeded);
    let first_id = first.data["authorizationId"].as_str().unwrap();
    assert!(consume_extension_authorization(&mut state, Some(first_id), &invocation, &snapshot));
    assert!(!consume_extension_authorization(&mut state, Some(first_id), &invocation, &snapshot));
    let wrong_target = authorize(&mut state, request.clone());
    invocation.target.revision += 1;
    assert!(!consume_extension_authorization(&mut state, wrong_target.data["authorizationId"].as_str(), &invocation, &snapshot));
    invocation.target.revision -= 1;
    let expired = authorize(&mut state, request.clone());
    let expired_id = expired.data["authorizationId"].as_str().unwrap();
    state.resource_authorizations.get_mut(expired_id).unwrap().issued -= EXTENSION_AUTHORIZATION_TTL;
    assert!(!consume_extension_authorization(&mut state, Some(expired_id), &invocation, &snapshot));
    let check = authorize(&mut state, loom_protocol::ExtensionCommandAuthorizeRequest { check_only: true, ..request.clone() });
    assert_eq!(check.data["authorized"], true);
    assert!(state.resource_authorizations.is_empty());
    let before_revoke = authorize(&mut state, request.clone());
    let grants = loom_tool_registry::capability::CapabilityGrantStore::new(root);
    let grant = grants.list().unwrap().into_iter().find(|grant| grant.qualified_id == plugin.id).unwrap();
    grants.revoke_plugin(&plugin.id).unwrap();
    assert_eq!(runtime.generation(), snapshot.generation, "test revokes without a snapshot update");
    assert!(!consume_extension_authorization(&mut state, before_revoke.data["authorizationId"].as_str(), &invocation, &snapshot));
    assert_eq!(authorize(&mut state, request.clone()).status, ExtensionBridgeStatus::Failed);
    grants.grant(&grant.qualified_id, &grant.package_digest, &grant.permissions).unwrap();
    let fresh = authorize(&mut state, request);
    assert_eq!(fresh.status, ExtensionBridgeStatus::Succeeded);
    state.begin_extension_session("replacement".to_owned(), &[]);
    assert!(state.resource_authorizations.is_empty());
    assert!(!consume_extension_authorization(&mut state, fresh.data["authorizationId"].as_str(), &invocation, &snapshot));
}

fn extension_upload_snapshot(permissions: &[&str]) -> ContributionSnapshot {
    let contribution = loom_protocol::ExtensionContribution {
        id: "publisher.example/plugin.read-image".to_owned(),
        plugin_id: "publisher.example/plugin".to_owned(),
        scope_id: "scope-1".to_owned(),
        title: Some("Read image".to_owned()),
        command_id: Some("publisher.example/plugin.read-image".to_owned()),
        when: None,
        placement: None,
        order: None,
        payload: json!({ "permissions": permissions }),
    };
    ContributionSnapshot {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        generation: 1,
        plugins: vec![loom_protocol::ExtensionPluginBinding {
            id: "publisher.example/plugin".to_owned(), version: "1.0.0".to_owned(),
            package_digest: "a".repeat(64), permission_grant_digest: "b".repeat(64),
            trust_status: loom_protocol::ExtensionTrustStatus::Trusted,
            effective_permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
            scope_id: "scope-1".to_owned(),
        }],
        contributions: loom_protocol::ExtensionContributions {
            commands: vec![contribution],
            shortcuts: Vec::new(),
            menus: Vec::new(),
            settings: Vec::new(),
            data_types: Vec::new(),
            renderers: Vec::new(),
            unit_overlays: Vec::new(),
            background_tasks: Vec::new(),
            resource_providers: Vec::new(),
            diagnostics: Vec::new(),
            event_subscriptions: Vec::new(),
        },
    }
}
