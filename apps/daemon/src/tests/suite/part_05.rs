// Loom daemon tests fragment 5; included into the shared crate test module.
fn art_package_zip(id: &str, version: &str, payload: &[u8]) -> Vec<u8> {
    let manifest = serde_json::json!({
        "id": id,
        "name": "Daemon package Art",
        "description": "daemon package integrity fixture",
        "enabled": true,
        "execution": { "type": "framework_art", "framework": "process" },
        "metadata": {
            "dependencies": { "framework": "process" },
            "art": { "qualifiedId": format!("publisher.test/{id}") },
            "packageSecurity": {
                "version": version,
                "publisher": { "id": "publisher.test", "name": "Publisher Test" }
            }
        }
    });
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        writer.start_file("bin/tool.exe", options).unwrap();
        writer.write_all(payload).unwrap();
        writer.start_file("art.runtime.json", options).unwrap();
        writer
                .write_all(
                    br#"{"protocolVersion":"loom.art.runtime.v1","entry":{"command":"bin/tool.exe","args":[]}}"#,
                )
                .unwrap();
        writer.finish().unwrap();
    }
    bytes
}

fn workflow_art_package_zip(id: &str, workflow_id: &str) -> Vec<u8> {
    let manifest = serde_json::json!({
        "id": id,
        "name": "Daemon workflow Art",
        "description": "daemon workflow package fixture",
        "enabled": true,
        "execution": { "type": "workflow", "workflowId": workflow_id },
        "metadata": {
            "dependencies": { "framework": "workflow" },
            "art": { "qualifiedId": format!("publisher.test/{id}") },
            "packageSecurity": {
                "version": "1.0.0",
                "publisher": { "id": "publisher.test", "name": "Publisher Test" }
            }
        }
    });
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        writer.start_file("workflow.yaml", options).unwrap();
        writer
            .write_all(b"name: Package Flow\nnodes: []\n")
            .unwrap();
        writer.finish().unwrap();
    }
    bytes
}

fn surface_art_package_zip(id: &str, version: &str, scene: &Value, instance_mode: &str) -> Vec<u8> {
    let manifest = serde_json::json!({
        "id": id,
        "name": "Surface package Art",
        "description": "declarative Surface fixture",
        "enabled": true,
        "execution": { "type": "framework_art", "framework": "process" },
        "metadata": {
            "dependencies": { "framework": "process" },
            "art": { "qualifiedId": format!("publisher.test/{id}") },
            "packageSecurity": {
                "version": version,
                "publisher": { "id": "publisher.test", "name": "Publisher Test" }
            },
            "capabilities": {
                "surface": {
                    "protocolVersion": "loom.surface.v1",
                    "apiVersion": "1.0",
                    "instanceMode": instance_mode,
                    "variants": [{
                        "runtime": "declarative",
                        "entry": "surface/main.json"
                    }],
                    "views": [{
                        "id": "full",
                        "label": "Full",
                        "fullSize": { "width": 640, "height": 480 }
                    }],
                    "defaultViewId": "full",
                    "requiredNodes": ["column", "text", "button"],
                    "actions": [{
                        "id": "refresh_price",
                        "risk": "low",
                        "offlinePolicy": "reject",
                        "concurrency": "replace_latest",
                        "idempotent": true,
                        "confirmation": false,
                        "cancelable": true,
                        "timeoutMs": 5000,
                        "progress": true
                    }]
                }
            }
        }
    });
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        writer.start_file("bin/tool.exe", options).unwrap();
        writer.write_all(b"surface-fixture").unwrap();
        writer.start_file("art.runtime.json", options).unwrap();
        writer
                .write_all(
                    br#"{"protocolVersion":"loom.art.runtime.v1","entry":{"command":"bin/tool.exe","args":[]}}"#,
                )
                .unwrap();
        writer.start_file("surface/main.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(scene).unwrap().as_bytes())
            .unwrap();
        writer.finish().unwrap();
    }
    bytes
}

fn javascript_surface_art_package_zip(
    id: &str,
    version: &str,
    source: &[u8],
    fallback: &Value,
) -> Vec<u8> {
    let manifest = serde_json::json!({
        "id": id,
        "name": "JavaScript Surface package Art",
        "description": "sandboxed JavaScript Surface fixture",
        "enabled": true,
        "execution": { "type": "framework_art", "framework": "process" },
        "metadata": {
            "dependencies": { "framework": "process" },
            "art": { "qualifiedId": format!("publisher.test/{id}") },
            "packageSecurity": {
                "version": version,
                "publisher": { "id": "publisher.test", "name": "Publisher Test" }
            },
            "capabilities": {
                "surface": {
                    "protocolVersion": "loom.surface.v1",
                    "apiVersion": "1.0",
                    "variants": [{
                        "runtime": "javascript",
                        "entry": "surface/main.js",
                        "requiredCapabilities": ["surface.javascript.v1"]
                    }],
                    "fallbackScene": "surface/fallback.json",
                    "requiredNodes": ["column", "text", "button"],
                    "actions": [{
                        "id": "refresh_price",
                        "risk": "low",
                        "offlinePolicy": "reject",
                        "concurrency": "replace_latest",
                        "idempotent": true,
                        "confirmation": false,
                        "cancelable": true,
                        "timeoutMs": 5000,
                        "progress": true
                    }]
                }
            }
        }
    });
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        writer.start_file("bin/tool.exe", options).unwrap();
        writer.write_all(b"surface-fixture").unwrap();
        writer.start_file("art.runtime.json", options).unwrap();
        writer
                .write_all(
                    br#"{"protocolVersion":"loom.art.runtime.v1","entry":{"command":"bin/tool.exe","args":[]}}"#,
                )
                .unwrap();
        writer.start_file("surface/main.js", options).unwrap();
        writer.write_all(source).unwrap();
        writer.start_file("surface/fallback.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(fallback).unwrap().as_bytes())
            .unwrap();
        writer.finish().unwrap();
    }
    bytes
}

fn migrating_surface_art_package_zip(
    id: &str,
    version: &str,
    state_schema_version: u32,
    scene: &Value,
    migration: Option<&Value>,
) -> Vec<u8> {
    let migrations = migration
        .map(|_| {
            vec![json!({
                "from": 1,
                "to": state_schema_version,
                "entry": "migrations/1-2.json"
            })]
        })
        .unwrap_or_default();
    let manifest = json!({
        "id": id,
        "name": "Migrating Surface package Art",
        "description": "Surface state migration fixture",
        "enabled": true,
        "execution": { "type": "framework_art", "framework": "process" },
        "metadata": {
            "dependencies": { "framework": "process" },
            "art": { "qualifiedId": format!("publisher.test/{id}") },
            "packageSecurity": {
                "version": version,
                "publisher": { "id": "publisher.test", "name": "Publisher Test" }
            },
            "capabilities": {
                "surface": {
                    "protocolVersion": "loom.surface.v1",
                    "apiVersion": "1.0",
                    "stateSchemaVersion": state_schema_version,
                    "migrations": migrations,
                    "variants": [{
                        "runtime": "declarative",
                        "entry": "surface/main.json"
                    }],
                    "requiredNodes": ["column", "text"]
                }
            }
        }
    });
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        writer.start_file("bin/tool.exe", options).unwrap();
        writer.write_all(b"surface-fixture").unwrap();
        writer.start_file("art.runtime.json", options).unwrap();
        writer
                .write_all(
                    br#"{"protocolVersion":"loom.art.runtime.v1","entry":{"command":"bin/tool.exe","args":[]}}"#,
                )
                .unwrap();
        writer.start_file("surface/main.json", options).unwrap();
        writer
            .write_all(serde_json::to_string(scene).unwrap().as_bytes())
            .unwrap();
        if let Some(migration) = migration {
            writer.start_file("migrations/1-2.json", options).unwrap();
            writer
                .write_all(serde_json::to_string(migration).unwrap().as_bytes())
                .unwrap();
        }
        writer.finish().unwrap();
    }
    bytes
}

#[test]
fn framework_package_routes_cover_install_upgrade_disable_enable_uninstall() {
    let root = unique_temp_dir("framework-package-routes");
    let registry = FrameworkRegistry::new(&root);

    let install_body = serde_json::to_string(&json!({
        "zipBase64": format!(
            "data:application/zip;base64,{}",
            BASE64.encode(framework_package_zip("process", "1.0.0"))
        )
    }))
    .expect("install body");
    let (status, body) = install_framework_package(&install_body, &registry).expect("install");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("install response")["framework"]["version"],
        "1.0.0"
    );

    let (status, body) = set_framework_enabled("process", false, &registry).expect("disable");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("disable response")["framework"]["enabled"],
        false
    );

    let (status, body) = set_framework_enabled("process", true, &registry).expect("enable");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("enable response")["framework"]["ready"],
        true
    );

    let upgrade_body = serde_json::to_string(&json!({
        "zipBase64": BASE64.encode(framework_package_zip("process", "2.0.0"))
    }))
    .expect("upgrade body");
    let (status, body) =
        upgrade_framework_package("process", &upgrade_body, &registry).expect("upgrade");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("upgrade response")["framework"]["version"],
        "2.0.0"
    );

    let (status, body) = uninstall_framework("process", &registry).expect("uninstall");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("uninstall response")["framework"]["installed"],
        false
    );
    assert!(!root.join("frameworks").join("process").exists());

    fs::remove_dir_all(&root).ok();
}

#[test]
fn framework_doctor_reports_permission_mode_and_enforcement_matrix() {
    let root = unique_temp_dir("framework-permission-doctor");
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&framework_package_zip("process", "1.0.0"))
        .expect("install framework");
    let (status, body) = framework_doctor(&registry).expect("framework doctor");
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["permissionMode"], "audit");
    assert_eq!(body["enforcementMatrix"]["processTree"], "enforced");
    assert_eq!(
        body["enforcementMatrix"]["directNetwork"],
        "not-os-enforced"
    );
    let process = body["frameworks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|framework| framework["id"] == "process")
        .expect("process status");
    assert_eq!(process["strictCompatible"], true);
    assert!(process["declaredPermissions"].is_array());
    fs::remove_dir_all(&root).ok();
}
