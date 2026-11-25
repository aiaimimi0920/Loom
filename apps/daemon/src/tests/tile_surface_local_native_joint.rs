// 独立LAN/Art进程oracle：本地值不发动作、权威同步、按钮只提交文本表单。
mod local_values {
    use super::*;
    #[test]
    #[ignore = "explicit real Loom LAN Tile local values and text-only form result"]
    fn real_lan_native_tile_handlerless_values_authority_and_text_form_result() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        let art = live_trigger_executable("OTTER_TEXT_ART_EXE");
        let framework = live_trigger_executable("OTTER_TRIGGER_FRAMEWORK_EXE");
        let declaration = json!({"protocolVersion":"loom.surface.v1","authoritativeState":{"value":"initial"},"scene":{
            "id":"root","type":"column","layout":{"width":640,"height":480},"children":[
                {"id":"label","type":"text","layout":{"width":640,"height":40},"props":{"text":"initial"}},
                {"id":"field","type":"input","layout":{"width":300,"height":60},"props":{"value":"initial"}},
                {"id":"switch","type":"switch","layout":{"width":300,"height":40},"props":{"value":false}},
                {"id":"select","type":"select","layout":{"width":300,"height":50},"props":{"value":"a","options":[{"value":"a","label":"A"},{"value":"b","label":"B"}]}},
                {"id":"slider","type":"slider","layout":{"width":300,"height":40},"props":{"value":0,"min":0,"max":100,"step":10}},
                {"id":"send","type":"button","layout":{"width":300,"height":50},"props":{"label":"Send","includeSurfaceValues":true},"events":{"click":"refresh_price"}}]}});
        let package = live_trigger_replace_zip(
            surface_art_package_zip("wall-art", "1.0.0", &declaration, "independent"),
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
        let mut child = f.child("local_values_and_cleanup", false);
        let registered = f.until(&mut child, |s| {
            s["endpoints"]
                .as_array()
                .is_some_and(|v| v.len() == 1 && v[0]["online"] == true)
        });
        let endpoint = &registered["endpoints"][0]["endpoint"];
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
        f.phase(&mut child, "local-ready.json");
        signal(&f, "local-edit");
        let edited = f.phase(&mut child, "local-edited.json");
        assert_eq!(edited["queuedActions"], 0);
        assert_eq!(
            edited["values"],
            json!({"switch":false,"select":0,"slider":100.})
        );
        let no_events = source(&f, instance);
        assert!(no_events["eventAcks"].as_object().unwrap().is_empty());
        assert!(no_events["pendingEvents"].as_array().unwrap().is_empty());
        assert!(!f.root.join("text-executions.txt").exists());
        for label in ["same-source", "new-source"] {
            let source = source(&f, instance);
            let attachments = source["attachments"].as_object().unwrap();
            assert_eq!(
                attachments.len(),
                2,
                "only the test Source and Tile view may be present"
            );
            let mut targets = Vec::new();
            // put_snapshot仅更新声明attachment，不广播；两个真实测试宿主各有自己的快照。
            for record in attachments.values() {
                let mut snapshot = record["snapshot"].clone();
                snapshot["revision"] = json!(snapshot["revision"].as_u64().unwrap() + 1);
                snapshot["scene"]["children"][0]["props"]["text"] = label.into();
                if label == "new-source" {
                    for (index, value) in [(2, json!(true)), (3, json!("b")), (4, json!(20))] {
                        snapshot["scene"]["children"][index]["props"]["value"] = value;
                    }
                }
                targets.push(json!({"attachment":snapshot["attachmentId"],"revision":snapshot["revision"],"label":label}));
                let (status, changed) = admin(
                    f.port,
                    "PUT",
                    &format!("/v1/surfaces/instances/{instance}/snapshot"),
                    Some(snapshot),
                );
                assert_eq!(status, 200, "{changed}");
            }
            fs::write(
                f.root.join(format!("authority-targets-{label}.json")),
                serde_json::to_vec_pretty(&targets).unwrap(),
            )
            .unwrap();
            f.phase(&mut child, &format!("local-{label}.json"));
            assert!(source_no_acks(&f, instance));
        }
        signal(&f, "local-submit");
        let submitted = f.phase(&mut child, "local-submitted.json");
        assert_eq!(submitted["uiQueueAccepted"], true);
        let final_state = wait_source(&f, &mut child, instance, |v| {
            v["authoritativeState"]["form"] == "仅本地🙂"
                && v["latestResult"]["outputs"]["form"]["value"] == "仅本地🙂"
                && v["eventAcks"]
                    .as_object()
                    .is_some_and(|a| a.len() == 1 && a.values().all(|a| a["status"] == "succeeded"))
        });
        let presented = f.phase(&mut child, "local-result.json");
        assert_eq!(
            presented["scene"]["children"][1]["props"]["value"],
            "initial"
        );
        let actual: Vec<Value> = fs::read_to_string(f.root.join("text-executions.txt"))
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        let expected =
            json!([{"class":"discrete","payload":{"surfaceValues":{"field":"仅本地🙂"}}}]);
        assert_eq!(json!(actual), expected);
        signal(&f, "tile-stop");
        f.joined(&mut child);
        let cleanup: Value =
            serde_json::from_slice(&fs::read(f.root.join("tile-output-complete.json")).unwrap())
                .unwrap();
        assert_eq!(cleanup["threadsJoined"], true);
        assert_eq!(cleanup["artReleased"], true);
        assert_eq!(cleanup["localValuesCleared"], true);
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
            fs::write(path.join("service-oracle.json"),serde_json::to_vec_pretty(&json!({"actual":actual,"expected":expected,"edited":edited,"submitted":submitted,"presented":presented,"cleanup":cleanup,"final":final_state})).unwrap()).unwrap();
            for name in [
                "local_values_and_cleanup.stdout.log",
                "local_values_and_cleanup.stderr.log",
                "local-same-source.json",
                "local-new-source.json",
            ] {
                fs::copy(f.root.join(name), path.join(name)).unwrap();
            }
        }
    }
    fn source_no_acks(f: &Joint, instance: &str) -> bool {
        source(f, instance)["eventAcks"]
            .as_object()
            .is_some_and(|a| a.is_empty())
    }
}
