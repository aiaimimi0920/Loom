use super::*;
use std::io::Write;

fn signed_package(
    root: &std::path::Path,
    kind: StorePackageKind,
    key: &loom_plugin_security::SigningKeyDocument,
    publisher: &str,
    wrong_location: bool,
) -> Vec<u8> {
    let name = match kind {
        StorePackageKind::Capability => "capability.manifest.json",
        StorePackageKind::McpServer => "mcp.server.json",
        StorePackageKind::Art => unreachable!(),
    };
    let signature =
        serde_json::json!({"algorithm":"ed25519","keyId":key.key_id,"file":"signature.json"});
    let mut manifest = serde_json::json!({
        "id":"signed-addon","version":"1.0.0","name":"Addon",
        "publisher":{"id":publisher,"name":"Publisher"},
    });
    if kind == StorePackageKind::Capability {
        manifest["publisher"]["keyId"] = serde_json::json!(key.key_id);
    }
    if kind == StorePackageKind::Capability && !wrong_location {
        manifest["signature"] = signature;
    } else {
        manifest["packageSecurity"] = serde_json::json!({"signature": signature});
    }
    let package = root.join("fixture");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join(name), serde_json::to_vec(&manifest).unwrap()).unwrap();
    loom_plugin_security::sign_package(&package, "signature.json", key).unwrap();
    let mut bytes = Vec::new();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
    for file in [name, "signature.json"] {
        zip.start_file(file, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&std::fs::read(package.join(file)).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();
    bytes
}

#[test]
fn capability_and_mcp_signatures_use_their_canonical_manifest_locations() {
    for kind in [StorePackageKind::Capability, StorePackageKind::McpServer] {
        let root = std::env::temp_dir().join(format!(
            "loom-store-signature-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let key = loom_plugin_security::generate_signing_key("key-1");
        let publisher = register_publisher(&root, &key.key_id, &key.public_key).unwrap();
        let bytes = signed_package(&root, kind, &key, &publisher.user_id, false);
        let published = store_verified_published_zip(&root, Some("signed-addon"), &bytes).unwrap();
        assert_eq!(published.kind, kind);
        assert_eq!(build_unified_catalog(&root).unwrap()[0].kind, kind);
        if kind == StorePackageKind::Capability {
            let wrong = signed_package(&root, kind, &key, &publisher.user_id, true);
            assert!(matches!(
                store_verified_published_zip(&root, None, &wrong),
                Err(StoreError::MissingPublisherSignature)
            ));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
