//! Persistent registry behavior and recovery coverage.

use super::*;

#[test]
pub(super) fn registry_save_update_delete_roundtrip() {
    let root = temp_root("roundtrip");
    let registry = ToolRegistry::new(&root);

    let tool = ToolDefinition::new(
        "brave-search",
        "Brave Search",
        "Search the web through MCP",
        ToolExecution::Mcp {
            server_id: "brave".to_owned(),
            tool_name: "brave_web_search".to_owned(),
        },
    );

    registry.save_tool(tool.clone()).expect("save tool");
    assert!(root.join("tools.json").exists());
    assert_eq!(
        registry.list_tools().expect("list tools"),
        vec![tool.clone()]
    );
    assert_eq!(
        registry.get_tool("brave-search").expect("get tool"),
        Some(tool.clone())
    );

    let updated = ToolDefinition {
        name: "Brave Web Search".to_owned(),
        enabled: false,
        ..tool
    };
    registry.save_tool(updated.clone()).expect("update tool");
    assert_eq!(
        registry.get_tool("brave-search").expect("get updated"),
        Some(updated)
    );

    assert!(registry.delete_tool("brave-search").expect("delete tool"));
    assert!(registry.list_tools().expect("list after delete").is_empty());
    assert!(!registry.delete_tool("brave-search").expect("delete absent"));

    fs::remove_dir_all(root).expect("cleanup temp tool registry root");
}

#[test]
pub(super) fn registry_rehydrates_persisted_art_settings_after_restart() {
    let root = temp_root("persisted-art-settings");
    let registry = ToolRegistry::new(root.join("tools"));
    let mut tool = ToolDefinition::new(
        "image-search",
        "Image Search",
        "Package-backed image search",
        ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    );
    tool.metadata = Some(serde_json::json!({
        "packageSecurity": {
            "publisher": { "id": "publisher.example", "name": "Publisher" }
        }
    }));
    registry.save_tool(tool).expect("save package tool");

    art_settings::ArtSettingsStore::new(&root)
        .save(
            "publisher.example/image-search",
            art_settings::ArtUserSettings {
                credential_bindings: std::collections::BTreeMap::from([(
                    "api_key".to_owned(),
                    "stored-secret".to_owned(),
                )]),
                ..art_settings::ArtUserSettings::default()
            },
        )
        .expect("save Art settings independently of the registry projection");

    let restarted = ToolRegistry::new(root.join("tools"));
    let rehydrated = restarted
        .get_tool("publisher.example/image-search")
        .expect("read restarted registry")
        .expect("rehydrated tool");
    assert_eq!(
        rehydrated
            .metadata
            .as_ref()
            .and_then(|metadata| {
                metadata.pointer("/artUserSettings/credentialBindings/api_key")
            })
            .and_then(serde_json::Value::as_str),
        Some("stored-secret")
    );

    fs::remove_dir_all(root).expect("cleanup persisted Art settings root");
}

#[test]
pub(super) fn a_damaged_art_settings_file_does_not_hide_every_art() {
    let root = temp_root("damaged-art-settings");
    let registry = ToolRegistry::new(root.join("tools"));
    let mut tool = ToolDefinition::new(
        "image-search",
        "Image Search",
        "Package-backed image search",
        ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    );
    tool.metadata = Some(serde_json::json!({
        "packageSecurity": {
            "publisher": { "id": "publisher.example", "name": "Publisher" }
        }
    }));
    registry.save_tool(tool).expect("save package tool");

    let settings_path = root.join("art-user-settings.json");
    fs::write(&settings_path, b"{\"arts\":{\"publisher.example/image-s")
        .expect("truncate the Art settings file");

    // Before this fix the truncated preferences file propagated its parse error out of
    // `read_tools`, so every registry operation failed and the Art list came back empty.
    let restarted = ToolRegistry::new(root.join("tools"));
    let tools = restarted
        .list_tools()
        .expect("list tools past damaged settings");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].qualified_id(), "publisher.example/image-search");
    assert!(restarted
        .get_tool("publisher.example/image-search")
        .expect("get tool past damaged settings")
        .is_some());
    assert!(
        fs::read_dir(&root)
            .expect("read control plane root")
            .filter_map(|entry| entry.ok())
            .any(|entry| entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("art-user-settings.json.corrupt-"))),
        "the damaged settings file should be copied aside before it is reset"
    );

    fs::remove_dir_all(root).expect("cleanup damaged Art settings root");
}

#[test]
pub(super) fn registry_removes_stale_art_settings_without_a_persisted_entry() {
    let root = temp_root("stale-art-settings");
    let tools_root = root.join("tools");
    fs::create_dir_all(&tools_root).expect("create canonical registry root");
    let mut tool = ToolDefinition::new(
        "image-search",
        "Image Search",
        "Package-backed image search",
        ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    );
    tool.metadata = Some(serde_json::json!({
        "packageSecurity": {
            "publisher": { "id": "publisher.example", "name": "Publisher" }
        },
        "artUserSettings": {
            "credentialBindings": { "api_key": "stale-secret" }
        }
    }));
    fs::write(
        tools_root.join(TOOLS_FILE),
        serde_json::to_vec_pretty(&vec![tool]).expect("serialize stale registry"),
    )
    .expect("write stale registry");

    let restarted = ToolRegistry::new(&tools_root);
    let sanitized = restarted
        .get_tool("publisher.example/image-search")
        .expect("read restarted registry")
        .expect("sanitized tool");
    assert!(sanitized
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artUserSettings"))
        .is_none());
    assert!(sanitized
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("packageSecurity"))
        .is_some());

    fs::remove_dir_all(root).expect("cleanup stale Art settings root");
}

#[test]
pub(super) fn registry_keeps_same_local_id_isolated_by_publisher() {
    let root = temp_root("publisher-namespace");
    let registry = ToolRegistry::new(&root);
    let make_tool = |publisher: &str, name: &str| {
        let mut tool = ToolDefinition::new(
            "shared-art",
            name,
            "Publisher-scoped Art",
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
        );
        tool.metadata = Some(serde_json::json!({
            "packageSecurity": {
                "publisher": { "id": publisher, "name": publisher }
            }
        }));
        tool
    };
    let alpha = make_tool("publisher.alpha", "Alpha");
    let beta = make_tool("publisher.beta", "Beta");
    registry.save_tool(alpha.clone()).expect("save alpha");
    registry.save_tool(beta.clone()).expect("save beta");

    assert_eq!(registry.list_tools().expect("list").len(), 2);
    assert_eq!(
        registry
            .get_tool("publisher.alpha/shared-art")
            .expect("get qualified alpha"),
        Some(alpha)
    );
    assert!(matches!(
        registry.get_tool("shared-art"),
        Err(ToolRegistryError::AmbiguousToolId { .. })
    ));
    assert!(registry
        .delete_tool("publisher.beta/shared-art")
        .expect("delete qualified beta"));
    assert_eq!(
        registry
            .get_tool("shared-art")
            .expect("bare id becomes unambiguous")
            .expect("remaining alpha")
            .name,
        "Alpha"
    );
    fs::remove_dir_all(root).expect("cleanup publisher namespace registry");
}

#[test]
pub(super) fn registry_recovers_trailing_json_and_quarantines_original() {
    let root = temp_root("trailing-json");
    fs::create_dir_all(&root).expect("create registry root");
    let tool = ToolDefinition::new(
        "recovered-tool",
        "Recovered Tool",
        "Tool from a recoverable registry",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    let valid = serde_json::to_string_pretty(&vec![tool.clone()]).expect("serialize tool");
    let corrupted = format!("{valid}\n  }}  }}\n]");
    fs::write(root.join("tools.json"), &corrupted).expect("write corrupted registry");

    let registry = ToolRegistry::new(&root);
    assert_eq!(registry.list_tools().expect("recover tools"), vec![tool]);

    let canonical = fs::read_to_string(root.join("tools.json")).expect("read repaired registry");
    let parsed: Vec<ToolDefinition> =
        serde_json::from_str(&canonical).expect("repaired registry is valid JSON");
    assert_eq!(parsed.len(), 1);

    let backups = fs::read_dir(&root)
        .expect("read registry directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("tools.json.corrupt-")
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        fs::read_to_string(backups[0].path()).expect("read corruption backup"),
        corrupted
    );

    fs::remove_dir_all(root).expect("cleanup recovered registry root");
}

#[test]
pub(super) fn registry_does_not_remove_unknown_future_execution_entries() {
    let root = temp_root("future-execution");
    fs::create_dir_all(&root).expect("create registry root");
    let original = serde_json::to_string_pretty(&serde_json::json!([{
        "id": "future-art",
        "name": "Future Art",
        "description": "unknown future execution",
        "enabled": true,
        "execution": { "type": "future_runtime" }
    }]))
    .expect("serialize future tool");
    let registry_path = root.join("tools.json");
    fs::write(&registry_path, &original).expect("write future registry");

    let registry = ToolRegistry::new(&root);
    assert!(matches!(
        registry.list_tools(),
        Err(ToolRegistryError::Json(_))
    ));
    assert_eq!(
        fs::read_to_string(&registry_path).expect("read unchanged registry"),
        original
    );
    assert_eq!(
        fs::read_dir(&root)
            .expect("read registry directory")
            .filter_map(Result::ok)
            .filter(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("tools.json.corrupt-"))
            .count(),
        0
    );

    fs::remove_dir_all(root).expect("cleanup future registry root");
}

#[test]
pub(super) fn registry_does_not_recover_comma_only_trailing_json() {
    let root = temp_root("trailing-commas");
    fs::create_dir_all(&root).expect("create registry root");
    let tool = ToolDefinition::new(
        "preserved-tool",
        "Preserved Tool",
        "Tool in an unrecoverable registry",
        ToolExecution::FrameworkArt {
            framework: "process".to_owned(),
        },
    );
    let valid = serde_json::to_string_pretty(&vec![tool]).expect("serialize tool");
    let corrupted = format!("{valid}\n,,,");
    let registry_path = root.join("tools.json");
    fs::write(&registry_path, &corrupted).expect("write comma-corrupted registry");

    let registry = ToolRegistry::new(&root);
    assert!(matches!(
        registry.list_tools(),
        Err(ToolRegistryError::Json(_))
    ));
    assert_eq!(
        fs::read_to_string(&registry_path).expect("read unchanged registry"),
        corrupted
    );
    assert_eq!(
        fs::read_dir(&root)
            .expect("read registry directory")
            .filter_map(Result::ok)
            .filter(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("tools.json.corrupt-"))
            .count(),
        0
    );

    fs::remove_dir_all(root).expect("cleanup comma-corrupted registry root");
}

#[test]
pub(super) fn registry_read_is_bounded_before_json_parsing() {
    let root = temp_root("oversized-registry");
    fs::write(
        root.join(TOOLS_FILE),
        vec![b' '; super::super::registry::MAX_TOOL_REGISTRY_BYTES + 1],
    )
    .expect("write oversized registry");

    let error = ToolRegistry::new(&root)
        .list_tools()
        .expect_err("oversized registry must be rejected");

    assert!(matches!(
        error,
        ToolRegistryError::Io(ref source)
            if source.kind() == std::io::ErrorKind::InvalidData
    ));
    fs::remove_dir_all(root).expect("cleanup oversized registry root");
}
