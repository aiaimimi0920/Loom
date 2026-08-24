use std::{collections::BTreeMap, fs, path::PathBuf};

use super::protection::protect_value;
use super::types::{CREDENTIALS_FILE, MAX_CREDENTIAL_FILE_BYTES};
use super::values::is_safe_scope_reference;
use super::*;

fn temp_root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "loom-credentials-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&path);
    path
}

#[test]
fn credentials_are_scoped_and_never_returned_in_summaries() {
    let root = temp_root();
    let store = CredentialStore::new(&root);
    store
        .upsert(CredentialInput {
            name: "api_key".to_owned(),
            value: "secret-value".to_owned(),
            value_type: CredentialValueType::String,
            scope: CredentialScope {
                framework_id: Some("cloud_api".to_owned()),
                art_id: Some("example-art".to_owned()),
                mcp_server_id: None,
            },
            expires_at: None,
        })
        .expect("upsert");
    let summaries = store.summaries().expect("summaries");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].value_type, CredentialValueType::String);
    assert!(!serde_json::to_string(&summaries)
        .unwrap()
        .contains("secret-value"));
    let revealed = store
        .reveal("api_key", &summaries[0].scope)
        .expect("reveal")
        .expect("stored credential");
    assert_eq!(revealed.value, "secret-value");
    assert_eq!(revealed.value_type, CredentialValueType::String);
    assert!(store
        .grants_for("cloud_api", "other-art", &["api_key".to_owned()])
        .unwrap()
        .is_empty());
    let grants = store
        .grants_for("cloud_api", "example-art", &["api_key".to_owned()])
        .expect("grants");
    assert_eq!(grants[0].value, "secret-value");
    let credential_path = root.join(CREDENTIALS_FILE);
    assert!(credential_path.is_file());
    assert!(!fs::read_to_string(&credential_path)
        .unwrap()
        .contains("secret-value"));
    assert_eq!(
        fs::read_dir(&root)
            .expect("list credential root")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&root)
                .expect("credential root metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&credential_path)
                .expect("credential file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn global_credentials_can_be_reused_under_art_specific_aliases() {
    let root = temp_root();
    let store = CredentialStore::new(&root);
    store
        .upsert(CredentialInput {
            name: "cloudflare_key".to_owned(),
            value: "global-secret".to_owned(),
            value_type: CredentialValueType::String,
            scope: CredentialScope::default(),
            expires_at: None,
        })
        .unwrap();
    let bindings = BTreeMap::from([("api_token".to_owned(), "cloudflare_key".to_owned())]);
    for art_id in ["art-a", "art-b"] {
        let grants = store
            .grants_for_bindings("cloud_api", art_id, &bindings)
            .unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].name, "api_token");
        assert_eq!(grants[0].value, "global-secret");
    }
    let missing = store
        .grants_for_bindings(
            "cloud_api",
            "art-a",
            &BTreeMap::from([("token".to_owned(), "missing".to_owned())]),
        )
        .unwrap_err();
    assert!(matches!(missing, CredentialError::MissingBinding { .. }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_credentials_are_resolved_only_for_the_bound_server() {
    let root = temp_root();
    let store = CredentialStore::new(&root);
    store
        .upsert(CredentialInput {
            name: "image_search_key".to_owned(),
            value: "mcp-secret".to_owned(),
            value_type: CredentialValueType::String,
            scope: CredentialScope {
                framework_id: None,
                art_id: None,
                mcp_server_id: Some("neuro-image-search".to_owned()),
            },
            expires_at: None,
        })
        .expect("store MCP credential");
    let bindings = BTreeMap::from([("brave_api_key".to_owned(), "image_search_key".to_owned())]);
    let grants = store
        .grants_for_mcp_bindings("neuro-image-search", &bindings)
        .expect("resolve MCP binding");
    assert_eq!(grants[0].name, "brave_api_key");
    assert_eq!(grants[0].value, "mcp-secret");
    assert!(matches!(
        store.grants_for_mcp_bindings("other-server", &bindings),
        Err(CredentialError::MissingBinding { .. })
    ));
    assert!(store
        .grants_for("mcp", "art", &["image_search_key".to_owned()])
        .expect("Art grant lookup")
        .is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn credential_scopes_accept_publisher_qualified_package_ids() {
    let root = temp_root();
    let store = CredentialStore::new(&root);
    store
        .upsert(CredentialInput {
            name: "art_secret".to_owned(),
            value: "secret-value".to_owned(),
            value_type: CredentialValueType::String,
            scope: CredentialScope {
                framework_id: Some("neuro.official/mcp".to_owned()),
                art_id: Some("neuro.official/custom-image-search".to_owned()),
                mcp_server_id: None,
            },
            expires_at: None,
        })
        .expect("qualified scope");
    let grants = store
        .grants_for_bindings(
            "neuro.official/mcp",
            "neuro.official/custom-image-search",
            &BTreeMap::from([("brave_api_key".to_owned(), "art_secret".to_owned())]),
        )
        .expect("qualified grants");
    assert_eq!(grants[0].value, "secret-value");
    assert!(!is_safe_scope_reference("publisher/art/extra"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn typed_global_values_are_canonicalized_and_resolved_without_disclosure() {
    let root = temp_root();
    let store = CredentialStore::new(&root);
    for (name, value, value_type) in [
        ("ratio", "3.500", CredentialValueType::Number),
        ("count", "3", CredentialValueType::Integer),
        ("enabled", "true", CredentialValueType::Boolean),
        (
            "payload",
            "{ \"b\": 2, \"a\": 1 }",
            CredentialValueType::Json,
        ),
    ] {
        store
            .upsert(CredentialInput {
                name: name.to_owned(),
                value: value.to_owned(),
                value_type,
                scope: CredentialScope::default(),
                expires_at: None,
            })
            .unwrap();
    }
    let values = store
        .global_values_for_bindings(&BTreeMap::from([
            ("quality".to_owned(), "ratio".to_owned()),
            ("count".to_owned(), "count".to_owned()),
            ("enabled".to_owned(), "enabled".to_owned()),
            ("config".to_owned(), "payload".to_owned()),
        ]))
        .unwrap();
    assert_eq!(values["quality"].value, serde_json::json!(3.5));
    assert_eq!(values["count"].value, serde_json::json!(3));
    assert_eq!(values["enabled"].value, serde_json::json!(true));
    assert_eq!(
        values["config"].value,
        serde_json::json!({ "a": 1, "b": 2 })
    );
    let serialized = fs::read_to_string(root.join(CREDENTIALS_FILE)).unwrap();
    assert!(!serialized.contains("3.500"));
    assert!(!serde_json::to_string(&store.summaries().unwrap())
        .unwrap()
        .contains("\"a\":1"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn typed_values_reject_invalid_inputs_and_non_string_secret_bindings() {
    let root = temp_root();
    let store = CredentialStore::new(&root);
    for (name, value, value_type) in [
        ("integer", "1.5", CredentialValueType::Integer),
        ("boolean", "yes", CredentialValueType::Boolean),
        ("json", "{", CredentialValueType::Json),
    ] {
        assert!(matches!(
            store.upsert(CredentialInput {
                name: name.to_owned(),
                value: value.to_owned(),
                value_type,
                scope: CredentialScope::default(),
                expires_at: None,
            }),
            Err(CredentialError::InvalidValue { .. })
        ));
    }
    store
        .upsert(CredentialInput {
            name: "count".to_owned(),
            value: "3".to_owned(),
            value_type: CredentialValueType::Integer,
            scope: CredentialScope::default(),
            expires_at: None,
        })
        .unwrap();
    assert!(matches!(
        store.grants_for_bindings(
            "process",
            "sample",
            &BTreeMap::from([("api_key".to_owned(), "count".to_owned())]),
        ),
        Err(CredentialError::NonStringSecretBinding { .. })
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stored_records_require_the_current_schema_and_value_type() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let (protected_value, protection) = protect_value(b"secret").unwrap();
    fs::write(
        root.join(CREDENTIALS_FILE),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 2,
            "credentials": [{
                "name": "missing-type",
                "protectedValue": protected_value,
                "protection": protection,
                "scope": {}
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let store = CredentialStore::new(&root);
    assert!(matches!(store.summaries(), Err(CredentialError::Json(_))));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stored_records_reject_unknown_schema_versions() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(CREDENTIALS_FILE),
        br#"{"schemaVersion":999,"credentials":[]}"#,
    )
    .unwrap();
    let store = CredentialStore::new(&root);
    assert!(matches!(
        store.summaries(),
        Err(CredentialError::UnsupportedSchemaVersion {
            actual: 999,
            expected: 2
        })
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn oversized_credential_documents_are_rejected_before_parsing() {
    let root = temp_root();
    fs::create_dir_all(&root).unwrap();
    let file = fs::File::create(root.join(CREDENTIALS_FILE)).unwrap();
    file.set_len(MAX_CREDENTIAL_FILE_BYTES + 1).unwrap();
    let store = CredentialStore::new(&root);
    assert!(matches!(
        store.summaries(),
        Err(CredentialError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn invalid_stored_expirations_never_authorize_credentials() {
    let root = temp_root();
    let store = CredentialStore::new(&root);
    store
        .upsert(CredentialInput {
            name: "api_key".to_owned(),
            value: "secret-value".to_owned(),
            value_type: CredentialValueType::String,
            scope: CredentialScope::default(),
            expires_at: None,
        })
        .unwrap();
    let path = root.join(CREDENTIALS_FILE);
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    document["credentials"][0]["expiresAt"] = serde_json::json!("invalid-timestamp");
    fs::write(&path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();

    assert!(store
        .grants_for("process", "sample", &["api_key".to_owned()])
        .unwrap()
        .is_empty());
    let bindings = BTreeMap::from([("token".to_owned(), "api_key".to_owned())]);
    assert!(matches!(
        store.grants_for_bindings("process", "sample", &bindings),
        Err(CredentialError::MissingBinding { .. })
    ));
    assert!(matches!(
        store.grants_for_mcp_bindings("server", &bindings),
        Err(CredentialError::MissingBinding { .. })
    ));
    assert!(matches!(
        store.global_values_for_bindings(&bindings),
        Err(CredentialError::MissingBinding { .. })
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn concurrent_upserts_preserve_every_credential() {
    let root = temp_root();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
    let threads = (0..12)
        .map(|index| {
            let root = root.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                CredentialStore::new(root)
                    .upsert(CredentialInput {
                        name: format!("key_{index}"),
                        value: format!("value-{index}"),
                        value_type: CredentialValueType::String,
                        scope: CredentialScope::default(),
                        expires_at: None,
                    })
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    let summaries = CredentialStore::new(&root).summaries().unwrap();
    assert_eq!(summaries.len(), 12);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deeply_nested_json_credentials_are_rejected() {
    let root = temp_root();
    let levels = 40;
    let value = format!("{}0{}", "[".repeat(levels), "]".repeat(levels));
    assert!(matches!(
        CredentialStore::new(&root).upsert(CredentialInput {
            name: "payload".to_owned(),
            value,
            value_type: CredentialValueType::Json,
            scope: CredentialScope::default(),
            expires_at: None,
        }),
        Err(CredentialError::InvalidValue { reason, .. }) if reason.contains("nesting")
    ));
    let _ = fs::remove_dir_all(root);
}
