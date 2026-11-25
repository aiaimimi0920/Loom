// Source实际UIA→Viewer生产配置接缝→LAN→原Surface executor→真实framework/Art进程与最终业务提交。
include!("live_trigger_native_fixture.rs");
#[test]
#[ignore = "explicit native Source UIA, Viewer trigger and real packaged Surface action executables"]
fn live_trigger_native_configuration_and_surface_terminal_result() {
    let _env = lock_ignoring_poison(&ENV_LOCK);
    let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
    assert!(executable.is_absolute() && executable.is_file());
    let art = live_trigger_executable("OTTER_TRIGGER_ART_EXE");
    let framework = live_trigger_executable("OTTER_TRIGGER_FRAMEWORK_EXE");
    let package = live_trigger_replace_zip(
        surface_art_package_zip(
            "local-counter",
            "1.0.0",
            &json!({
            "protocolVersion":"loom.surface.v1","authoritativeState":{"price":100},
                    "scene":{"id":"root","type":"column","children":[
                        {"id":"price","type":"text","props":{"text":"100"}},
                        {"id":"refresh","type":"button","props":{"text":"Refresh"},"events":{"click":"refresh_price"}}
                    ]}
                }),
            "independent",
        ),
        "bin/tool.exe",
        &art,
    );
    let framework = live_trigger_replace_zip(
        framework_package_zip("process", "1.0.0"),
        "runtime/loom-framework-process.exe",
        &framework,
    );
    let f = Fixture::build_package_framework(&package, false, false, false, &framework);
    struct RootGuard(Option<std::ffi::OsString>);
    impl Drop for RootGuard {
        fn drop(&mut self) {
            if let Some(root) = self.0.take() {
                std::env::set_var("LOOM_CONTROL_PLANE_ROOT", root)
            } else {
                std::env::remove_var("LOOM_CONTROL_PLANE_ROOT")
            }
        }
    }
    let _root = RootGuard(std::env::var_os("LOOM_CONTROL_PLANE_ROOT"));
    std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &f.root);
    let source_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (source_device, _) = super::wall_http::pair_with_key(f.port, "Trigger Source", &source_key);
    let viewer_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
    let (viewer_device, _) = super::wall_http::pair_with_key(f.port, "Trigger Viewer", &viewer_key);
    let lan = live_lan_update(&f, true);
    let (_, info) = admin(f.port, "GET", "/v1/otter/identity", None);
    let identity = |device: &str, key: &ed25519_dalek::SigningKey| json!({"schemaVersion":1,"deviceId":device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())});
    let source_input = f.root.join("source-input.json");
    fs::write(&source_input,serde_json::to_vec(&json!({"origin":lan["endpoint"],"certificate":info["certificate"],"identity":identity(&source_device,&source_key)})).unwrap()).unwrap();
    let spawn = |test: &str, variable: &str, input: &PathBuf, prefix: &str| {
        joint::Otter(
            Command::new(&executable)
                .args([
                    &format!("desktop::application::remote_live::resources::tests::{test}"),
                    "--exact",
                    "--ignored",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env(variable, input)
                .stdout(fs::File::create(f.root.join(format!("{prefix}.stdout.log"))).unwrap())
                .stderr(fs::File::create(f.root.join(format!("{prefix}.stderr.log"))).unwrap())
                .spawn()
                .unwrap(),
        )
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut source = spawn(
        "live_source::joint::source_observation_menu_real_loom_lan",
        "OTTER_LIVE_SOURCE_JOINT_INPUT",
        &source_input,
        "source",
    );
    live_lan_phase(&f, &mut source, "source-ready.json", deadline);
    let read = |name: &str| -> Value {
        serde_json::from_slice(&fs::read(f.root.join(name)).unwrap()).unwrap()
    };
    let ready = read("source-ready.json");
    let session = ready["session"].as_str().unwrap();
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
            "initial stable observation: {}",
            f.root.display()
        );
        thread::sleep(Duration::from_millis(5));
    };
    let viewer_input = f.root.join("viewer-input.json");
    fs::write(&viewer_input,serde_json::to_vec(&json!({"origin":lan["endpoint"],"certificate":info["certificate"],"identity":identity(&viewer_device,&viewer_key),"session":session,"artId":"publisher.test/local-counter","observationId":observation["observationId"]})).unwrap()).unwrap();
    let mut viewer = spawn(
        "live_viewer::joint::viewer_trigger_real_loom_surface_action",
        "OTTER_LIVE_VIEWER_JOINT_INPUT",
        &viewer_input,
        "viewer",
    );
    live_lan_phase(&f, &mut viewer, "viewer-ready.json", deadline);
    live_lan_phase(&f, &mut viewer, "viewer-trigger-added.json", deadline);
    live_lan_phase(&f, &mut viewer, "viewer-trigger-updated.json", deadline);
    let host = read("viewer-ready.json");
    let surface_path = format!(
        "/v1/surfaces/instances/{}",
        host["instance"].as_str().unwrap()
    );
    let before = f
        .live_sessions
        .get_for_member(session, &viewer_device)
        .unwrap();
    assert_eq!(before.viewer_connections.get(&viewer_device), Some(&1));
    assert_eq!(before.requester_control.unwrap().control_sequence, 3);
    let registrations = serde_json::to_value(&before.triggers).unwrap();
    assert_eq!(registrations[0]["binding"]["authorizedBy"], viewer_device);
    assert_eq!(registrations[0]["condition"]["revision"], 2);
    assert_eq!(
        registrations[0]["target"]["surfaceAttachmentId"],
        host["attachment"]
    );
    assert!(live_trigger_executions(&f.root).is_empty());
    fs::write(f.root.join("source-change"), b"change owned EDIT").unwrap();
    let (audit, surface) = loop {
        let snapshot = serde_json::to_value(f.live_sessions.get(session).unwrap()).unwrap();
        let (_, surface) = admin(f.port, "GET", &surface_path, None);
        let final_audit = snapshot["triggerAudits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| {
                a["outcome"] == "fired"
                    && a["conditionRevision"] == 2
                    && a["actionRequestId"].is_string()
            });
        if let Some(a) = final_audit {
            let ack = &surface["eventAcks"][a["idempotencyKey"].as_str().unwrap()];
            if ack["status"] == "succeeded" {
                assert_eq!(ack["requestId"], a["actionRequestId"]);
                break (a.clone(), surface);
            }
            assert!(
                !matches!(ack["status"].as_str(), Some("failed" | "cancelled")),
                "Surface terminal failure: {ack}; fixture {}",
                f.root.display()
            );
        }
        assert!(
            Instant::now() < deadline,
            "trigger terminal timeout: {}; snapshot {snapshot}; surface {surface}",
            f.root.display()
        );
        thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(surface["authoritativeState"]["price"], 101);
    assert_eq!(
        surface["attachments"][host["attachment"].as_str().unwrap()]["snapshot"]["scene"]
            ["children"][0]["props"]["text"],
        "101"
    );
    assert_eq!(surface["latestResult"]["outputs"]["price"]["value"], 101);
    let calls = live_trigger_executions(&f.root);
    assert_eq!(calls.len(), 1);
    assert_eq!(fs::read(&calls[0]).unwrap(), b"1");
    fs::write(
        f.root.join("viewer-disable"),
        b"disable after actual terminal",
    )
    .unwrap();
    live_lan_phase(&f, &mut viewer, "viewer-trigger-disabled.json", deadline);
    let disabled = f
        .live_sessions
        .get_for_member(session, &viewer_device)
        .unwrap();
    assert_eq!(disabled.requester_control.unwrap().control_sequence, 4);
    assert!(!disabled.triggers[0].binding.enabled);
    assert_eq!(disabled.triggers[0].condition.revision, 3);
    let after = disabled
        .observations
        .iter()
        .find(|o| o.observation_id == observation["observationId"].as_str().unwrap())
        .unwrap()
        .sequence;
    loop {
        let s = f.live_sessions.get(session).unwrap();
        if s.observations.iter().any(|o| {
            o.observation_id == observation["observationId"].as_str().unwrap() && o.sequence > after
        }) {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fs::read(&calls[0]).unwrap(), b"1");
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
    if let Some(output) = std::env::var_os("OTTER_LIVE_TRIGGER_EVIDENCE_DIR") {
        let output = PathBuf::from(output);
        assert!(output.is_absolute());
        fs::create_dir(&output).unwrap();
        for name in [
            "source-ready.json",
            "source-complete.json",
            "source.stdout.log",
            "source.stderr.log",
            "viewer-ready.json",
            "viewer-trigger-added.json",
            "viewer-trigger-updated.json",
            "viewer-trigger-disabled.json",
            "viewer-complete.json",
            "viewer.stdout.log",
            "viewer.stderr.log",
        ] {
            fs::copy(f.root.join(name), output.join(name)).unwrap();
        }
        fs::write(output.join("joint-oracle.json"),serde_json::to_vec(&json!({"audit":audit,"surface":surface,"actualFrameworkAndArtProcesses":true,"executionCount":1,"disabledRevision":3,"mediaConnections":0,"sourceActualUia":true,"viewerConfigurationSeam":true,"ordinaryDualProduct":false,"manualMenuNavigation":false,"physicalCrossDevice":false})).unwrap()).unwrap();
    }
}
