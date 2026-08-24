// Runtime manifest and untrusted MCP response resource-limit regressions.
struct ManifestFixtureDir(PathBuf);

impl ManifestFixtureDir {
    fn create() -> Self {
        static NEXT_TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let temp_id = NEXT_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "loom-runtime-mcp-manifest-limit-{}-{temp_id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create manifest limit fixture");
        Self(path)
    }
}

impl Drop for ManifestFixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn runtime_manifest_read_has_an_exact_byte_boundary() {
    let art_dir = ManifestFixtureDir::create();
    let manifest_path = art_dir.0.join("manifest.json");
    fs::write(&manifest_path, vec![b' '; MAX_ART_MANIFEST_BYTES])
        .expect("write exact-limit manifest fixture");
    assert_eq!(
        read_art_manifest(&manifest_path)
            .expect("read exact-limit manifest")
            .len(),
        MAX_ART_MANIFEST_BYTES
    );

    fs::write(&manifest_path, vec![b' '; MAX_ART_MANIFEST_BYTES + 1])
        .expect("write oversized manifest fixture");

    let error = load_config(&art_dir.0).expect_err("oversized manifest must fail closed");

    assert!(error.contains("exceeds the 1048576 byte limit"), "{error}");
}

#[test]
fn response_limits_reject_excess_bytes_and_nesting() {
    let value = json!({ "payload": "x".repeat(64) });
    let encoded_len = serde_json::to_vec(&value).unwrap().len();
    validate_json_value_limits(&value, "fixture response", encoded_len, 64).unwrap();
    assert!(
        validate_json_value_limits(&value, "fixture response", encoded_len - 1, 64)
            .unwrap_err()
            .contains("byte limit")
    );

    let mut nested = Value::Null;
    for _ in 0..64 {
        nested = Value::Array(vec![nested]);
    }
    validate_json_value_limits(&nested, "fixture response", 1024, 64).unwrap();
    nested = Value::Array(vec![nested]);
    assert!(
        validate_json_value_limits(&nested, "fixture response", 1024, 64)
            .unwrap_err()
            .contains("nesting limit")
    );
}
