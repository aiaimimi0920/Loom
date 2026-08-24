//! Shared package archives, signing keys, and filesystem fixtures.

use super::*;

pub(super) fn package_bytes(manifest: &str, script: &[u8]) -> Vec<u8> {
    package_bytes_with_entry(manifest, "runtime/server.ps1", script)
}

pub(super) fn package_bytes_with_entry(manifest: &str, entry: &str, script: &[u8]) -> Vec<u8> {
    package_bytes_with_files(&[
        (MCP_SERVER_PACKAGE_MANIFEST, manifest.as_bytes()),
        (entry, script),
    ])
}

pub(super) fn package_bytes_with_files(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        let options = SimpleFileOptions::default();
        for (name, contents) in files {
            zip.start_file(*name, options).expect("zip entry");
            zip.write_all(contents).expect("zip bytes");
        }
        zip.finish().expect("finish zip");
    }
    bytes.into_inner()
}

pub(super) const SIGNATURE_FILE: &str = "package.signature.json";

pub(super) fn signed_manifest(key_id: &str) -> String {
    format!(
        r#"{{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{{"id":"publisher.test","name":"Publisher"}},
                "transport":"stdio",
                "entry":{{"command":"runtime/server.ps1","args":[]}},
                "packageSecurity":{{"signature":{{"algorithm":"ed25519","keyId":"{key_id}","file":"{SIGNATURE_FILE}"}}}}
            }}"#
    )
}

/// Build a package the way a publisher would: lay the tree out, sign it, then archive the tree
/// together with the signature document `sign_package` wrote.
pub(super) fn signed_package_bytes(key: &SigningKeyDocument, script: &[u8]) -> Vec<u8> {
    let manifest = signed_manifest(&key.key_id);
    let source = std::env::temp_dir().join(staging_name());
    fs::create_dir_all(source.join("runtime")).expect("create source tree");
    fs::write(source.join(MCP_SERVER_PACKAGE_MANIFEST), &manifest).expect("write manifest");
    fs::write(source.join("runtime").join("server.ps1"), script).expect("write entry");
    sign_package(&source, SIGNATURE_FILE, key).expect("sign package");
    let signature = fs::read(source.join(SIGNATURE_FILE)).expect("read signature document");
    let _ = fs::remove_dir_all(&source);
    package_bytes_with_files(&[
        (MCP_SERVER_PACKAGE_MANIFEST, manifest.as_bytes()),
        ("runtime/server.ps1", script),
        (SIGNATURE_FILE, &signature),
    ])
}

pub(super) fn write_trust_store(root: &Path, store: &TrustStore) {
    store
        .write_atomic(&root.join("plugin-trust.json"))
        .expect("write trust store");
}

pub(super) fn stdio_package_bytes() -> Vec<u8> {
    package_bytes(
        r#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server.ps1","args":[]}
            }"#,
        b"Write-Output ready",
    )
}
