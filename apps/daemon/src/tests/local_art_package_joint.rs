// Real HTTPS package resources -> online Otter exits -> Loom stops -> new Otter runs from full cached package within original expiry without live device delivery.
mod package_joint {
    use super::*;
    use ed25519_dalek::SigningKey;
    fn child(executable: &Path, root: &Path, input: &Path, stage: &str) {
        let stdout = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join(format!("package-{stage}.stdout.log")))
            .unwrap();
        let stderr = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join(format!("package-{stage}.stderr.log")))
            .unwrap();
        let mut child=super::joint::Otter(Command::new(executable).args(["device_session_client::tests::live_control::art_prepare::joint::package_joint::art_package_joint_real_loom","--exact","--ignored","--test-threads=1","--nocapture"]).env("OTTER_LOCAL_ART_JOINT_INPUT",input).env("OTTER_ART_RESUME_STAGE",stage).stdout(stdout).stderr(stderr).spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "package {stage} failed, evidence {}",
                    root.display()
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "package {stage} timed out, evidence {}",
                root.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    #[ignore = "explicit cross-repository package and stopped-Loom offline acceptance"]
    fn local_art_joint_package_resources_across_processes_with_loom_stopped() {
        let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
        assert!(executable.is_absolute() && executable.is_file());
        let zip=package_http::zip(|m| {
            // Intentional UTF-8/CJK custom port name literal to verify non-ASCII port signing across processes.
            m["inputs"]=json!([{"name":"原始图片","type":"image","defaultVisible":false}]);
            m["outputs"]=json!([{"name":"image","type":"image","defaultVisible":false},{"name":"answer","type":"number","defaultVisible":false}]);
        });
        let mut f = Fixture::build_package(&zip,false);
        f.request["permissions"].as_array_mut().unwrap().push(json!("workspace_result"));
        let key = SigningKey::generate(&mut OsRng);
        let (device, token) =
            super::super::wall_http::pair_with_key(f.port, "Otter package joint", &key);
        f.device = device.clone();
        f.token = token;
        f.request["binding"]["deviceId"] = device.into();
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let (status, lan) = admin(
            f.port,
            "PUT",
            "/v1/otter/connections/lan",
            Some(json!({"enabled":true,"host":"127.0.0.1","port":port})),
        );
        assert_eq!(status, 200);
        assert_eq!(lan["listening"], true);
        f.request["binding"]["origin"] = lan["endpoint"].clone();
        let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
        assert_eq!(f.approve(0, true).0, 200);
        let input = f.root.join("package-input.json");
        fs::write(&input,serde_json::to_vec(&json!({"request":f.request,"issuer":f.issuer,"certificate":info["certificate"],
            "expectedPorts":{"inputs":[{"name":"原始图片","type":"image"}],"outputs":[{"name":"image","type":"image"},{"name":"answer","type":"number"}]},
            "expectedVisibility":{"inputs":[false],"outputs":[false,false]},
            "identity":{"schemaVersion":1,"deviceId":f.device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())}})).unwrap()).unwrap();
        child(&executable, &f.root, &input, "online");
        f.server.finish().unwrap();
        child(&executable, &f.root, &input, "incomplete");
        child(&executable, &f.root, &input, "offline");
        let incomplete: Value = serde_json::from_slice(
            &fs::read(f.root.join("otter-package-incomplete.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(incomplete["incompleteRejected"], true);
        for stage in ["online", "offline"] {
            let result: Value = serde_json::from_slice(
                &fs::read(f.root.join(format!("otter-package-{stage}.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(result["packageResources"], true);
            assert_eq!(result["formalPixels"], true);
            assert_eq!(result["formalResult"], true);
            assert_eq!(result["signedPorts"], true);
            assert_eq!(result["signedVisibility"], true);
            if stage == "offline" {
                assert_eq!(result["deviceSession"], false);
            }
        }
        if let Some(output) = std::env::var_os("OTTER_ART_PACKAGE_EVIDENCE_DIR") {
            let output = PathBuf::from(output);
            assert!(output.is_absolute());
            fs::create_dir(&output).unwrap();
            for stage in ["online", "incomplete", "offline"] {
                for name in [
                    format!("otter-package-{stage}.json"),
                    format!("package-{stage}.stdout.log"),
                    format!("package-{stage}.stderr.log"),
                ] {
                    fs::copy(f.root.join(&name), output.join(name)).unwrap();
                }
            }
        }
    }
}
