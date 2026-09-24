use std::fs;
use std::path::PathBuf;

use loom_protocol::parse_capability_manifest;

#[test]
fn official_translation_manifest_declares_ctrl_five_and_language_setting() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../capability-packages/text-translation/capability.manifest.json");
    let manifest =
        parse_capability_manifest(&fs::read(path).expect("manifest")).expect("valid manifest");
    assert_eq!(manifest.qualified_id(), "neuro.official/text-translation");
    assert!(manifest.description.contains("local"));
    assert!(manifest
        .host_compatibility
        .loom_capability_api
        .required_features
        .iter()
        .any(|feature| feature == "model-broker.v1"));
    assert!(manifest.contributes.shortcuts.iter().any(|shortcut| {
        shortcut.command.as_deref() == Some("neuro.official/text-translation.toggle")
            && shortcut
                .payload
                .get("keys")
                .and_then(|value| value.as_str())
                == Some("ctrl+5")
    }));
    assert!(manifest.contributes.shortcuts.iter().any(|shortcut| {
        shortcut.command.as_deref() == Some("neuro.official/text-translation.toggle-overlay")
            && shortcut.when.as_deref() == Some("unit.kind == 'sticker' && unit.hasImage")
            && shortcut
                .payload
                .get("keys")
                .and_then(|value| value.as_str())
                == Some("alt+5")
    }));
    for command in &manifest.contributes.commands {
        assert_eq!(
            command.when.as_deref(),
            Some("unit.kind == 'sticker' && unit.hasImage")
        );
        assert!(
            command.toggle_attachment_type.is_none(),
            "Ctrl+5 must refresh without deleting cached results"
        );
    }
    assert!(manifest
        .contributes
        .settings
        .iter()
        .any(|setting| { setting.id == "neuro.official/text-translation.target-language" }));
    assert!(manifest
        .contributes
        .data_types
        .iter()
        .any(|data_type| { data_type.id == "neuro.official/text-translation.result.v1" }));
}
