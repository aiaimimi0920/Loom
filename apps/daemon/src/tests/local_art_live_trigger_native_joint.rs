// Real Source UIA, dual-ended WSS, native independent grant, public SDK-signed child, formal result, and server ACK.
mod local_trigger_joint {
    use super::*;
    fn spawn(
        exe: &Path,
        root: &Path,
        test: &str,
        var: &str,
        input: &Path,
        prefix: &str,
    ) -> joint::Otter {
        joint::Otter(
            Command::new(exe)
                .args([
                    &format!("desktop::application::remote_live::resources::tests::{test}"),
                    "--exact",
                    "--ignored",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(var, input)
                .stdout(fs::File::create(root.join(format!("{prefix}.stdout.log"))).unwrap())
                .stderr(fs::File::create(root.join(format!("{prefix}.stderr.log"))).unwrap())
                .spawn()
                .unwrap(),
        )
    }
    #[test]
    #[ignore = "explicit native local Art automatic trigger over real LAN and signed SDK child"]
    fn local_art_trigger_native_real_lan_formal_result_ack_and_stop() {
        let exe = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
        assert!(exe.is_absolute() && exe.is_file());
        let path = PathBuf::from(std::env::var_os("OTTER_ART_TRIGGER_EXAMPLE").unwrap());
        assert!(path.is_absolute());
        assert!(fs::metadata(&path).unwrap().len() <= 64 * 1024);
        let source = fs::read(path).unwrap();
        assert_eq!(
            sha256_bytes(&source).to_ascii_lowercase(),
            std::env::var("OTTER_ART_TRIGGER_SHA256")
                .unwrap()
                .to_ascii_lowercase()
        );
        let mut f = Fixture::build_scoped("local.js", &sha256_bytes(&source), false, &source, true);
        let source_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let (source_device, _) =
            super::super::wall_http::pair_with_key(f.port, "Local trigger Source", &source_key);
        let viewer_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let (viewer_device, token) =
            super::super::wall_http::pair_with_key(f.port, "Local trigger Viewer", &viewer_key);
        f.device = viewer_device.clone();
        f.token = token;
        f.request["binding"]["deviceId"] = json!(viewer_device);
        f.request["permissions"]
            .as_array_mut()
            .unwrap()
            .push(json!("workspace_result"));
        let lan = live_lan_update(&f, true);
        f.request["binding"]["origin"] = lan["endpoint"].clone();
        assert_eq!(f.approve(0, true).0, 200);
        let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
        let identity = |device: &str, key: &ed25519_dalek::SigningKey| json!({"schemaVersion":1,"deviceId":device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())});
        let source_input = f.root.join("source-input.json");
        fs::write(&source_input,serde_json::to_vec(&json!({"origin":lan["endpoint"],"certificate":info["certificate"],"identity":identity(&source_device,&source_key)})).unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut source = spawn(
            &exe,
            &f.root,
            "live_source::joint::source_observation_menu_real_loom_lan",
            "OTTER_LIVE_SOURCE_JOINT_INPUT",
            &source_input,
            "source",
        );
        live_lan_phase(&f, &mut source, "source-ready.json", deadline);
        let read = |name: &str| -> Value {
            serde_json::from_slice(&fs::read(f.root.join(name)).unwrap()).unwrap()
        };
        let source_ready = read("source-ready.json");
        let session = source_ready["session"].as_str().unwrap();
        let observation = loop {
            let snapshot = serde_json::to_value(f.live_sessions.get(session).unwrap()).unwrap();
            assert!(!snapshot["observations"]
                .to_string()
                .contains("password-secret"));
            if let Some(o) = snapshot["observations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["state"] == "stable" && o["value"]["value"]["text"] == "value-seven")
            {
                break o.clone();
            }
            assert!(
                Instant::now() < deadline,
                "stable Source UIA: {}",
                f.root.display()
            );
            thread::sleep(Duration::from_millis(5));
        };
        let input = f.root.join("viewer-input.json");
        fs::write(&input,serde_json::to_vec(&json!({"origin":lan["endpoint"],"certificate":info["certificate"],"identity":identity(&viewer_device,&viewer_key),
            "issuer":f.issuer,"session":session,"artId":"publisher.test/local-counter","observationId":observation["observationId"]})).unwrap()).unwrap();
        let mut viewer=spawn(&exe,&f.root,"live_viewer::joint::local_trigger::local_art_automatic_trigger_real_loom_lan_result_and_stop",
            "OTTER_LIVE_VIEWER_JOINT_INPUT",&input,"viewer");
        live_lan_phase(&f, &mut viewer, "local-trigger-updated.json", deadline);
        let current = f
            .live_sessions
            .get_for_member(session, &viewer_device)
            .unwrap();
        assert!(current.source_connected);
        assert_eq!(current.viewer_connections.get(&viewer_device), Some(&1));
        assert_eq!(current.requester_control.unwrap().control_sequence, 3);
        let inspection = read("local-trigger-private-inspect.json");
        let inspect = || {
            let (status, value) = device(
                f.port,
                &f.token,
                "POST",
                ART_TRIGGER_ROUTE,
                Some(inspection.clone()),
            );
            assert_eq!(status, 200, "{value}");
            value
        };
        let configured = inspect();
        assert_eq!(configured["status"], "idle");
        assert_eq!(configured["condition"]["revision"], 2);
        assert_eq!(
            configured["scope"],
            read("local-trigger-updated.json")["scope"]
        );
        fs::write(f.root.join("source-change"), b"change owned EDIT").unwrap();
        live_lan_phase(&f, &mut viewer, "local-trigger-result.json", deadline);
        let acknowledged = loop {
            let reply = inspect();
            if reply["status"] == "acknowledged" {
                break reply;
            }
            assert!(viewer.0.try_wait().unwrap().is_none());
            assert!(Instant::now() < deadline, "ACK: {}", f.root.display());
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(
            acknowledged["receipt"],
            json!({"kind":"applied","formalRevision":1})
        );
        assert_eq!(acknowledged["delivery"], Value::Null);
        let result = read("local-trigger-result.json");
        assert_eq!(result["data"]["deliveryId"], acknowledged["deliveryId"]);
        assert_eq!(result["data"]["count"], 1);
        assert_eq!(
            result["data"]["observation"]["value"]["text"],
            "value-eight"
        );
        assert_eq!(result["formalResult"], true);
        assert_eq!(result["saveRestore"], true);
        assert_eq!(result["editorUnchanged"], true);
        fs::write(f.root.join("local-trigger-stop"), b"explicit stop").unwrap();
        live_lan_phase(&f, &mut viewer, "local-trigger-stopped.json", deadline);
        let stopped = inspect();
        assert_eq!(stopped["status"], "stopped");
        assert_eq!(stopped["enabled"], false);
        assert_eq!(stopped["reason"], "user_stopped");
        assert_eq!(read("local-trigger-stopped.json")["preservedFormal"], true);
        fs::write(f.root.join("viewer-stop"), b"stop").unwrap();
        live_lan_phase(&f, &mut viewer, "viewer-complete.json", deadline);
        viewer_source_reap(&mut viewer, deadline, &f.root);
        fs::write(f.root.join("source-stop"), b"stop").unwrap();
        live_lan_phase(&f, &mut source, "source-complete.json", deadline);
        viewer_source_reap(&mut source, deadline, &f.root);
        loop {
            let after = f.live_sessions.get(session).unwrap();
            if after.closed
                && !after.source_connected
                && after.viewer_connections.is_empty()
                && f.live_sessions.media_connections.load(Ordering::SeqCst) == 0
            {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        if let Some(output) = std::env::var_os("OTTER_ART_TRIGGER_EVIDENCE_DIR") {
            let output = PathBuf::from(output);
            assert!(output.is_absolute());
            fs::create_dir(&output).unwrap();
            for name in [
                "source-ready.json",
                "source-complete.json",
                "local-trigger-updated.json",
                "local-trigger-result.json",
                "local-trigger-stopped.json",
                "viewer-complete.json",
                "source.stdout.log",
                "source.stderr.log",
                "viewer.stdout.log",
                "viewer.stderr.log",
            ] {
                fs::copy(f.root.join(name), output.join(name)).unwrap();
            }
            fs::write(output.join("server-ack.json"),serde_json::to_vec(&json!({"scope":acknowledged["scope"],"status":acknowledged["status"],
                "deliveryId":acknowledged["deliveryId"],"receipt":acknowledged["receipt"],"stopped":true,"mediaPermitsReclaimed":true})).unwrap()).unwrap();
        }
    }
}
