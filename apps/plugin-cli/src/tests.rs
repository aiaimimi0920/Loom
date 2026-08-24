// CLI, package, trust and process-contract regression coverage.
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    pub(super) fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "loom-plugin-cli-{label}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create test root");
        root
    }

    fn run_cli(args: &[String]) -> Result<String> {
        let mut output = Vec::new();
        run(args, &mut output)?;
        Ok(String::from_utf8(output).expect("CLI UTF-8"))
    }

    fn conformance_fixture(root: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let script = root.join("conformance-fixture.ps1");
            fs::write(
                &script,
                "$null = [Console]::In.ReadToEnd()\n[Console]::Out.Write('{\"status\":\"success\",\"output\":{\"content\":[]}}')\n",
            )
            .expect("write PowerShell fixture");
            let wrapper = root.join("conformance-fixture.cmd");
            fs::write(
                &wrapper,
                "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0conformance-fixture.ps1\"\r\n",
            )
            .expect("write command fixture");
            wrapper
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let wrapper = root.join("conformance-fixture.sh");
            fs::write(
                &wrapper,
                "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"status\":\"success\",\"output\":{\"content\":[]}}'\n",
            )
            .expect("write shell fixture");
            let mut permissions = fs::metadata(&wrapper).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&wrapper, permissions).unwrap();
            wrapper
        }
    }

    #[test]
    fn embedded_schemas_are_valid_json() {
        for schema in [
            schemas::FRAMEWORK_MANIFEST_V1,
            schemas::FRAMEWORK_EXECUTE_REQUEST_V1,
            schemas::FRAMEWORK_EXECUTE_RESPONSE_V1,
            schemas::FRAMEWORK_AUTHORING_V1,
            schemas::ART_RUNTIME_V1,
            schemas::SURFACE_MANIFEST_V1,
            schemas::SURFACE_MESSAGE_V1,
            schemas::SURFACE_SCENE_V1,
            schemas::SURFACE_STREAM_V1,
            schemas::DEVICE_SESSION_V1,
            schemas::HOOK_MESSAGE_V1,
        ] {
            serde_json::from_str::<Value>(schema).expect("schema JSON");
        }
    }

    #[test]
    fn help_lists_source_independent_workflow() {
        let mut output = Vec::new();
        run(["loom-plugin", "--help"], &mut output).expect("help");
        let output = String::from_utf8(output).expect("UTF-8");
        assert!(output.contains("init framework"));
        assert!(output.contains("conformance"));
        assert!(output.contains("pack"));
        assert!(output.contains("surface-manifest"));
        assert!(output.contains("hook-message"));
    }

    #[test]
    fn surface_art_validation_checks_scene_actions_and_confirmation() {
        let root = temp_root("surface-validation");
        fs::create_dir_all(root.join("runtime")).expect("runtime directory");
        fs::create_dir_all(root.join("surface")).expect("surface directory");
        fs::write(root.join("runtime/main.ps1"), b"exit 0\n").expect("runtime entry");
        write_pretty_json(
            root.join("art.runtime.json"),
            &json!({
                "protocolVersion": ART_RUNTIME_PROTOCOL_VERSION,
                "entry": {
                    "command": "runtime/main.ps1",
                    "args": []
                }
            }),
        )
        .expect("runtime manifest");
        let valid_scene = json!({
            "protocolVersion": "loom.surface.v1",
            "scene": {
                "id": "root",
                "type": "column",
                "children": [{
                    "id": "submit",
                    "type": "button",
                    "props": { "label": "Submit" },
                    "events": { "click": "submit" }
                }]
            },
            "authoritativeState": {}
        });
        write_pretty_json(root.join("surface/main.json"), &valid_scene).expect("Surface scene");
        let mut manifest = json!({
            "id": "surface-validator-fixture",
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": {
                "packageSecurity": {
                    "version": "1.0.0",
                    "publisher": { "id": "publisher.test" }
                },
                "art": { "qualifiedId": "publisher.test/surface-validator-fixture" },
                "capabilities": {
                    "surface": {
                        "protocolVersion": "loom.surface.v1",
                        "apiVersion": "1.0",
                        "variants": [{ "runtime": "declarative", "entry": "surface/main.json" }],
                        "requiredNodes": ["column", "button"],
                        "actions": [{
                            "id": "submit",
                            "risk": "high",
                            "offlinePolicy": "reject",
                            "concurrency": "serial",
                            "confirmation": true,
                            "timeoutMs": 1000
                        }]
                    }
                }
            }
        });
        write_pretty_json(root.join("manifest.json"), &manifest).expect("Art manifest");

        let valid = validate_path_with_payload(&root, true, &TrustStore::default())
            .expect("valid Surface Art");
        assert!(valid.contains("Art package valid"), "{valid}");

        let mut unknown_node_scene = valid_scene.clone();
        unknown_node_scene["scene"]["children"][0]["type"] = json!("webview");
        write_pretty_json(root.join("surface/main.json"), &unknown_node_scene)
            .expect("unknown-node Surface scene");
        let error = validate_path_with_payload(&root, true, &TrustStore::default())
            .expect_err("unknown declarative node must fail");
        assert!(error.to_string().contains("unknown node type"), "{error:#}");

        let mut undeclared_action_scene = valid_scene.clone();
        undeclared_action_scene["scene"]["children"][0]["events"]["click"] =
            json!("undeclared-admin-action");
        write_pretty_json(root.join("surface/main.json"), &undeclared_action_scene)
            .expect("undeclared-action Surface scene");
        let error = validate_path_with_payload(&root, true, &TrustStore::default())
            .expect_err("undeclared Surface action must fail");
        assert!(error.to_string().contains("undeclared action"), "{error:#}");

        write_pretty_json(root.join("surface/main.json"), &valid_scene)
            .expect("restore valid Surface scene");

        manifest["metadata"]["capabilities"]["surface"]["actions"][0]["confirmation"] =
            json!(false);
        write_pretty_json(root.join("manifest.json"), &manifest).expect("invalid manifest");
        let error = validate_path_with_payload(&root, true, &TrustStore::default())
            .expect_err("high-risk action without confirmation must fail");
        assert!(error.to_string().contains("host confirmation"), "{error:#}");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unsafe_package_ids_are_rejected() {
        let root = std::env::temp_dir().join("loom-plugin-cli-unsafe-id");
        let _ = fs::remove_dir_all(&root);
        assert!(init_framework(&root, "../escape", "publisher.example").is_err());
        assert!(init_framework(&root, "safe-id", "../publisher").is_err());
    }

    #[test]
    fn initialized_packages_are_publisher_qualified_and_validate() {
        let root = temp_root("publisher-qualified-init");
        let framework_dir = root.join("framework");
        init_framework(&framework_dir, "process", "publisher.example").expect("init framework");
        let framework = validate_path_with_payload(&framework_dir, false, &TrustStore::default())
            .expect("validate initialized framework");
        assert!(
            framework.contains("publisher.example/process"),
            "{framework}"
        );

        let art_dir = root.join("art");
        init_art(
            &art_dir,
            "sample-art",
            "publisher.example/process",
            "publisher.example",
        )
        .expect("init Art");
        let art = validate_path_with_payload(&art_dir, false, &TrustStore::default())
            .expect("validate initialized Art");
        assert!(art.contains("Art package valid"), "{art}");
        let manifest: Value = read_json(&art_dir.join("manifest.json")).expect("Art manifest");
        assert_eq!(
            manifest.pointer("/metadata/art/qualifiedId"),
            Some(&json!("publisher.example/sample-art"))
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn art_validation_rejects_missing_publisher() {
        let root = temp_root("missing-art-publisher");
        init_art(
            &root,
            "sample-art",
            "publisher.example/process",
            "publisher.example",
        )
        .expect("init Art");
        let manifest_path = root.join("manifest.json");
        let mut manifest: Value = read_json(&manifest_path).expect("Art manifest");
        manifest["metadata"]["packageSecurity"]
            .as_object_mut()
            .expect("packageSecurity object")
            .remove("publisher");
        write_pretty_json(manifest_path, &manifest).expect("write Art manifest");

        let error = validate_path_with_payload(&root, false, &TrustStore::default())
            .expect_err("publisher-less Art must fail");
        assert!(
            error
                .to_string()
                .contains("Art package publisher metadata is required"),
            "{error:#}"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn bounded_reader_rejects_excess_output() {
        let bytes = vec![b'x'; MAX_CONFORMANCE_OUTPUT_BYTES + 1];
        assert!(read_bounded(Cursor::new(bytes)).is_err());
    }

    #[test]
    fn signing_art_preserves_existing_package_security_metadata() {
        let root = temp_root("sign-art-metadata");
        let art_dir = root.join("art-package");
        let key_path = root.join("publisher-key.json");
        init_art(
            &art_dir,
            "signed-art",
            "publisher.example/process",
            "publisher.example",
        )
        .expect("init Art");

        let manifest_path = art_dir.join("manifest.json");
        let mut manifest: Value = read_json(&manifest_path).expect("read manifest");
        manifest["metadata"]["packageSecurity"] = json!({
            "version": "1.2.3",
            "publisher": {
                "id": "publisher.example",
                "name": "Publisher Example",
                "icon": "P"
            }
        });
        write_pretty_json(manifest_path.clone(), &manifest).expect("write manifest");

        let key = generate_signing_key("release-key-1");
        write_signing_key_document(&key_path, &key).expect("write key");
        sign_plugin_package(&art_dir, &key_path, "publisher.example").expect("sign Art");

        let signed: Value = read_json(&manifest_path).expect("read signed manifest");
        let security = &signed["metadata"]["packageSecurity"];
        assert_eq!(security["version"], "1.2.3");
        assert_eq!(security["publisher"]["name"], "Publisher Example");
        assert_eq!(security["publisher"]["icon"], "P");
        assert_eq!(security["publisher"]["keyId"], "release-key-1");
        assert_eq!(security["signature"]["file"], "signature.json");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn cli_sign_trust_pack_install_conformance_and_revoke_e2e() {
        let root = temp_root("e2e");
        let framework_dir = root.join("framework-package");
        let art_dir = root.join("art-package");
        let key_path = root.join("publisher-key.json");
        let trust_path = root.join("plugin-trust.json");
        let framework_zip = root.join("framework.zip");
        let art_zip = root.join("art.zip");
        let framework_id = "e2e-framework";
        let qualified_framework_id = "publisher.example/e2e-framework";

        run_cli(&[
            "loom-plugin".to_owned(),
            "init".to_owned(),
            "framework".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
            framework_id.to_owned(),
            "publisher.example".to_owned(),
        ])
        .expect("init framework");
        fs::write(
            framework_dir
                .join("runtime")
                .join(format!("{framework_id}.exe")),
            b"MZ-framework-fixture",
        )
        .expect("framework payload");
        run_cli(&[
            "loom-plugin".to_owned(),
            "keygen".to_owned(),
            key_path.to_string_lossy().into_owned(),
            "publisher-key".to_owned(),
        ])
        .expect("keygen");
        run_cli(&[
            "loom-plugin".to_owned(),
            "sign".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
            key_path.to_string_lossy().into_owned(),
            "publisher.example".to_owned(),
        ])
        .expect("sign framework");
        let verified = run_cli(&[
            "loom-plugin".to_owned(),
            "validate".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
        ])
        .expect("validate verified framework");
        assert!(verified.contains("trust=Verified"), "{verified}");
        run_cli(&[
            "loom-plugin".to_owned(),
            "trust".to_owned(),
            "add".to_owned(),
            trust_path.to_string_lossy().into_owned(),
            "publisher.example".to_owned(),
            key_path.to_string_lossy().into_owned(),
        ])
        .expect("trust publisher");
        let trusted = run_cli(&[
            "loom-plugin".to_owned(),
            "validate".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
            "--trust-store".to_owned(),
            trust_path.to_string_lossy().into_owned(),
        ])
        .expect("validate trusted framework");
        assert!(trusted.contains("trust=Trusted"), "{trusted}");
        run_cli(&[
            "loom-plugin".to_owned(),
            "pack".to_owned(),
            framework_dir.to_string_lossy().into_owned(),
            framework_zip.to_string_lossy().into_owned(),
        ])
        .expect("pack framework");
        assert!(framework_zip.is_file());
        assert!(root.join("framework.zip.sha256").is_file());

        let framework_registry = loom_tool_registry::framework::FrameworkRegistry::new(&root);
        let installed_framework = framework_registry
            .install_framework_package_from_zip(&fs::read(&framework_zip).unwrap())
            .expect("install packed framework");
        assert_eq!(installed_framework.qualified_id, qualified_framework_id);
        assert!(installed_framework.ready);

        run_cli(&[
            "loom-plugin".to_owned(),
            "init".to_owned(),
            "art".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            "e2e-art".to_owned(),
            qualified_framework_id.to_owned(),
            "publisher.example".to_owned(),
        ])
        .expect("init Art");
        fs::write(art_dir.join("runtime/main.exe"), b"MZ-art-fixture").expect("Art payload");
        run_cli(&[
            "loom-plugin".to_owned(),
            "sign".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            key_path.to_string_lossy().into_owned(),
            "publisher.example".to_owned(),
        ])
        .expect("sign Art");
        let trusted_art = run_cli(&[
            "loom-plugin".to_owned(),
            "validate".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            "--trust-store".to_owned(),
            trust_path.to_string_lossy().into_owned(),
        ])
        .expect("validate trusted Art");
        assert!(trusted_art.contains("trust=Trusted"), "{trusted_art}");
        run_cli(&[
            "loom-plugin".to_owned(),
            "pack".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            art_zip.to_string_lossy().into_owned(),
        ])
        .expect("pack Art");

        let tool_registry = loom_tool_registry::ToolRegistry::new(root.join("tools"));
        let installed_art = loom_tool_registry::install::install_art_from_zip(
            &fs::read(&art_zip).unwrap(),
            &root,
            &framework_registry,
            &tool_registry,
        )
        .expect("install packed Art");
        assert_eq!(installed_art.tool_id, "e2e-art");
        assert_eq!(installed_art.framework, qualified_framework_id);

        let fixture = conformance_fixture(&root);
        let conformance = run_cli(&[
            "loom-plugin".to_owned(),
            "conformance".to_owned(),
            fixture.to_string_lossy().into_owned(),
            qualified_framework_id.to_owned(),
            art_dir.to_string_lossy().into_owned(),
        ])
        .expect("run conformance");
        assert!(conformance.contains("conformance passed"), "{conformance}");

        run_cli(&[
            "loom-plugin".to_owned(),
            "trust".to_owned(),
            "revoke".to_owned(),
            trust_path.to_string_lossy().into_owned(),
            "publisher.example".to_owned(),
            "publisher-key".to_owned(),
        ])
        .expect("revoke publisher");
        let revoked = run_cli(&[
            "loom-plugin".to_owned(),
            "validate".to_owned(),
            art_dir.to_string_lossy().into_owned(),
            "--trust-store".to_owned(),
            trust_path.to_string_lossy().into_owned(),
        ]);
        assert!(revoked
            .expect_err("revoked package must be rejected")
            .to_string()
            .contains("revoked"));
        fs::remove_dir_all(&root).ok();
    }
}
