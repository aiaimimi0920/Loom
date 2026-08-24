//! Manifest-bound validation regressions.

use super::*;

#[test]
fn manifest_validation_bounds_tools_arguments_and_credential_labels() {
    let root = std::env::temp_dir().join(staging_name());
    fs::create_dir_all(root.join("runtime")).expect("create package validation fixture");
    fs::write(root.join("runtime/server.ps1"), b"Write-Output ready")
        .expect("write package validation entry");
    let mut manifest: McpServerPackageManifest = serde_json::from_value(serde_json::json!({
        "schemaVersion": 1,
        "id": "fixture-search",
        "name": "Fixture Search",
        "description": "bounded",
        "version": "1.2.3",
        "publisher": {"id": "publisher.test", "name": "Publisher"},
        "transport": "stdio",
        "entry": {"command": "runtime/server.ps1", "args": []},
        "tools": [],
        "credentials": []
    }))
    .expect("parse package validation manifest");

    manifest.tools = vec!["search".to_owned(); MAX_MCP_TOOLS + 1];
    assert!(validate_manifest(&manifest, &root)
        .unwrap_err()
        .to_string()
        .contains("tools contains"));

    manifest.tools.clear();
    manifest.entry.args = vec!["argument".to_owned(); MAX_MCP_ARGUMENTS + 1];
    assert!(validate_manifest(&manifest, &root)
        .unwrap_err()
        .to_string()
        .contains("entry.args"));

    manifest.entry.args.clear();
    manifest.credentials = vec![McpPackageCredential {
        id: "api_key".to_owned(),
        label: "x".repeat(MAX_MCP_CREDENTIAL_LABEL_BYTES + 1),
        required: true,
        target: McpPackageCredentialTarget {
            kind: McpPackageCredentialTargetKind::Env,
            name: "API_KEY".to_owned(),
        },
    }];
    assert!(validate_manifest(&manifest, &root)
        .unwrap_err()
        .to_string()
        .contains("credentials[0].label"));
    fs::remove_dir_all(root).expect("cleanup package validation fixture");
}
