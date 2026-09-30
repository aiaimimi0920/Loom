use loom_protocol::{validate_extension_message, ExtensionMessage, ExtensionTrustStatus};

use super::*;

#[test]
fn registration_scope_and_generation_change_atomically_with_lifecycle() {
    let root = temp_root("snapshot");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    let initial = host.contribution_snapshot().expect("initial snapshot");
    assert_eq!(initial.generation, 0);
    assert!(initial.plugins.is_empty());

    host.activate(package(&root, &executable, &[]))
        .expect("activate runtime");
    let active = host.contribution_snapshot().expect("active snapshot");
    assert_eq!(active.generation, 1);
    assert_eq!(active.plugins.len(), 1);
    assert_eq!(active.plugins[0].id, "publisher.example/fixture");
    assert_eq!(
        active.plugins[0].trust_status,
        ExtensionTrustStatus::Trusted
    );
    assert!(active.plugins[0].scope_id.starts_with("scope:"));
    assert_eq!(active.contributions.commands.len(), 1);
    assert_eq!(
        active.contributions.commands[0].scope_id,
        active.plugins[0].scope_id
    );
    validate_extension_message(&ExtensionMessage::Snapshot(active.clone()))
        .expect("valid extension snapshot");

    host.deactivate("publisher.example/fixture")
        .expect("deactivate runtime");
    let disabled = host.contribution_snapshot().expect("disabled snapshot");
    assert_eq!(disabled.generation, 2);
    assert!(disabled.plugins.is_empty());
    assert!(disabled.contributions.commands.is_empty());
    cleanup(&root);
}
