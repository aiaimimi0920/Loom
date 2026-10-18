use super::*;

#[test]
fn cloud_install_large_application_keeps_verified_file_not_archive_bytes() {
    let dir = std::env::temp_dir().join(format!("loom-large-prepare-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("large.zip");
    let payload = vec![42u8; 9 * 1024 * 1024];
    fs::write(&path, &payload).unwrap();
    let input = Input {
        task_id: None,
        publication_id: "publication".into(),
        kind: "application".into(),
        sha256: format!("{:x}", Sha256::digest(&payload)),
    };
    let receipt = json!({"publicationId":input.publication_id, "kind":"application", "policy":"application-large-zip-format-v1", "size":payload.len(), "sha256":input.sha256,"path":path});
    let prepared = checked_package(&input, receipt.clone()).unwrap();
    assert!(prepared.bytes.is_empty());
    assert_eq!(
        prepared.archive.as_ref().unwrap().metadata().unwrap().len(),
        payload.len() as u64
    );
    #[cfg(windows)]
    assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
    drop(prepared);
    let mut legacy = receipt.clone();
    legacy["policy"] = json!("application-zip-format-v1");
    assert!(checked_package(&input, legacy).is_err());
    fs::write(&path, b"changed").unwrap();
    assert!(checked_package(&input, receipt).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cloud_install_rejects_paths_unknown_fields_and_unsupported_selection() {
    let sha = "a".repeat(64);
    for extra in [
        json!({"path":"C:/private.zip"}),
        json!({"bundledCatalog":true}),
        json!({"approve":true}),
    ] {
        let mut input = json!({"publicationId":"id","kind":"art","sha256":sha});
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(parse_input(&input.to_string()).is_err());
    }
    for kind in ["framework", ""] {
        assert!(
            parse_input(&json!({"publicationId":"id","kind":kind,"sha256":sha}).to_string())
                .is_err()
        );
    }
    assert!(parse_input(&"x".repeat(4097)).is_err());
    assert!(parse_input(
        &json!({"publicationId":"id","kind":"art","sha256":"A".repeat(64)}).to_string()
    )
    .is_err());
}

#[test]
fn cloud_install_rechecks_selected_identity_and_saved_bytes() {
    let dir = std::env::temp_dir().join(format!("loom-cloud-install-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("package.zip");
    let bytes = b"bounded fixture, not executed";
    fs::write(&path, bytes).unwrap();
    let input = Input {
        task_id: None,
        publication_id: "publication".into(),
        kind: "capability".into(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
    };
    let receipt = json!({"publicationId":input.publication_id,"kind":input.kind,"sha256":input.sha256,"size":bytes.len(),"path":path,"policy":"loom-capability-zip-clamav-v1"});
    let prepared = checked_package(&input, receipt.clone()).unwrap();
    assert_eq!(prepared.bytes, bytes);
    let mut unscanned = receipt.clone();
    unscanned["policy"] = json!("loom-capability-zip-format-v1");
    assert_eq!(checked_package(&input, unscanned).unwrap().bytes, bytes);
    let mcp_input = parse_input(
        &json!({"publicationId":"publication", "kind":"mcp", "sha256":input.sha256}).to_string(),
    )
    .unwrap();
    let mut mcp_receipt = receipt.clone();
    mcp_receipt["kind"] = json!("mcp");
    mcp_receipt["policy"] = json!("loom-mcp-zip-format-v1");
    assert_eq!(
        checked_package(&mcp_input, mcp_receipt.clone())
            .unwrap()
            .bytes,
        bytes
    );
    for policy in ["mcp-zip-format-v1", "loom-mcp-zip-clamav-v1"] {
        mcp_receipt["policy"] = json!(policy);
        assert!(checked_package(&mcp_input, mcp_receipt.clone()).is_err());
    }
    for (key, value) in [
        ("kind", json!("art")),
        ("sha256", json!("b".repeat(64))),
        ("publicationId", json!("other")),
        ("policy", json!("capability-zip-clamav-v1")),
        ("policy", json!("capability-zip-format-v1")),
        ("policy", json!("loom-art-zip-format-v1")),
    ] {
        let mut changed = receipt.clone();
        changed[key] = value;
        assert!(checked_package(&input, changed).is_err());
    }
    fs::write(&path, b"tampered").unwrap();
    assert!(checked_package(&input, receipt.clone()).is_err());
    fs::write(&path, [bytes.as_slice(), b"extra"].concat()).unwrap();
    assert!(checked_package(&input, receipt.clone()).is_err());
    let mut oversized = receipt;
    oversized["size"] = json!(8 * 1024 * 1024 + 1);
    assert!(checked_package(&input, oversized).is_err());
    fs::remove_file(path).unwrap();
    fs::remove_dir(dir).unwrap();
}
