//! Locked child resolution regressions.

use super::*;

#[test]
fn packaged_workflow_resolves_its_immutable_locked_child_instead_of_active_registry() {
    let root = temp_root("locked-child-resolution");
    let registry = ToolRegistry::new(root.join("tools"));
    let mut active = ToolDefinition::new(
        "child",
        "Active v2",
        "active child",
        ToolExecution::CloudApi {
            endpoint: "https://active.invalid".to_owned(),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    active.metadata = Some(json!({
        "packageSecurity": {
            "publisher": { "id": "test.publisher", "name": "Test Publisher" }
        }
    }));
    registry.save_tool(active).expect("save active child");
    let mut locked = ToolDefinition::new(
        "child",
        "Locked v1",
        "locked child",
        ToolExecution::CloudApi {
            endpoint: "https://locked.invalid".to_owned(),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    locked.metadata = Some(json!({
        "packageSecurity": {
            "publisher": { "id": "test.publisher", "name": "Test Publisher" }
        }
    }));
    let mut parent = workflow_tool("locked-workflow");
    parent.metadata = Some(json!({
        "dependencies": { "arts": ["test.publisher/child"] },
        "artPackage": {
            "lockedArts": { "test.publisher/child": locked }
        }
    }));

    let resolved = resolve_workflow_child_tool(&parent, "test.publisher/child", &registry)
        .expect("resolve child")
        .expect("locked child");
    assert_eq!(resolved.name, "Locked v1");

    let mut missing = parent.clone();
    missing.metadata.as_mut().unwrap()["artPackage"]["lockedArts"] = json!({});
    assert!(
        resolve_workflow_child_tool(&missing, "test.publisher/child", &registry)
            .expect("resolve missing lock")
            .is_none()
    );
    fs::remove_dir_all(root).expect("cleanup locked child resolution root");
}
