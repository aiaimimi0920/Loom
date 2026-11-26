// Real LAN discrete values -> original executor -> live framework/Art; expected values remain independent of Otter event encoding and rendering.
mod values {
    use super::*;
    #[test]
    #[ignore = "explicit real Loom LAN Tile discrete values and real Art terminal gate"]
    fn real_lan_native_tile_switch_select_cancel_and_formal_result() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        let art = live_trigger_executable("OTTER_VALUE_ART_EXE");
        let framework = live_trigger_executable("OTTER_TRIGGER_FRAMEWORK_EXE");
        let scene = json!({"protocolVersion":"loom.surface.v1","authoritativeState":{"price":100},"scene":{
            "id":"root","type":"column","layout":{"width":640,"height":480},"children":[
            {"id":"price","type":"text","layout":{"width":640,"height":40},"props":{"text":"100"}},
            {"id":"flag","type":"switch","props":{"value":false},"events":{"change":"refresh_price"}},
            {"id":"choice","type":"select","layout":{"width":300,"height":60},"props":{"value":"a","options":[
                {"value":"a","label":"A"},{"value":"b","label":"B & C"}]},"events":{"change":"refresh_price"}}]}});
        let package = live_trigger_replace_zip(
            surface_art_package_zip("wall-art", "1.0.0", &scene, "independent"),
            "bin/tool.exe",
            &art,
        );
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
        let (status, attached) = admin(
            f.port,
            "POST",
            &format!("/v1/surfaces/instances/{instance}/attachments"),
            Some(
                json!({"hookNodeId":"source-art","deviceId":device,"capabilities":default_declarative_surface_host_capabilities()}),
            ),
        );
        assert_eq!(status, 201, "{attached}");
        let attachment = attached["descriptor"]["attachmentId"].as_str().unwrap();
        assert_eq!(
            admin(
                f.port,
                "POST",
                &format!("/v1/surfaces/instances/{instance}/mount"),
                Some(json!({"attachmentId":attachment}))
            )
            .0,
            200
        );
        let mut child = f.child("values_and_cleanup", false);
        let registered = f.until(&mut child, |s| {
            s["endpoints"]
                .as_array()
                .is_some_and(|v| v.len() == 1 && v[0]["online"] == true)
        });
        let endpoint = &registered["endpoints"][0]["endpoint"];
        assert_eq!(endpoint["inputCapabilities"], json!(["pointer"]));
        let layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":2,
            "bounds":{"x":0,"y":0,"width":640,"height":480},"tiles":[{"tileId":"tile","endpointId":endpoint["endpointId"],
            "rect":{"x":0,"y":0,"width":640,"height":480},"rotation":"deg0"}],"placements":[{"placementId":"art",
            "source":{"kind":"surface","id":instance},"rect":{"x":0,"y":0,"width":640,"height":480},
            "sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":true}]});
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
        f.phase(&mut child, "tile-values-ready.json");
        for name in ["select-cancel", "select-current"] {
            signal(&f, name);
            f.phase(&mut child, &format!("{name}-done.json"));
            assert!(live_trigger_executions(&f.root).is_empty());
            assert_eq!(source(&f, instance)["eventAcks"], json!({}));
        }
        signal(&f, "switch-on");
        f.phase(&mut child, "switch-on-done.json");
        // Patches commit and release the lock before the formal result/ACK arrives; intermediate state must not be treated as action completion.
        let first = wait_source(&f, &mut child, instance, |v| {
            v["authoritativeState"]["price"] == 101
                && v["latestResult"]["outputs"]["price"]["value"] == 101
                && v["eventAcks"]
                    .as_object()
                    .is_some_and(|a| a.len() == 1 && a.values().all(|a| a["status"] == "succeeded"))
        });
        assert_eq!(first["latestResult"]["outputs"]["price"]["value"], 101);
        let painted = f.phase(&mut child, "flag-presented.json");
        assert_eq!(painted["scene"]["children"][1]["props"]["value"], true);
        signal(&f, "select-new");
        f.phase(&mut child, "select-new-done.json");
        let second = wait_source(&f, &mut child, instance, |v| {
            v["authoritativeState"]["price"] == 102
                && v["latestResult"]["outputs"]["price"]["value"] == 102
                && v["eventAcks"]
                    .as_object()
                    .is_some_and(|a| a.len() == 2 && a.values().all(|a| a["status"] == "succeeded"))
        });
        assert_eq!(second["latestResult"]["outputs"]["price"]["value"], 102);
        let painted = f.phase(&mut child, "choice-presented.json");
        assert_eq!(painted["scene"]["children"][1]["props"]["value"], true);
        assert_eq!(painted["scene"]["children"][2]["props"]["value"], "b");
        let calls = fs::read_to_string(f.root.join("executions.txt")).unwrap();
        let actual: Vec<Value> = calls
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(actual, vec![json!({"value":true}), json!({"value":"b"})]);
        assert_eq!(second["eventAcks"].as_object().unwrap().len(), 2);
        assert!(second["eventAcks"]
            .as_object()
            .unwrap()
            .values()
            .all(|a| a["status"] == "succeeded"));
        signal(&f, "tile-stop");
        f.joined(&mut child);
        let cleanup: Value =
            serde_json::from_slice(&fs::read(f.root.join("tile-output-complete.json")).unwrap())
                .unwrap();
        assert_eq!(cleanup["threadsJoined"], true);
        assert_eq!(cleanup["artReleased"], true);
        while source(&f, instance)["attachments"]
            .as_object()
            .unwrap()
            .len()
            != 1
        {
            assert!(Instant::now() < f.deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let final_state = source(&f, instance);
        assert!(final_state["attachments"].get(attachment).is_some());
        assert_eq!(final_state["authoritativeState"]["price"], 102);
        server.finish().unwrap();
        if let Some(path) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
            let path = PathBuf::from(path);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("tile-values-service-oracle.json"),serde_json::to_vec_pretty(&json!({
                "actualPayloads":actual,"formalValue":102,"sourcePreserved":true,"cleanup":cleanup,
                "textImeKeyboardAdvertised":false,"ordinaryOutputExe":false,"finalSwapChainReadback":false})).unwrap()).unwrap();
            for name in [
                "tile-values-ready.json",
                "select-cancel-done.json",
                "select-current-done.json",
                "switch-on-done.json",
                "flag-presented.json",
                "select-new-done.json",
                "choice-presented.json",
                "tile-output-complete.json",
                "values_and_cleanup.stdout.log",
                "values_and_cleanup.stderr.log",
            ] {
                fs::copy(f.root.join(name), path.join(name)).unwrap();
            }
        }
    }
}
