// Loom daemon tests fragment 31; included into the shared crate test module.
#[test]
fn surface_instance_migration_is_explicit_remounts_and_can_rollback_exactly() {
    let root = unique_temp_dir("surface-instance-migration");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    runtime
        .framework_registry
        .install_framework_package_from_zip(&framework_package_zip("process", "1.0.0"))
        .expect("install process framework");
    let scene_v1 = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "scene": {
            "id": "root",
            "type": "column",
            "children": [{
                "id": "version",
                "type": "text",
                "props": { "text": "v1" }
            }]
        },
        "authoritativeState": {
            "legacy": "value",
            "remove": "yes"
        }
    });
    loom_tool_registry::install::install_art_from_zip(
        &migrating_surface_art_package_zip("surface-migrating", "1.0.0", 1, &scene_v1, None),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install Surface v1");

    let (status, created) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            "/v1/surfaces/instances",
            &[],
            Some(&json!({ "artId": "surface-migrating" }).to_string()),
        ),
    )
    .expect("create Surface v1 instance");
    assert_eq!(status, 201, "{created}");
    let created: Value = serde_json::from_str(&created).expect("created JSON");
    let instance_id = created["descriptor"]["instanceId"]
        .as_str()
        .expect("instance id")
        .to_owned();
    let digest_v1 = created["descriptor"]["packageDigest"]
        .as_str()
        .expect("v1 package digest")
        .to_owned();

    let attach_path = format!("/v1/surfaces/instances/{instance_id}/attachments");
    let (status, attached) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &attach_path,
            &[],
            Some(
                &json!({
                    "hookNodeId": "hook-node:surface-migrating",
                    "deviceId": "device-000-local",
                    "capabilities": default_declarative_surface_host_capabilities()
                })
                .to_string(),
            ),
        ),
    )
    .expect("attach Surface v1 instance");
    assert_eq!(status, 201, "{attached}");
    let attached: Value = serde_json::from_str(&attached).expect("attached JSON");
    let attachment_id = attached["descriptor"]["attachmentId"]
        .as_str()
        .expect("attachment id")
        .to_owned();
    let mount_path = format!("/v1/surfaces/instances/{instance_id}/mount");
    let (status, mounted_v1) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &mount_path,
            &[],
            Some(&json!({ "attachmentId": attachment_id }).to_string()),
        ),
    )
    .expect("mount Surface v1 instance");
    assert_eq!(status, 200, "{mounted_v1}");
    let mounted_v1: Value = serde_json::from_str(&mounted_v1).expect("mounted v1 JSON");
    let revision_v1 = mounted_v1["instance"]["attachments"][&attachment_id]["snapshot"]["revision"]
        .as_u64()
        .expect("v1 revision");

    let scene_v2 = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "scene": {
            "id": "root",
            "type": "column",
            "children": [{
                "id": "version",
                "type": "text",
                "props": { "text": "v2" }
            }]
        },
        "authoritativeState": { "mustNotReplaceMigratedState": true }
    });
    let migration_v2 = json!({
        "from": 1,
        "to": 2,
        "statePatch": {
            "schema": 2,
            "remove": null
        }
    });
    loom_tool_registry::install::install_art_from_zip(
        &migrating_surface_art_package_zip(
            "surface-migrating",
            "2.0.0",
            2,
            &scene_v2,
            Some(&migration_v2),
        ),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install Surface v2");
    let tool_v2 = runtime
        .tool_registry
        .get_tool("surface-migrating")
        .expect("read active v2")
        .expect("active v2 tool");
    let digest_v2 = tool_v2
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.pointer("/artPackage/digest"))
        .and_then(Value::as_str)
        .expect("v2 package digest")
        .to_owned();

    let migrate_path = format!("/v1/surfaces/instances/{instance_id}/migrate");
    let (status, migrated_v2) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &migrate_path,
            &[],
            Some(
                &json!({
                    "targetVersion": "2.0.0",
                    "targetDigest": digest_v2,
                    "expectedGeneration": 0
                })
                .to_string(),
            ),
        ),
    )
    .expect("migrate Surface to v2");
    assert_eq!(status, 200, "{migrated_v2}");
    let migrated_v2: Value = serde_json::from_str(&migrated_v2).expect("migrated v2 JSON");
    assert_eq!(migrated_v2["descriptor"]["artVersion"], "2.0.0");
    assert_eq!(migrated_v2["descriptor"]["stateSchemaVersion"], 2);
    assert_eq!(migrated_v2["descriptor"]["generation"], 1);
    assert_eq!(migrated_v2["authoritativeState"]["legacy"], "value");
    assert_eq!(migrated_v2["authoritativeState"]["schema"], 2);
    assert!(migrated_v2["authoritativeState"].get("remove").is_none());
    assert!(migrated_v2["authoritativeState"]
        .get("mustNotReplaceMigratedState")
        .is_none());
    assert_eq!(
        migrated_v2["attachments"][&attachment_id]["snapshot"]["artVersion"],
        "2.0.0"
    );
    assert!(
        migrated_v2["attachments"][&attachment_id]["snapshot"]["revision"]
            .as_u64()
            .expect("v2 revision")
            > revision_v1
    );
    assert_eq!(
        migrated_v2["attachments"][&attachment_id]["lifecycle"],
        "mounted"
    );
    assert_eq!(migrated_v2["migrationHistory"][0]["artVersion"], "1.0.0");

    let (status, rolled_back) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &migrate_path,
            &[],
            Some(
                &json!({
                    "targetVersion": "1.0.0",
                    "targetDigest": digest_v1,
                    "expectedGeneration": 1
                })
                .to_string(),
            ),
        ),
    )
    .expect("roll Surface back to v1");
    assert_eq!(status, 200, "{rolled_back}");
    let rolled_back: Value = serde_json::from_str(&rolled_back).expect("rollback JSON");
    assert_eq!(rolled_back["descriptor"]["artVersion"], "1.0.0");
    assert_eq!(rolled_back["descriptor"]["stateSchemaVersion"], 1);
    assert_eq!(rolled_back["descriptor"]["generation"], 2);
    assert_eq!(rolled_back["authoritativeState"]["legacy"], "value");
    assert_eq!(rolled_back["authoritativeState"]["remove"], "yes");
    assert!(rolled_back["authoritativeState"].get("schema").is_none());
    assert_eq!(
        rolled_back["attachments"][&attachment_id]["snapshot"]["artVersion"],
        "1.0.0"
    );

    let scene_v3 = json!({
        "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
        "scene": {
            "id": "root",
            "type": "column",
            "children": [{
                "id": "version",
                "type": "text",
                "props": { "text": "v3" }
            }]
        },
        "authoritativeState": {}
    });
    loom_tool_registry::install::install_art_from_zip(
        &migrating_surface_art_package_zip("surface-migrating", "3.0.0", 3, &scene_v3, None),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install Surface v3 without migration");
    let tool_v3 = runtime
        .tool_registry
        .get_tool("surface-migrating")
        .expect("read active v3")
        .expect("active v3 tool");
    let digest_v3 = tool_v3
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.pointer("/artPackage/digest"))
        .and_then(Value::as_str)
        .expect("v3 package digest")
        .to_owned();
    let (status, rejected) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &migrate_path,
            &[],
            Some(
                &json!({
                    "targetVersion": "3.0.0",
                    "targetDigest": digest_v3,
                    "expectedGeneration": 2
                })
                .to_string(),
            ),
        ),
    )
    .expect("reject incomplete Surface migration chain");
    assert_eq!(status, 409, "{rejected}");
    let rejected: Value = serde_json::from_str(&rejected).expect("rejection JSON");
    assert_eq!(rejected["error"]["code"], "surface_state_migration_failed");
    let persisted = runtime
        .surface_instances
        .lock()
        .expect("Surface instance store")
        .get(&instance_id)
        .expect("persisted Surface instance");
    assert_eq!(persisted.descriptor.art_version, "1.0.0");
    assert_eq!(persisted.descriptor.generation, 2);
    assert_eq!(persisted.authoritative_state["remove"], "yes");
    let _ = fs::remove_dir_all(root);
}

fn write_fixture_response(stdout: &mut impl Write, response: serde_json::Value) {
    writeln!(
        stdout,
        "\n{}",
        serde_json::to_string(&response).expect("serialize fixture response")
    )
    .expect("write fixture response");
    stdout.flush().expect("flush fixture response");
}
