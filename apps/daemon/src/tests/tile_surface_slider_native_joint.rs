// Real LAN Slider sampling and bounded coalescing -> original Surface executor; oracle never invokes Otter's implementation.
mod slider {
    use super::*;
    #[test]
    #[ignore = "explicit real Loom LAN Tile Slider and real Art formal result gate"]
    fn real_lan_native_tile_slider_continuous_commit_and_formal_result() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        let art = live_trigger_executable("OTTER_SLIDER_ART_EXE");
        let framework = live_trigger_executable("OTTER_TRIGGER_FRAMEWORK_EXE");
        let scene = json!({"protocolVersion":"loom.surface.v1","authoritativeState":{"price":0},"scene":{
            "id":"root","type":"column","layout":{"width":640,"height":480},"children":[
                {"id":"price","type":"text","layout":{"width":640,"height":40},"props":{"text":"0"}},
                {"id":"slider","type":"slider","layout":{"width":300,"height":60},"props":{"min":0,"max":100,"step":10,"value":0},
                    "events":{"input":"refresh_price","change":"refresh_price"}}]}});
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
            Some(json!({
            "hookNodeId":"source-art","deviceId":device,"capabilities":default_declarative_surface_host_capabilities()})),
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
        let mut child = f.child("slider_and_cleanup", false);
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
        f.phase(&mut child, "tile-slider-ready.json");
        signal(&f, "slider-start");
        f.phase(&mut child, "slider-started.json");
        wait_source(&f, &mut child, instance, |v| {
            v["authoritativeState"]["price"] == 50.0
                && v["eventAcks"]
                    .as_object()
                    .is_some_and(|a| a.len() == 1 && a.values().all(|a| a["status"] == "succeeded"))
        });
        f.phase(&mut child, "slider-first-presented.json");
        signal(&f, "slider-finish");
        f.phase(&mut child, "slider-finished.json");
        let final_state = wait_source(&f, &mut child, instance, |v| {
            v["authoritativeState"]["price"] == 100.0
                && v["latestResult"]["outputs"]["price"]["value"] == 100.0
                && v["pending"].as_array().is_none_or(|p| p.is_empty())
                && v["eventAcks"]
                    .as_object()
                    .is_some_and(|a| a.len() >= 3 && a.values().all(|a| a["status"] == "succeeded"))
        });
        f.phase(&mut child, "slider-final-presented.json");
        let calls = fs::read_to_string(f.root.join("slider-executions.txt")).unwrap();
        let actual: Vec<Value> = calls
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            actual.first(),
            Some(&json!({"class":"continuous","payload":{"value":50.0}}))
        );
        assert_eq!(
            actual.last(),
            Some(&json!({"class":"commit","payload":{"value":100.0}}))
        );
        assert_eq!(actual.iter().filter(|v| v["class"] == "commit").count(), 1);
        assert!(actual[1..actual.len() - 1]
            .iter()
            .all(|v| v["class"] == "continuous"));
        assert!(
            actual.len() < 20,
            "144 move samples must be bounded/coalesced: {actual:?}"
        );
        assert_eq!(
            actual.len(),
            final_state["eventAcks"].as_object().unwrap().len()
        );
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
        assert!(source(&f, instance)["attachments"]
            .get(attachment)
            .is_some());
        server.finish().unwrap();
        if let Some(path) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
            let path = PathBuf::from(path);
            fs::create_dir_all(&path).unwrap();
            fs::write(
                path.join("tile-slider-service-oracle.json"),
                serde_json::to_vec_pretty(&json!({
                "actual":actual,"formalValue":100.0,"sourcePreserved":true,"cleanup":cleanup,
                "ordinaryOutputExe":false,"finalSwapChainReadback":false}))
                .unwrap(),
            )
            .unwrap();
            for name in [
                "tile-slider-ready.json",
                "slider-started.json",
                "slider-first-presented.json",
                "slider-finished.json",
                "slider-final-presented.json",
                "tile-output-complete.json",
                "slider_and_cleanup.stdout.log",
                "slider_and_cleanup.stderr.log",
            ] {
                fs::copy(f.root.join(name), path.join(name)).unwrap();
            }
        }
    }
}
