use std::fs;
use std::path::PathBuf;

use loom_protocol::parse_capability_manifest;

#[test]
fn official_manifest_declares_the_complete_ocr_surface() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../capability-packages/ocr/capability.manifest.json");
    let manifest = parse_capability_manifest(&fs::read(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest.qualified_id(), "neuro.official/ocr");
    assert_eq!(manifest.contributes.commands.len(), 8);
    assert_eq!(manifest.contributes.shortcuts.len(), 2);
    assert_eq!(manifest.contributes.menus.len(), 3);
    assert_eq!(
        manifest.contributes.menus[0].id,
        "neuro.official/ocr.menu.copy-full-text"
    );
    assert!(manifest.contributes.menus.iter().any(|menu| {
        menu.id == "neuro.official/ocr.menu.copy-layout-text"
            && menu.command.as_deref() == Some("neuro.official/ocr.copy-layout-text")
    }));
    assert!(manifest.contributes.menus.iter().any(|menu| {
        menu.id == "neuro.official/ocr.menu.copy-selected-text"
            && menu.command.as_deref() == Some("neuro.official/ocr.copy-selected-text")
    }));
    assert!(manifest
        .contributes
        .commands
        .iter()
        .any(|command| command.id == "neuro.official/ocr.scan-codes"));
    assert_eq!(manifest.contributes.data_types.len(), 2);
    assert_eq!(manifest.contributes.renderers.len(), 2);
    assert!(manifest
        .permissions
        .contains(&"hook.unit.image.read".to_owned()));
    assert!(manifest
        .permissions
        .contains(&"hook.external.open".to_owned()));
}
