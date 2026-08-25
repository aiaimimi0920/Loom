use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::model::{
    ArtSettingsFile, ART_SETTINGS_FILE, MAX_ART_SETTINGS_DEPTH, MAX_ART_SETTINGS_FILE_BYTES,
    MAX_ART_SETTING_ENTRIES, MAX_ART_SETTING_VALUE_BYTES,
};
use super::*;
use crate::credentials::{CredentialStore, CredentialValueType};
use crate::{ToolDefinition, ToolExecution};

fn temp_root() -> PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for _ in 0..32 {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "loom-art-settings-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create isolated art settings test root: {error}"),
        }
    }
    panic!("create isolated art settings test root: exhausted unique names");
}

#[test]
fn settings_default_to_auto_update_and_roundtrip_atomically() {
    let root = temp_root();
    let store = ArtSettingsStore::new(&root);
    assert!(store.get("sample").unwrap().auto_update);
    let settings = ArtUserSettings {
        auto_update: false,
        defaults: BTreeMap::from([("strength".to_owned(), serde_json::json!(0.8))]),
        value_bindings: BTreeMap::from([("quality".to_owned(), "image_quality".to_owned())]),
        credential_bindings: BTreeMap::from([(
            "cloudflare".to_owned(),
            "cloudflare_key".to_owned(),
        )]),
        ..ArtUserSettings::default()
    };
    store.save("sample", settings.clone()).unwrap();
    assert_eq!(store.get("sample").unwrap(), settings);
    assert!(!root.join("art-user-settings.json.tmp").exists());
    assert_eq!(
        fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join(ART_SETTINGS_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    fs::remove_dir_all(root).unwrap();
}

fn corruption_backups(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("art-user-settings.json.corrupt-"))
                .then_some(path)
        })
        .collect()
}

#[test]
fn a_damaged_settings_file_is_backed_up_and_reset_instead_of_failing_every_read() {
    let root = temp_root();
    let store = ArtSettingsStore::new(&root);
    let path = root.join("art-user-settings.json");
    let damaged = b"{\"schemaVersion\":1,\"arts\":{\"sample\":{\"autoUpd";
    fs::write(&path, damaged).unwrap();

    // One truncated preference document must not hide every installed Art.
    assert_eq!(store.get_optional("sample").unwrap(), None);
    assert!(store.get("sample").unwrap().auto_update);
    assert!(store.list().unwrap().is_empty());

    let backups = corruption_backups(&root);
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(&backups[0]).unwrap(), damaged);
    assert!(serde_json::from_slice::<ArtSettingsFile>(&fs::read(&path).unwrap()).is_ok());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_damaged_settings_file_still_accepts_the_next_save() {
    let root = temp_root();
    let store = ArtSettingsStore::new(&root);
    fs::write(root.join("art-user-settings.json"), b"not json at all").unwrap();

    let settings = ArtUserSettings {
        auto_update: false,
        ..ArtUserSettings::default()
    };
    store.save("sample", settings.clone()).unwrap();
    assert_eq!(store.get("sample").unwrap(), settings);
    assert_eq!(corruption_backups(&root).len(), 1);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_authorship_requires_authoring_metadata() {
    let mut tool = ToolDefinition::new(
        "local",
        "Local",
        "",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    assert!(!art_is_locally_authored(&tool));
    tool.metadata = Some(serde_json::json!({ "authoring": { "origin": "local" } }));
    assert!(art_is_locally_authored(&tool));
    tool.metadata = Some(serde_json::json!({
        "authoring": { "origin": "local" },
        "packageSecurity": { "publisher": { "id": "neuro.official", "name": "Neuro" } }
    }));
    assert!(art_is_locally_authored(&tool));
}

#[test]
fn explicit_parameters_override_saved_and_manifest_defaults() {
    let mut tool = ToolDefinition::new(
        "sample",
        "Sample",
        "",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    tool.params = vec![
        serde_json::json!({ "id": "strength", "default": 0.2 }),
        serde_json::json!({ "id": "api_token", "type": "secret", "default": "must-not-merge" }),
    ];
    tool.metadata = Some(serde_json::json!({
        "artUserSettings": {
            "defaults": { "strength": 0.6, "quality": 90, "api_token": "must-not-merge" },
            "valueBindings": { "quality": "global_quality" }
        }
    }));
    let merged = merge_tool_arguments(
        &tool,
        serde_json::json!({ "inputs": { "image": "x" }, "params": { "strength": 0.9 } }),
    );
    assert_eq!(merged["params"]["strength"], 0.9);
    assert!(merged["params"].get("quality").is_none());
    assert!(merged["params"].get("api_token").is_none());
}

#[test]
fn metadata_projection_removes_defaults_for_secret_parameters() {
    let mut tool = ToolDefinition::new(
        "sample",
        "Sample",
        "",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    tool.params = vec![serde_json::json!({
        "id": "api_token",
        "type": "secret"
    })];
    let settings = ArtUserSettings {
        defaults: BTreeMap::from([
            (
                "api_token".to_owned(),
                serde_json::json!("must-not-persist"),
            ),
            ("quality".to_owned(), serde_json::json!(90)),
        ]),
        ..ArtUserSettings::default()
    };
    apply_settings_metadata(&mut tool, &settings);
    let defaults = &tool.metadata.unwrap()["artUserSettings"]["defaults"];
    assert!(defaults.get("api_token").is_none());
    assert_eq!(defaults["quality"], 90);
}

#[test]
fn global_value_bindings_resolve_typed_values_and_explicit_params_win() {
    let root = temp_root();
    let art_dir = root
        .join("arts")
        .join("neuro.official")
        .join("sample")
        .join("versions")
        .join("1.0.0");
    fs::create_dir_all(&art_dir).unwrap();
    CredentialStore::new(&root)
        .upsert(crate::credentials::CredentialInput {
            name: "image_count".to_owned(),
            value: "3".to_owned(),
            value_type: CredentialValueType::Integer,
            scope: crate::credentials::CredentialScope::default(),
            expires_at: None,
        })
        .unwrap();
    let mut tool = ToolDefinition::new(
        "sample",
        "Sample",
        "",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    tool.params = vec![serde_json::json!({
        "id": "count",
        "type": "number",
        "minimum": 1,
        "maximum": 5,
        "default": 1
    })];
    tool.metadata = Some(serde_json::json!({
        "artPackage": { "dir": art_dir },
        "artUserSettings": {
            "defaults": { "count": 2 },
            "valueBindings": { "count": "image_count" }
        }
    }));
    let prepared = resolve_tool_value_bindings(
        &tool,
        merge_tool_arguments(&tool, serde_json::json!({ "params": {} })),
    )
    .unwrap();
    assert_eq!(prepared["params"]["count"], 3);
    let explicit = resolve_tool_value_bindings(
        &tool,
        merge_tool_arguments(&tool, serde_json::json!({ "params": { "count": 4 } })),
    )
    .unwrap();
    assert_eq!(explicit["params"]["count"], 4);
    let disabled = resolve_tool_value_bindings(
        &tool,
        merge_tool_arguments(
            &tool,
            serde_json::json!({ "params": {}, "disabledParams": ["count"] }),
        ),
    )
    .unwrap();
    assert!(disabled["params"].get("count").is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn global_value_bindings_reject_missing_or_mismatched_values() {
    let root = temp_root();
    let art_dir = root
        .join("arts")
        .join("sample")
        .join("versions")
        .join("1.0.0");
    fs::create_dir_all(&art_dir).unwrap();
    CredentialStore::new(&root)
        .upsert(crate::credentials::CredentialInput {
            name: "label".to_owned(),
            value: "three".to_owned(),
            value_type: CredentialValueType::String,
            scope: crate::credentials::CredentialScope::default(),
            expires_at: None,
        })
        .unwrap();
    let mut tool = ToolDefinition::new(
        "sample",
        "Sample",
        "",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    tool.params = vec![serde_json::json!({ "id": "count", "type": "integer" })];
    tool.metadata = Some(serde_json::json!({
        "artPackage": { "dir": art_dir },
        "artUserSettings": { "valueBindings": { "count": "label" } }
    }));
    assert!(matches!(
        resolve_tool_value_bindings(&tool, serde_json::json!({})),
        Err(ArtSettingsError::ParameterBinding(_))
    ));
    tool.metadata.as_mut().unwrap()["artUserSettings"]["valueBindings"]["count"] =
        serde_json::json!("missing");
    assert!(matches!(
        resolve_tool_value_bindings(&tool, serde_json::json!({})),
        Err(ArtSettingsError::ParameterBinding(_))
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_schema_version_must_be_current() {
    let root = temp_root();
    let path = root.join(ART_SETTINGS_FILE);
    let document = br#"{"schemaVersion":2,"arts":{}}"#;
    fs::write(&path, document).unwrap();
    let store = ArtSettingsStore::new(&root);
    assert!(matches!(
        store.list(),
        Err(ArtSettingsError::UnsupportedSchemaVersion {
            actual: 2,
            expected: 1
        })
    ));
    assert_eq!(fs::read(&path).unwrap(), document);
    assert!(corruption_backups(&root).is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn oversized_settings_files_are_rejected_before_parsing() {
    let root = temp_root();
    let path = root.join(ART_SETTINGS_FILE);
    let file = fs::File::create(&path).unwrap();
    file.set_len(MAX_ART_SETTINGS_FILE_BYTES + 1).unwrap();
    let store = ArtSettingsStore::new(&root);
    assert!(matches!(
        store.list(),
        Err(ArtSettingsError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData
    ));
    assert_eq!(
        fs::metadata(path).unwrap().len(),
        MAX_ART_SETTINGS_FILE_BYTES + 1
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn deeply_nested_settings_are_rejected_without_resetting_valid_json() {
    let root = temp_root();
    let path = root.join(ART_SETTINGS_FILE);
    let levels = MAX_ART_SETTINGS_DEPTH + 8;
    let nested = format!("{}0{}", "[".repeat(levels), "]".repeat(levels));
    let document = [
        "{\"schemaVersion\":1,\"arts\":{\"sample\":{\"defaults\":{\"payload\":",
        &nested,
        "}}}}",
    ]
    .concat();
    fs::write(&path, &document).unwrap();
    let store = ArtSettingsStore::new(&root);
    assert!(matches!(
        store.list(),
        Err(ArtSettingsError::InvalidDocument(reason)) if reason.contains("nesting")
    ));
    assert_eq!(fs::read_to_string(path).unwrap(), document);
    assert!(corruption_backups(&root).is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_entry_and_value_budgets_are_enforced_before_write() {
    let root = temp_root();
    let store = ArtSettingsStore::new(&root);
    let too_many = ArtUserSettings {
        defaults: (0..=MAX_ART_SETTING_ENTRIES)
            .map(|index| (format!("key_{index}"), serde_json::json!(index)))
            .collect(),
        ..ArtUserSettings::default()
    };
    assert!(matches!(
        store.save("sample", too_many),
        Err(ArtSettingsError::InvalidDocument(reason)) if reason.contains("entries")
    ));
    let too_large = ArtUserSettings {
        defaults: BTreeMap::from([(
            "payload".to_owned(),
            serde_json::json!("x".repeat(MAX_ART_SETTING_VALUE_BYTES + 1)),
        )]),
        ..ArtUserSettings::default()
    };
    assert!(matches!(
        store.save("sample", too_large),
        Err(ArtSettingsError::InvalidDocument(reason)) if reason.contains("byte limit")
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_saves_preserve_every_art() {
    let root = temp_root();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
    let threads = (0..12)
        .map(|index| {
            let root = root.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                ArtSettingsStore::new(root)
                    .save(
                        &format!("sample_{index}"),
                        ArtUserSettings {
                            auto_update: index % 2 == 0,
                            ..ArtUserSettings::default()
                        },
                    )
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(ArtSettingsStore::new(&root).list().unwrap().len(), 12);
    assert_eq!(
        fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn corruption_backups_are_private_and_bounded() {
    let root = temp_root();
    let store = ArtSettingsStore::new(&root);
    for index in 0..5 {
        fs::write(root.join(ART_SETTINGS_FILE), format!("damaged-{index}")).unwrap();
        assert!(store.list().unwrap().is_empty());
    }
    let backups = corruption_backups(&root);
    assert_eq!(backups.len(), 3);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert!(backups
            .iter()
            .all(|backup| { fs::metadata(backup).unwrap().permissions().mode() & 0o777 == 0o600 }));
    }
    fs::remove_dir_all(root).unwrap();
}
