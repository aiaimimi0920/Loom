// Generation CAS, failure publication, and migration reset regression coverage.
#[test]
fn stale_generation_cannot_replace_preview_or_formal_result() {
    let path = temp_path("generation");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let record = create(&mut store);
    let instance_id = record.descriptor.instance_id.clone();
    let generation = store
        .begin_generation(&instance_id, Some(0))
        .expect("begin generation")
        .generation;
    let resource = SurfaceResourceDescriptor {
        resource_id: format!("sha256:{}", "c".repeat(64)),
        kind: SurfaceResourceKind::Image,
        mime: "image/webp".to_owned(),
        size: 10,
        width: Some(2),
        height: Some(2),
    };
    store
        .commit_preview(
            &instance_id,
            SurfacePreviewCommit {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                request_id: "request:one".to_owned(),
                generation,
                preview_revision: 1,
                port_id: "preview".to_owned(),
                value: SurfacePortValue::Resource {
                    resource: resource.clone(),
                },
            },
        )
        .expect("commit preview");
    store
        .begin_generation(&instance_id, Some(generation))
        .expect("begin newer generation");
    let result = SurfaceResultCommit {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: instance_id.clone(),
        request_id: "request:one".to_owned(),
        generation,
        result_revision: 1,
        outputs: BTreeMap::from([("output".to_owned(), SurfacePortValue::Resource { resource })]),
        state_patch: Value::Null,
    };
    assert!(matches!(
        store.commit_result(&instance_id, result),
        Err(SurfaceStoreError::Conflict(_))
    ));
    assert!(store
        .get(&instance_id)
        .expect("record")
        .latest_result
        .is_none());
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn failure_preserves_last_successful_atomic_result() {
    let path = temp_path("failure");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let record = create(&mut store);
    let instance_id = record.descriptor.instance_id;
    let generation = store
        .begin_generation(&instance_id, None)
        .expect("begin generation")
        .generation;
    store
        .commit_result(
            &instance_id,
            SurfaceResultCommit {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                request_id: "request:success".to_owned(),
                generation,
                result_revision: 1,
                outputs: BTreeMap::from([(
                    "price".to_owned(),
                    SurfacePortValue::Value {
                        value: serde_json::json!(100),
                    },
                )]),
                state_patch: serde_json::json!({"price": 100}),
            },
        )
        .expect("commit result");
    let generation = store
        .begin_generation(&instance_id, Some(generation))
        .expect("begin failing generation")
        .generation;
    let failed = store
        .record_failure(
            &instance_id,
            SurfaceExecutionFailure {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                request_id: "request:failure".to_owned(),
                generation,
                error: loom_protocol::SurfaceExecutionError {
                    code: "offline".to_owned(),
                    message: "provider unavailable".to_owned(),
                    detail: None,
                },
                last_successful_result_revision: None,
            },
        )
        .expect("record failure");
    assert_eq!(
        failed.latest_result.expect("last result").result_revision,
        1
    );
    assert_eq!(
        failed
            .last_failure
            .expect("failure")
            .last_successful_result_revision,
        Some(1)
    );
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}
