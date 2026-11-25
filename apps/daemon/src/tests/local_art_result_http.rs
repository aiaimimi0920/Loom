// 正式结果权限必须同时由 immutable manifest 和精确审批授权，旧审批不自动扩权。
#[test]
fn local_art_result_http_requires_explicit_approval_and_manifest_scope() {
    let mut f = Fixture::build_scoped("local.js", &sha256_bytes(SOURCE), false, SOURCE, true);
    assert_eq!(f.approve(0, true).0, 200);
    let (status, original) = f.prepare(f.request.clone());
    assert_eq!(status, 200, "{original}");
    let original = loom_plugin_security::local_art_grant::verify(
        &serde_json::to_vec(&original["grant"]).unwrap(),
        f.issuer["keyId"].as_str().unwrap(),
        f.issuer["publicKey"].as_str().unwrap(),
    )
    .unwrap();
    use loom_protocol::local_art::Permission;
    assert!(!original.permissions.contains(&Permission::WorkspaceResult));
    f.request["permissions"]
        .as_array_mut()
        .unwrap()
        .push(json!("workspace_result"));
    assert_eq!(
        f.prepare(f.request.clone()).0,
        403,
        "old approval must not expand"
    );
    assert_eq!(f.approve(1, true).0, 200);
    let (status, response) = f.prepare(f.request.clone());
    assert_eq!(status, 200, "{response}");
    let grant = loom_plugin_security::local_art_grant::verify(
        &serde_json::to_vec(&response["grant"]).unwrap(),
        f.issuer["keyId"].as_str().unwrap(),
        f.issuer["publicKey"].as_str().unwrap(),
    )
    .unwrap();
    assert!(grant.permissions.contains(&Permission::WorkspaceResult));
    assert_eq!(
        serde_json::to_value(&grant.permissions).unwrap(),
        f.request["permissions"]
    );
    assert_eq!(grant.result_owner, "otter_workspace");
    assert_eq!(f.approve(2, false).0, 200);
    assert_eq!(f.prepare(f.request.clone()).0, 403);

    let mut missing = Fixture::new();
    missing.request["permissions"]
        .as_array_mut()
        .unwrap()
        .push(json!("workspace_result"));
    assert_eq!(missing.approve(0, true).0, 200);
    assert_eq!(
        missing.prepare(missing.request.clone()).0,
        409,
        "approval cannot expand manifest"
    );
}
