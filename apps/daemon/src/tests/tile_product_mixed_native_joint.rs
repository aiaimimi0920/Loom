// 普通产品三类动态同屏；复用Source/Art fixture，不用帧号或源像素代替桌面oracle。
mod mixed_product {
    use super::super::mixed as sources;
    use super::*;
    fn install(f: &Joint, child: &mut Native, layout: &mut Value, resource: &Value) -> Value {
        let base = f.state()["revision"].as_u64().unwrap();
        let revision = base + 1;
        layout["revision"] = json!(revision);
        layout["placements"][0]["source"]["id"] = resource["resource"]["resourceId"].clone();
        let (status, value) = admin(
            f.port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision":base,"layout":layout,"activationDelayMs":300})),
        );
        assert_eq!(status, 200, "{value}");
        f.until(child, |s| {
            s["endpoints"][0]["appliedRevision"] == revision
                && s["endpoints"][0]["scene"]["prepared"] == true
        })
    }
    fn connections(f: &Joint, child: &mut Native, sessions: &SharedLiveSessionStore, count: usize) {
        while sessions.media_connections.load(Ordering::SeqCst) != count {
            f.alive(child);
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "explicit ordinary EXE visible image Live Art updates freeze black resume and source-preserving cleanup"]
    fn real_lan_ordinary_tile_product_mixed_visible_updates_and_cleanup() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        let (f, device, mut server, sessions) = setup_configure(true);
        let mut child = driver(&f, "ordinary_tile_product_mixed_visible_pixels_and_stop");
        let registered = f.until(&mut child, |s| {
            s["endpoints"]
                .as_array()
                .is_some_and(|a| a.len() == 1 && a[0]["online"] == true)
        });
        let endpoint = registered["endpoints"][0]["endpoint"].clone();
        assert_eq!(endpoint["deviceId"], device);
        let window = f.phase(&mut child, "product-window.json");
        assert_eq!(window["ordinaryOutputExe"], true);
        assert_eq!(window["noUserInputInjection"], true);
        assert_eq!(window["externalActivationRequired"], true);
        assert_eq!(endpoint["outputId"], window["output"]["outputId"]);
        assert_eq!(
            endpoint["pixelSize"],
            json!({"width":window["output"]["width"],"height":window["output"]["height"]})
        );
        let activation = f.phase(&mut child, "product-activated.json");
        assert_eq!(activation["observedOwnedForeground"], true);
        assert_eq!(activation["observedOwnedLauncher"], true);
        assert_eq!(
            activation["mode"],
            "externalComputerUseLauncherThenOwnedChildTransfer"
        );
        // 前台握手不消耗Source原15秒寿命；只在此后开启正常的真实Source会话。
        let picture = sources::image(&f, [255, 0, 0, 255]);
        let picture2 = sources::image(&f, [0, 0, 255, 255]);
        let art = sources::image(&f, [0, 255, 0, 255]);
        let art2 = sources::image(&f, [0, 255, 255, 255]);
        let (instance, attachment) = sources::source(&f, &device);
        sources::update(&f, &instance, &attachment, &art);
        let (mut source, ready, session, original_frame) = live_source(&f, &sessions);
        let mut layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":2,
            "bounds":{"x":0,"y":0,"width":4,"height":4},
            "tiles":[{"tileId":"physical-mixed","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":4,"height":4},"rotation":"deg0"}],
            "placements":[
                {"placementId":"picture","source":{"kind":"image","id":picture["resource"]["resourceId"]},"rect":{"x":0,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":false},
                {"placementId":"live","source":{"kind":"live","id":session},"rect":{"x":2,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":1,"interactive":false},
                {"placementId":"art","source":{"kind":"surface","id":instance},"rect":{"x":0,"y":2,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":2,"interactive":false}]});
        let first_state = install(&f, &mut child, &mut layout, &picture);
        let first = request(
            &f,
            &mut child,
            "mixed-first",
            first_state["revision"].clone(),
            json!([[255, 0, 0], [211, 83, 17], [0, 255, 0], [0, 0, 0]]),
        );
        assert_eq!(
            sources::read(&f, &instance)["attachments"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
        // 三路均从原owner换真实内容；预期为独立常量，不复用产品采样/合成函数。
        fs::write(f.root.join("source-pixels-change"), b"change").unwrap();
        sources::update(&f, &instance, &attachment, &art2);
        let updated_state = install(&f, &mut child, &mut layout, &picture2);
        let expected = json!([[0, 0, 255], [71, 29, 193], [0, 255, 255], [0, 0, 0]]);
        let updated = request(
            &f,
            &mut child,
            "mixed-updated",
            updated_state["revision"].clone(),
            expected.clone(),
        );
        let current = sessions.get(&session).unwrap();
        assert!(
            current.source_connected && current.last_frame_id > original_frame && !current.closed
        );
        assert!(current.session.viewer_devices.is_empty() && current.viewer_connections.is_empty());
        assert!(current.session.controller_device.is_none());
        let frozen_state = f.present(&mut child, "frozen");
        let frozen = request(
            &f,
            &mut child,
            "frozen",
            frozen_state["revision"].clone(),
            expected.clone(),
        );
        connections(&f, &mut child, &sessions, 1);
        assert!(sessions.get(&session).unwrap().source_connected);
        let black_state = f.present(&mut child, "black");
        let black = request(
            &f,
            &mut child,
            "black",
            black_state["revision"].clone(),
            json!([[0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0]]),
        );
        let resumed_state = f.present(&mut child, "running");
        let resumed = request(
            &f,
            &mut child,
            "resumed",
            resumed_state["revision"].clone(),
            expected,
        );
        connections(&f, &mut child, &sessions, 2);
        request(
            &f,
            &mut child,
            "stop",
            resumed_state["revision"].clone(),
            Value::Null,
        );
        f.joined(&mut child);
        let cleanup = f.phase(&mut child, "product-complete.json");
        assert_eq!(cleanup["normalCloseExit"], true);
        assert_eq!(cleanup["ownedWindowGone"], true);
        // 关闭产品只退休Tile自己的view/媒体；来源附件、权威值和Source继续存在。
        while sources::read(&f, &instance)["attachments"]
            .as_object()
            .unwrap()
            .len()
            != 1
        {
            assert!(Instant::now() < f.deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let preserved = sources::read(&f, &instance);
        assert!(preserved["attachments"].get(&attachment).is_some());
        assert_eq!(preserved["authoritativeState"]["value"], 7);
        connections(&f, &mut source, &sessions, 1);
        let live_preserved = sessions.get(&session).unwrap();
        assert!(live_preserved.source_connected && !live_preserved.closed);
        fs::write(f.root.join("source-stop"), b"stop").unwrap();
        let source_cleanup = f.phase(&mut source, "source-complete.json");
        f.joined(&mut source);
        while sessions.media_connections.load(Ordering::SeqCst) != 0 {
            assert!(Instant::now() < f.deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(sessions.get(&session).unwrap().closed);
        let final_state = f.state();
        assert_eq!(final_state["endpoints"][0]["online"], false);
        assert!(final_state["endpoints"][0]["appliedRevision"].is_null());
        let proof = json!({"registered":registered,"window":window,"externalActivation":activation,
            "first":first,"updated":updated,"frozen":frozen,"black":black,"resumed":resumed,"sourceMenu":ready,
            "cleanup":cleanup,"sourceCleanup":source_cleanup,"sourceAttachmentPreserved":true,"sourceAuthoritativeValue":7,
            "sourceSurvivedTileClose":true,"threeSourcesChanged":true,"noController":true,"noSurfaceViewer":true,
            "mediaConnections":0,"final":final_state,"physicalCrossDevice":false,"ordinarySourceExe":false});
        server.finish().unwrap();
        if let Some(path) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
            let path = PathBuf::from(path);
            assert!(path.is_absolute());
            fs::create_dir_all(&path).unwrap();
            fs::write(
                path.join("product-mixed-service-oracle.json"),
                serde_json::to_vec_pretty(&proof).unwrap(),
            )
            .unwrap();
            for name in [
                "product-window.json",
                "product-activated.json",
                "product-complete.json",
                "product-driver.stdout.log",
                "product-driver.stderr.log",
                "ordinary-tile.stdout.log",
                "ordinary-tile.stderr.log",
                "live-source.stdout.log",
                "live-source.stderr.log",
                "source-complete.json",
            ] {
                fs::copy(f.root.join(name), path.join(name)).unwrap();
            }
            for stage in ["mixed-first", "mixed-updated", "frozen", "black", "resumed"] {
                let name = format!("product-pixels-{stage}.json");
                fs::copy(f.root.join(&name), path.join(name)).unwrap();
            }
        }
    }
}
