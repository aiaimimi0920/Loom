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
    fs::remove_dir_all(root).expect("cleanup upload fixture");
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
    fs::remove_dir_all(root).expect("cleanup upload permission fixture");
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
        plugins: Vec::new(),
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
