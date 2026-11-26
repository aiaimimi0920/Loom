// Real LAN -> offscreen Tile clicks and explicit choices -> original executor -> live framework/Art; independently verifies authoritative formal results.
mod actions {
    use super::*;
    include!("tile_surface_value_native_joint.rs");
    include!("tile_surface_slider_native_joint.rs");
    include!("tile_surface_text_native_joint.rs");
    include!("tile_surface_local_native_joint.rs");
    include!("live_trigger_native_fixture.rs");
    struct RootGuard(Vec<(&'static str, Option<std::ffi::OsString>)>);
    impl RootGuard {
        fn new(root: &Path) -> Self {
            let mut previous = Vec::new();
            // Managed child environment inherits only TEMP/TMP; test artifacts stay in the isolated root without writing into Art package directories.
            for key in ["LOOM_CONTROL_PLANE_ROOT", "TEMP", "TMP"] {
                previous.push((key, std::env::var_os(key)));
                std::env::set_var(key, root);
            }
            Self(previous)
        }
    }
    impl Drop for RootGuard {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                if let Some(v) = value {
                    std::env::set_var(key, v)
                } else {
                    std::env::remove_var(key)
                }
            }
        }
    }
    fn source(f: &Joint, instance: &str) -> Value {
        let (status, v) = admin(
            f.port,
            "GET",
            &format!("/v1/surfaces/instances/{instance}"),
            None,
        );
        assert_eq!(status, 200);
        v
    }
    fn wait_source(
        f: &Joint,
        child: &mut Native,
        instance: &str,
        check: impl Fn(&Value) -> bool,
    ) -> Value {
        let mut last = Value::Null;
        loop {
            if Instant::now() >= f.deadline {
                fs::write(
                    f.root.join("tile-action-source-at-deadline.json"),
                    serde_json::to_vec_pretty(&last).unwrap(),
                )
                .unwrap();
            }
            f.alive(child);
            let v = source(f, instance);
            if check(&v) {
                return v;
            }
            last = v;
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn awaiting(f: &Joint, child: &mut Native, instance: &str, count: usize) -> (String, Value) {
        let v = wait_source(f, child, instance, |v| {
            v["eventAcks"].as_object().is_some_and(|a| {
                a.len() == count && a.values().any(|a| a["status"] == "awaiting_confirmation")
            })
        });
        let event = v["eventAcks"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, a)| a["status"] == "awaiting_confirmation")
            .unwrap()
            .0
            .clone();
        (event, v)
    }
    fn signal(f: &Joint, name: &str) {
        fs::write(f.root.join(name), b"explicit test action").unwrap();
    }
    #[test]
    #[ignore = "explicit real Loom LAN Tile click, confirmation, cancellation and real Art terminal gate"]
    fn real_lan_native_tile_click_confirm_reject_cancel_and_formal_result() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        let art = live_trigger_executable("OTTER_TRIGGER_ART_EXE");
        let framework = live_trigger_executable("OTTER_TRIGGER_FRAMEWORK_EXE");
        let scene = json!({"protocolVersion":"loom.surface.v1","authoritativeState":{"price":100},"scene":{"id":"root","type":"column","layout":{"width":640,"height":480},"children":[
            {"id":"price","type":"text","layout":{"width":640,"height":40},"props":{"text":"100"}},
            {"id":"refresh","type":"button","layout":{"width":640,"height":100},"props":{"label":"Refresh"},"events":{"click":"refresh_price"}},
            {"id":"slow","type":"button","layout":{"width":640,"height":100},"props":{"label":"Slow","eventPayload":{"slow":true}},"events":{"click":"refresh_price"}}]}});
        let package =
            live_trigger_replace_zip(wall_confirmation_package(&scene), "bin/tool.exe", &art);
        let framework = live_trigger_replace_zip(
            framework_package_zip("process", "1.0.0"),
            "runtime/loom-framework-process.exe",
            &framework,
        );
        let (f, device, mut server, _sessions) = setup_packages(Some((package, framework)));
        let _root = RootGuard::new(&f.root);
        let (status, created) = admin(
            f.port,
            "POST",
            "/v1/surfaces/instances",
            Some(json!({"artId":"wall-art"})),
        );
        assert_eq!(status, 201, "{created}");
        let instance = created["descriptor"]["instanceId"].as_str().unwrap();
        let mut capabilities = default_declarative_surface_host_capabilities();
        capabilities.capabilities.push("remote_resources".into());
        let (status, attached) = admin(
            f.port,
            "POST",
            &format!("/v1/surfaces/instances/{instance}/attachments"),
            Some(json!({"hookNodeId":"source-art","deviceId":device,"capabilities":capabilities})),
        );
        assert_eq!(status, 201, "{attached}");
        let source_attachment = attached["descriptor"]["attachmentId"].as_str().unwrap();
        assert_eq!(
            admin(
                f.port,
                "POST",
                &format!("/v1/surfaces/instances/{instance}/mount"),
                Some(json!({"attachmentId":source_attachment}))
            )
            .0,
            200
        );
        let mut output = f.child("actions_and_cleanup", false);
        let registered = f.until(&mut output, |s| {
            s["endpoints"]
                .as_array()
                .is_some_and(|v| v.len() == 1 && v[0]["online"] == true)
        });
        let endpoint = &registered["endpoints"][0]["endpoint"];
        assert_eq!(endpoint["inputCapabilities"], json!(["pointer"]));
        let layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":2,"bounds":{"x":0,"y":0,"width":640,"height":480},
            "tiles":[{"tileId":"tile","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":640,"height":480},"rotation":"deg0"}],
            "placements":[{"placementId":"art","source":{"kind":"surface","id":instance},"rect":{"x":0,"y":0,"width":640,"height":480},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":true}]});
        assert_eq!(
            admin(
                f.port,
                "PUT",
                "/v1/walls/layouts",
                Some(json!({"baseRevision":1,"layout":layout}))
            )
            .0,
            200
        );
        f.until(&mut output, |s| s["endpoints"][0]["appliedRevision"] == 2);
        f.phase(&mut output, "tile-actions-ready.json");
        signal(&f, "click-reject");
        let (rejected, before) = awaiting(&f, &mut output, instance, 1);
        assert_eq!(before["authoritativeState"]["price"], 100);
        assert!(live_trigger_executions(&f.root).is_empty());
        signal(&f, "choice-none");
        f.phase(&mut output, "choice-none-done.json");
        assert_eq!(
            source(&f, instance)["eventAcks"][&rejected]["status"],
            "awaiting_confirmation"
        );
        // Identification preserves the layout/view; pending confirmation identity must remain explicitly rejectable across pause, real paint, and resume.
        let (status, identifying) = admin(
            f.port,
            "POST",
            "/v1/walls/endpoints/identify",
            Some(json!({"endpointId":endpoint["endpointId"]})),
        );
        assert_eq!(status, 200, "{identifying}");
        let applied = f.until(&mut output, |s| {
            s["endpoints"][0]["identification"]["applied"] == true
        });
        assert!(applied["endpoints"][0]["appliedRevision"].is_null());
        f.phase(&mut output, "tile-actions-identified.json");
        signal(&f, "tile-actions-dismiss");
        f.until(&mut output, |s| {
            s["endpoints"][0].get("identification").is_none()
                && s["endpoints"][0]["appliedRevision"] == 2
        });
        f.phase(&mut output, "tile-actions-resumed.json");
        let resumed = source(&f, instance);
        assert_eq!(
            resumed["eventAcks"][&rejected]["status"],
            "awaiting_confirmation"
        );
        assert_eq!(
            resumed["attachments"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            before["attachments"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
        signal(&f, "choice-reject");
        wait_source(&f, &mut output, instance, |v| {
            v["eventAcks"][&rejected]["status"] == "cancelled"
        });
        assert!(live_trigger_executions(&f.root).is_empty());
        signal(&f, "click-approve");
        let (approved, _) = awaiting(&f, &mut output, instance, 2);
        signal(&f, "choice-approve");
        let success = wait_source(&f, &mut output, instance, |v| {
            v["eventAcks"][&approved]["status"] == "succeeded"
        });
        assert_eq!(success["authoritativeState"]["price"], 101);
        assert_eq!(success["latestResult"]["outputs"]["price"]["value"], 101);
        let view = success["attachments"]
            .as_object()
            .unwrap()
            .keys()
            .find(|id| id.as_str() != source_attachment)
            .unwrap()
            .clone();
        assert_eq!(
            success["attachments"][&view]["snapshot"]["scene"]["children"][0]["props"]["text"],
            "101"
        );
        f.phase(&mut output, "tile-actions-result-presented.json");
        signal(&f, "click-slow");
        let (cancelled, _) = awaiting(&f, &mut output, instance, 3);
        signal(&f, "choice-approve-slow");
        wait_source(&f, &mut output, instance, |v| {
            v["eventAcks"][&cancelled]["status"] == "running"
                && live_trigger_executions(&f.root)
                    .iter()
                    .any(|p| fs::read(p).unwrap() == b"fs")
        });
        signal(&f, "cancel-running");
        wait_source(&f, &mut output, instance, |v| {
            v["eventAcks"][&cancelled]["status"] == "cancelled"
        });
        let until = Instant::now() + Duration::from_millis(2200);
        while Instant::now() < until {
            f.alive(&mut output);
            thread::sleep(Duration::from_millis(10));
        }
        let final_state = source(&f, instance);
        assert_eq!(final_state["authoritativeState"]["price"], 101);
        assert_eq!(final_state["latestResult"], success["latestResult"]);
        let calls = live_trigger_executions(&f.root);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], f.root.join("executions.txt"));
        assert_eq!(fs::read(&calls[0]).unwrap(), b"fs");
        signal(&f, "tile-stop");
        f.joined(&mut output);
        let complete: Value =
            serde_json::from_slice(&fs::read(f.root.join("tile-output-complete.json")).unwrap())
                .unwrap();
        assert_eq!(complete["threadsJoined"], true);
        assert_eq!(complete["controlsReleased"], true);
        while source(&f, instance)["attachments"]
            .as_object()
            .unwrap()
            .len()
            != 1
        {
            assert!(Instant::now() < f.deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(source(&f, instance)["attachments"]
            .get(source_attachment)
            .is_some());
        server.finish().unwrap();
        if let Some(dir) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
            let dir = PathBuf::from(dir);
            assert!(dir.is_absolute());
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("tile-action-service-oracle.json"),serde_json::to_vec_pretty(&json!({"rejectedEvent":rejected,"succeededEvent":approved,"cancelledEvent":cancelled,"authoritativePrice":101,"formalPrice":101,"executions":"fs","confirmationSurvivedIdentification":true,"sameViewAfterResume":true,"lateResultNotCommitted":true,"cleanup":complete,"ordinaryOutputExe":false,"finalSwapChainReadback":false})).unwrap()).unwrap();
            for name in [
                "actions_and_cleanup.stdout.log",
                "actions_and_cleanup.stderr.log",
            ] {
                fs::copy(f.root.join(name), dir.join(name)).unwrap();
            }
        }
    }
}
