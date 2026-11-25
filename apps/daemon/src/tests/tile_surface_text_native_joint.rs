// 真实LAN/实际Art进程oracle；每种控件独立原30秒期限，不扩大业务/网络超时。
mod text {
    use super::*;
    #[test]
    #[ignore = "explicit real Loom LAN native Tile text/textarea/number and handlerless form gate"]
    fn real_lan_native_tile_text_fields_ime_edges_and_formal_form_result() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        for (kind, text, expected, ime, handlers) in [
            ("input", "中文🙂", json!("中文🙂"), true, true),
            (
                "textarea",
                "第一行\r\n🙂第二行",
                json!("第一行\n🙂第二行"),
                true,
                true,
            ),
            ("number", "4", json!(4.0), false, true),
            ("input", "仅本地🙂", json!("仅本地🙂"), false, false),
        ] {
            run(kind, text, expected, ime, handlers, None);
        }
    }
    #[test]
    #[ignore = "explicit real Loom LAN Tile scroll/reveal and preserved form draft gate"]
    fn real_lan_native_tile_scroll_reveal_and_formal_form_result() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        for handlers in [true, false] {
            run(
                "input",
                "滚动🙂草稿",
                json!("滚动🙂草稿"),
                true,
                handlers,
                Some("wheel"),
            );
        }
    }
    #[test]
    #[ignore = "explicit real Loom LAN Tile scrollbar drag/page and preserved form draft gate"]
    fn real_lan_native_tile_scrollbar_drag_page_and_formal_form_result() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        for mode in ["drag", "page"] {
            for handlers in [true, false] {
                run(
                    "input",
                    "滚动🙂草稿",
                    json!("滚动🙂草稿"),
                    true,
                    handlers,
                    Some(mode),
                );
            }
        }
    }
    fn run(
        kind: &str,
        text: &str,
        expected: Value,
        ime: bool,
        handlers: bool,
        scroll_mode: Option<&str>,
    ) {
        let scrolling = scroll_mode.is_some();
        let scrollbar = scroll_mode.filter(|s| *s != "wheel");
        let art = live_trigger_executable("OTTER_TEXT_ART_EXE");
        let framework = live_trigger_executable("OTTER_TRIGGER_FRAMEWORK_EXE");
        let mut scene = json!({"protocolVersion":"loom.surface.v1","authoritativeState":{"value":"initial"},"scene":{
            "id":"root","type":"column","layout":{"width":640,"height":480},"children":[
                {"id":"label","type":"text","layout":{"width":640,"height":40},"props":{"text":"initial"}},
                {"id":"field","type":kind,"layout":{"width":300,"height":60},"props":{"value":if kind=="number" {"0"}else{"initial"}}},
                {"id":"send","type":"button","layout":{"width":300,"height":50},"props":{"label":"Send","includeSurfaceValues":true},"events":{"click":"refresh_price"}}]}});
        if handlers {
            scene["scene"]["children"][1]["events"] =
                json!({"input":"refresh_price","change":"refresh_price"});
        }
        if scrolling {
            scene["scene"]["type"] = json!("scroll");
            for (i, height) in [(0, 40), (1, 60), (2, 50)] {
                scene["scene"]["children"][i]["layout"]["minHeight"] = json!(height);
            }
            scene["scene"]["children"].as_array_mut().unwrap().insert(
                2,
                json!({"id":"gap","type":"view","layout":{"height":600,"minHeight":600}}),
            );
        }
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
        let mut child = f.child("text_and_cleanup", false);
        let registered = f.until(&mut child, |s| {
            s["endpoints"]
                .as_array()
                .is_some_and(|v| v.len() == 1 && v[0]["online"] == true)
        });
        let endpoint = &registered["endpoints"][0]["endpoint"];
        assert_eq!(endpoint["inputCapabilities"], json!(["pointer"]));
        let layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":2,"bounds":{"x":0,"y":0,"width":640,"height":480},"tiles":[{"tileId":"tile","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":640,"height":480},"rotation":"deg0"}],"placements":[{"placementId":"art","source":{"kind":"surface","id":instance},"rect":{"x":0,"y":0,"width":640,"height":480},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":true}]});
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
        f.phase(&mut child, "tile-text-ready.json");
        fs::write(
            f.root.join("text-request.pending"),
            serde_json::to_vec(
                &json!({"text":text,"ime":ime,"scrolling":scrolling,"scrollbar":scrollbar}),
            )
            .unwrap(),
        )
        .unwrap();
        fs::rename(
            f.root.join("text-request.pending"),
            f.root.join("text-request.json"),
        )
        .unwrap();
        f.phase(&mut child, "tile-text-submitted.json");
        let expected_text = expected
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}", expected.as_f64().unwrap()));
        let final_state = wait_source(&f, &mut child, instance, |v| {
            v["authoritativeState"]["form"] == expected_text
                && v["latestResult"]["outputs"]["form"]["value"] == expected_text
                && v["eventAcks"].as_object().is_some_and(|a| {
                    a.len() == if handlers { 3 } else { 1 }
                        && a.values().all(|a| a["status"] == "succeeded")
                })
        });
        let presented = f.phase(&mut child, "tile-text-presented.json");
        assert_eq!(
            presented["scene"]["children"][1]["props"]["value"],
            if handlers {
                json!(expected_text)
            } else {
                json!("initial")
            }
        );
        let actual: Vec<Value> = fs::read_to_string(f.root.join("text-executions.txt"))
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        let mut oracle = Vec::new();
        if handlers {
            oracle.push(json!({"class":"continuous","payload":{"value":expected}}));
            oracle.push(json!({"class":"commit","payload":{"value":expected}}));
        }
        oracle
            .push(json!({"class":"discrete","payload":{"surfaceValues":{"field":expected_text}}}));
        assert_eq!(actual, oracle);
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
            let path = PathBuf::from(path).join(format!(
                "{kind}-{handlers}{}",
                scrollbar.map(|s| format!("-{s}")).unwrap_or_default()
            ));
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("service-oracle.json"),serde_json::to_vec_pretty(&json!({"actual":actual,"expected":oracle,"cleanup":cleanup,"presented":presented,"installedImeManualTest":false})).unwrap()).unwrap();
            for name in [
                "tile-text-ready.json",
                "tile-text-submitted.json",
                "tile-text-presented.json",
                "tile-output-complete.json",
                "text_and_cleanup.stdout.log",
                "text_and_cleanup.stderr.log",
            ] {
                fs::copy(f.root.join(name), path.join(name)).unwrap();
            }
        }
    }
}
