//! The public execution APIs must reject revoked authority before native code runs.
#![cfg(windows)]

use super::super::*;
use super::execution_support::*;

#[test]
fn every_public_framework_execution_facade_rejects_disabled_package() {
    let _guard = lock_windows_powershell_fixture();
    let root = temp_root("disabled-admission");
    let packages = root.join("frameworks");
    let art = write_fixture_package(&packages, SUCCESS_SCRIPT);
    let tool = fixture_tool(&art);
    let _environment = EnvVarGuard::set("LOOM_FRAMEWORK_PACKAGES_DIR", &packages);
    let registry = crate::framework::FrameworkRegistry::new(&root);
    registry.disable("publisher.test/script").unwrap();
    let cancelled = AtomicBool::new(false);
    let calls = [
        execute_framework_art(&tool, "publisher.test/script", json!({})),
        execute_framework_art_with_timeout(
            &tool,
            "publisher.test/script",
            json!({}),
            Duration::from_secs(5),
        ),
        execute_framework_art_with_timeout_and_cancellation(
            &tool,
            "publisher.test/script",
            json!({}),
            Duration::from_secs(5),
            &cancelled,
        ),
        crate::execute_tool(&tool, &[], json!({})),
        crate::execute_tool_with_timeout(&tool, &[], json!({}), Duration::from_secs(5)),
        crate::execute_tool_with_timeout_and_cancellation(
            &tool,
            &[],
            json!({}),
            Duration::from_secs(5),
            &cancelled,
        ),
    ];
    for call in calls {
        assert!(call
            .unwrap_err()
            .to_string()
            .contains("framework is disabled"));
    }
    registry.enable("publisher.test/script").unwrap();
    crate::execute_tool(&tool, &[], json!({})).expect("enabling restores execution");
    registry
        .set_trust_policy(loom_plugin_security::TrustPolicy::RequireTrusted)
        .unwrap();
    assert!(crate::execute_tool(&tool, &[], json!({})).is_err());
    fs::remove_dir_all(root).unwrap();
}
