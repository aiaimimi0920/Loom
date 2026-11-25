// Real HTTP, signed device authentication, immutable install and independent grant verification.
mod local_art_http {
    use super::wall_http::{admin, device, pair, public};
    use super::*;
    include!("local_art_describe_http.rs");
    include!("local_art_service_fixture.rs");
    include!("local_art_service_http.rs");
    include!("otter_bridge_extension_cancel.rs");
    include!("local_art_service_images_http.rs");
    include!("local_art_service_image_output_http.rs");
    include!("local_art_service_confirmation_http.rs");
    include!("local_art_joint.rs");
    include!("local_art_result_http.rs");
    include!("local_art_cache_http.rs");
    include!("local_art_resume_http.rs");
    include!("local_art_resume_joint.rs");
    include!("local_art_package_http.rs");
    include!("local_art_ports_http.rs");
    include!("local_art_package_joint.rs");
    include!("local_art_live_viewer_http.rs");
    include!("local_art_live_control_http.rs");
    include!("local_art_live_trigger_fixture.rs");
    include!("local_art_live_trigger_http.rs");
    include!("local_art_live_trigger_lifecycle_tests.rs");
    include!("local_art_live_viewer_joint.rs");
    include!("local_art_live_trigger_native_joint.rs");
    include!("live_lan_native_joint.rs");
    include!("live_source_native_joint.rs");
    include!("live_viewer_source_native_joint.rs");
    include!("live_trigger_native_joint.rs");
    include!("live_h264_native_joint.rs");
    const SOURCE: &[u8] = b"NeuroSurface.define({mount({root}){root.textContent='local Art'}});";
    struct Fixture {
        root: PathBuf,
        port: u16,
        server: ConcurrencyTestFixture,
        device: String,
        token: String,
        request: Value,
        issuer: Value,
        runtime: SharedCapabilityRuntime,
        resources: SharedSurfaceResourceStore,
        live_sessions: SharedLiveSessionStore,
    }
    fn package(entry_path: &str, digest: &str, services: &[&str], source: &[u8]) -> Vec<u8> {
        package_scoped(entry_path, digest, services, source, false)
    }
    fn package_scoped(
        entry_path: &str,
        digest: &str,
        services: &[&str],
        source: &[u8],
        results: bool,
    ) -> Vec<u8> {
        let original = art_package_zip("local-counter", "1.0.0", b"not executed");
        let mut archive = zip::ZipArchive::new(Cursor::new(original)).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if entry.name().unwrap().as_ref() == "manifest.json" {
                let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
                manifest["metadata"]["otterExecution"] = json!({"sdkVersion":"otter.art.v1","entry":entry_path,
                    "sourceDigest":digest,"permissions":if services.is_empty(){json!(["local_state","preview"])}else{json!(["local_state","preview","loom_service"])},"services":services});
                if results {
                    manifest["metadata"]["otterExecution"]["permissions"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("workspace_result"));
                }
                bytes = serde_json::to_vec(&manifest).unwrap();
            }
            writer
                .start_file(
                    entry.name().unwrap(),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer
            .start_file("local.js", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(source).unwrap();
        writer.finish().unwrap().into_inner()
    }
    impl Fixture {
        fn new() -> Self {
            Self::with_source("local.js", &sha256_bytes(SOURCE))
        }
        fn with_source(entry: &str, source_digest: &str) -> Self {
            Self::build(entry, source_digest, false)
        }
        fn with_service() -> Self {
            Self::build("local.js", &sha256_bytes(SOURCE), true)
        }
        fn build(entry: &str, source_digest: &str, service: bool) -> Self {
            Self::build_source(entry, source_digest, service, SOURCE)
        }
        fn build_source(entry: &str, source_digest: &str, service: bool, source: &[u8]) -> Self {
            Self::build_scoped(entry, source_digest, service, source, false)
        }
        fn build_scoped(
            entry: &str,
            source_digest: &str,
            service: bool,
            source: &[u8],
            results: bool,
        ) -> Self {
            let services: Vec<&str> = if service { vec![SERVICE_ID] } else { vec![] };
            let package = if results {
                package_scoped(entry, source_digest, &services, source, true)
            } else {
                package(entry, source_digest, &services, source)
            };
            Self::build_package(&package, service)
        }
        fn build_package(package: &[u8], service: bool) -> Self {
            Self::build_package_images(package, service, false)
        }
        fn build_package_images(package: &[u8], service: bool, images: bool) -> Self {
            Self::build_package_service(package, service, images, false)
        }
        fn build_package_service(
            package: &[u8],
            service: bool,
            images: bool,
            gesture: bool,
        ) -> Self {
            Self::build_package_framework(
                package,
                service,
                images,
                gesture,
                &framework_package_zip("process", "1.0.0"),
            )
        }
        fn build_package_framework(
            package: &[u8],
            service: bool,
            images: bool,
            gesture: bool,
            framework: &[u8],
        ) -> Self {
            let root = unique_temp_dir(&format!("local-art-http-{}", Uuid::new_v4()));
            let daemon = LoomDaemon::bind(
                DaemonConfig::localhost(0)
                    .with_control_plane_root(&root)
                    .with_bounded_request_executor(4, 16),
            )
            .unwrap();
            daemon
                .runtime
                .framework_registry
                .install_framework_package_from_zip(framework)
                .unwrap();
            let services: Vec<&str> = if service { vec![SERVICE_ID] } else { vec![] };
            if service {
                if gesture {
                    install_local_service_fixture_options(
                        &root,
                        &daemon.runtime.capability_runtime,
                        images,
                        true,
                    );
                } else if images {
                    install_local_service_fixture_scoped(
                        &root,
                        &daemon.runtime.capability_runtime,
                        true,
                    );
                } else {
                    install_local_service_fixture(&root, &daemon.runtime.capability_runtime);
                }
            }
            loom_tool_registry::install::install_art_from_zip(
                package,
                &root,
                &daemon.runtime.framework_registry,
                &daemon.runtime.tool_registry,
            )
            .unwrap();
            let tool = daemon
                .runtime
                .tool_registry
                .get_tool("publisher.test/local-counter")
                .unwrap()
                .unwrap();
            let digest = tool.metadata.as_ref().unwrap()["artPackage"]["digest"]
                .as_str()
                .unwrap()
                .to_owned();
            let issuer = daemon.runtime.offline_peers.local_art_issuer().unwrap();
            let runtime = Arc::clone(&daemon.runtime.capability_runtime);
            let resources = Arc::clone(&daemon.runtime.surface_resources);
            let live_sessions = Arc::clone(&daemon.runtime.live_sessions);
            let port = daemon.local_addr().unwrap().port();
            let (tx, rx) = mpsc::channel();
            let server =
                ConcurrencyTestFixture::new(tx, thread::spawn(move || daemon.serve_until(rx)));
            let (device, token) = pair(port, "Local Art client");
            let request = json!({"binding":{"origin":format!("http://127.0.0.1:{port}"),"requestId":"request:1","deviceId":device,
                "workspaceId":"workspace:1","instanceId":"instance:1","generation":1,"artId":"publisher.test/local-counter",
                "artVersion":"1.0.0","packageDigest":digest,"sdkVersion":"otter.art.v1"},"permissions":if service{json!(["local_state","preview","loom_service"])}else{json!(["local_state","preview"])},"services":services});
            Self {
                root,
                port,
                server,
                device,
                token,
                request,
                issuer,
                runtime,
                resources,
                live_sessions,
            }
        }
        fn approve(&self, revision: u64, rows: bool) -> (u16, Value) {
            let b = &self.request["binding"];
            let approval = json!({"deviceId":self.device,"artId":b["artId"],"artVersion":b["artVersion"],"packageDigest":b["packageDigest"],
                "permissions":self.request["permissions"],"services":self.request["services"],"validUntilMs":unix_time_millis()+120_000,"offlineTtlMs":60_000});
            admin(
                self.port,
                "PUT",
                "/v1/art/local/approvals",
                Some(
                    json!({"expectedRevision":revision,"approvals":if rows{vec![approval]}else{vec![]} }),
                ),
            )
        }
        fn prepare(&self, mut request: Value) -> (u16, Value) {
            let object = request.as_object_mut().unwrap();
            object.entry("protocolVersion").or_insert_with(|| loom_protocol::local_art::package::visibility::PREPARE.into());
            object.entry("knownSourceDigest").or_insert_with(|| "0".repeat(64).into());
            device(
                self.port,
                &self.token,
                "POST",
                "/v3/art/local/prepare-package",
                Some(request),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.server.finish().expect("stop local Art test daemon");
            // 失败保留原始跨进程日志与夹具，成功才清理由本测试创建的临时目录。
            if !thread::panicking() {
                remove_test_dir(&self.root);
            }
        }
    }
    #[test]
    fn local_art_http_requires_explicit_scope_and_returns_verified_exact_source() {
        let f = Fixture::new();
        assert_eq!(
            public(
                f.port,
                "POST",
                "/v3/art/local/prepare-package",
                Some(f.request.clone())
            )
            .0,
            401
        );
        assert_eq!(
            admin(
                f.port,
                "POST",
                "/v3/art/local/prepare-package",
                Some(f.request.clone())
            )
            .0,
            403
        );
        assert_eq!(f.prepare(f.request.clone()).0, 403);
        assert_eq!(
            device(
                f.port,
                &f.token,
                "PUT",
                "/v1/art/local/approvals",
                Some(json!({}))
            )
            .0,
            403
        );
        assert_eq!(f.approve(0, true).0, 200);
        assert_eq!(f.approve(0, true).0, 409);
        let (status, response) = f.prepare(f.request.clone());
        assert_eq!(status, 200, "{response}");
        let grant = loom_plugin_security::local_art_grant::verify(
            &serde_json::to_vec(&response["grant"]).unwrap(),
            f.issuer["keyId"].as_str().unwrap(),
            f.issuer["publicKey"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(grant.binding.art_id, "publisher.test/local-counter");
        assert_eq!(grant.binding.request_id, "request:1");
        assert_eq!(grant.binding.device_id, f.device);
        assert_eq!(
            BASE64.decode(response["source"].as_str().unwrap()).unwrap(),
            SOURCE
        );
        assert_eq!(grant.source.digest, sha256_bytes(SOURCE));
        assert!(grant.offline_until_ms <= grant.expires_at_ms);
        assert_eq!(
            device(f.port, &f.token, "GET", "/v1/art/local/issuer", None).1,
            f.issuer
        );
        assert_eq!(f.approve(1, false).0, 200);
        assert_eq!(f.prepare(f.request.clone()).0, 403);
    }
    #[test]
    fn local_art_http_rejects_forged_identity_scope_and_package() {
        let f = Fixture::new();
        assert_eq!(f.approve(0, true).0, 200);
        for (field, value) in [
            ("deviceId", json!("other-device")),
            ("artId", json!("publisher.test/other")),
            ("artVersion", json!("9.0.0")),
            ("packageDigest", json!("b".repeat(64))),
        ] {
            let mut request = f.request.clone();
            request["binding"][field] = value;
            assert_eq!(f.prepare(request).0, 403, "{field}");
        }
        let mut request = f.request.clone();
        request["permissions"] = json!(["local_state", "preview", "loom_service"]);
        request["services"] = json!(["service:admin"]);
        assert_eq!(f.prepare(request).0, 403);
        let mut request = f.request.clone();
        request["binding"]["artId"] = json!("publisher.test/../escape");
        assert_eq!(f.prepare(request).0, 400);
        let mut request = f.request.clone();
        request["binding"]["origin"] = json!("https://host/path");
        assert_eq!(f.prepare(request).0, 400);
    }
    #[test]
    fn local_art_http_source_contract_and_corrupt_approval_fail_closed() {
        for (entry, digest) in [
            ("../local.js", sha256_bytes(SOURCE)),
            ("local.js", "0".repeat(64)),
        ] {
            let f = Fixture::with_source(entry, &digest);
            assert_eq!(f.approve(0, true).0, 200);
            let (status, error) = f.prepare(f.request.clone());
            assert_eq!(status, 409, "{error}");
            assert!(error.get("grant").is_none());
        }
        let f = Fixture::new();
        assert_eq!(f.approve(0, true).0, 200);
        fs::write(
            f.root.join("settings/local-art-approvals.json"),
            b"not valid JSON",
        )
        .unwrap();
        assert_eq!(f.prepare(f.request.clone()).0, 503);
        assert_eq!(
            fs::read(f.root.join("settings/local-art-approvals.json")).unwrap(),
            b"not valid JSON"
        );
    }
}
