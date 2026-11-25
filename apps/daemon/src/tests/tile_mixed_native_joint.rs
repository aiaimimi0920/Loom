// 三类真实来源共享同一Tile；不改生产状态/媒体队列/授权或期限。
mod mixed {
    use super::*;
    #[test]
    #[ignore = "explicit real Loom LAN image Live Art dynamic update and stop"]
    fn real_lan_native_tile_three_dynamic_sources_and_cleanup() {
        run(0);
    }
    #[test]
    #[ignore = "explicit real Loom LAN image Live Art mixed revocation cleanup"]
    fn real_lan_native_tile_three_sources_revocation_cannot_publish_late() {
        run(1);
    }
    #[test]
    #[ignore = "explicit real Loom LAN image Live Art freeze black resume and stop"]
    fn real_lan_native_tile_three_sources_freeze_black_resume() {
        run(2);
    }
    pub(super) fn image(f: &Joint, rgba: [u8; 4]) -> Value {
        let (status, resource) = admin(
            f.port,
            "POST",
            "/v1/surfaces/resources",
            Some(json!({
                "kind":"image","mime":"application/x-neuro-rgba8","width":2,"height":2,"dataBase64":BASE64.encode(rgba.repeat(4))
            })),
        );
        assert_eq!(status, 201, "{resource}");
        resource
    }
    pub(super) fn source(f: &Joint, device: &str) -> (String, String) {
        let (status, created) = admin(
            f.port,
            "POST",
            "/v1/surfaces/instances",
            Some(json!({"artId":"wall-art"})),
        );
        assert_eq!(status, 201, "{created}");
        let id = created["descriptor"]["instanceId"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut capabilities = default_declarative_surface_host_capabilities();
        capabilities.capabilities.push("remote_resources".into());
        let (status, attached) = admin(
            f.port,
            "POST",
            &format!("/v1/surfaces/instances/{id}/attachments"),
            Some(
                json!({"hookNodeId":"mixed-source-art","deviceId":device,"capabilities":capabilities}),
            ),
        );
        assert_eq!(status, 201, "{attached}");
        let attachment = attached["descriptor"]["attachmentId"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            admin(
                f.port,
                "POST",
                &format!("/v1/surfaces/instances/{id}/mount"),
                Some(json!({"attachmentId":attachment}))
            )
            .0,
            200
        );
        (id, attachment)
    }
    pub(super) fn read(f: &Joint, id: &str) -> Value {
        let (status, value) = admin(f.port, "GET", &format!("/v1/surfaces/instances/{id}"), None);
        assert_eq!(status, 200, "{value}");
        value
    }
    pub(super) fn update(f: &Joint, id: &str, attachment: &str, resource: &Value) {
        let mut snapshot = read(f, id)["attachments"][attachment]["snapshot"].clone();
        snapshot["revision"] = json!(snapshot["revision"].as_u64().unwrap() + 1);
        snapshot["resources"] = json!([resource["resource"]]);
        snapshot["resourceLeases"] = json!([resource]);
        snapshot["scene"] = json!({"id":"root","type":"image","props":{"resourceId":resource["resource"]["resourceId"]},"layout":{"width":640,"height":480}});
        let (status, value) = admin(
            f.port,
            "PUT",
            &format!("/v1/surfaces/instances/{id}/snapshot"),
            Some(snapshot),
        );
        assert_eq!(status, 200, "{value}");
    }
    fn run(mode: u8) {
        let revoked = mode == 1;
        let lifecycle = mode == 2;
        let (f, device, mut server, sessions) = setup_configure(true);
        if lifecycle {
            fs::write(f.root.join("tile-mixed-lifecycle"), b"lifecycle").unwrap();
        }
        let picture = image(&f, [255, 0, 0, 255]);
        let picture2 = image(&f, [0, 0, 255, 255]);
        let art_image = image(&f, [0, 255, 0, 255]);
        let art_image2 = image(&f, [0, 255, 255, 255]);
        let (instance, attachment) = source(&f, &device);
        update(&f, &instance, &attachment, &art_image);
        let (mut source, ready, session, original_frame) = live_source(&f, &sessions);
        let mut output = f.child("mixed_and_cleanup", revoked);
        let registered = f.until(&mut output, |s| {
            s["endpoints"]
                .as_array()
                .is_some_and(|a| a.len() == 1 && a[0]["online"] == true)
        });
        let endpoint = &registered["endpoints"][0]["endpoint"];
        assert_eq!(endpoint["pixelSize"], json!({"width":6,"height":2}));
        assert_eq!(
            endpoint["renderModes"],
            json!(["image", "raw_bgra", "surface_v1"])
        );
        let mut layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":2,"bounds":{"x":0,"y":0,"width":6,"height":2},
            "tiles":[{"tileId":"mixed-tile","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":6,"height":2},"rotation":"deg0"}],
            "placements":[
                {"placementId":"picture","source":{"kind":"image","id":picture["resource"]["resourceId"]},"rect":{"x":0,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":false},
                {"placementId":"live","source":{"kind":"live","id":session},"rect":{"x":2,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":1,"interactive":false},
                {"placementId":"art","source":{"kind":"surface","id":instance},"rect":{"x":4,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":2,"interactive":false}]});
        let (status, value) = admin(
            f.port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision":1,"layout":layout,"activationDelayMs":300})),
        );
        assert_eq!(status, 200, "{value}");
        let first = f.phase(&mut output, "tile-mixed-first.json");
        assert_eq!(
            first["pbgra"],
            json!([[0, 0, 255, 255], [17, 83, 211, 255], [0, 255, 0, 255]])
        );
        assert_eq!(
            read(&f, &instance)["attachments"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
        // 动态与呈现生命周期各用独立完整Source会话，保原15秒测试寿命，不延时或删检查。
        let expected = if lifecycle {
            first["pbgra"].clone()
        } else {
            // 三路各经原权威入口换内容；只改Source专属合成纹理，不写服务媒体store。
            fs::write(f.root.join("source-pixels-change"), b"change").unwrap();
            update(&f, &instance, &attachment, &art_image2);
            layout["revision"] = json!(3);
            layout["placements"][0]["source"]["id"] = picture2["resource"]["resourceId"].clone();
            let (status, value) = admin(
                f.port,
                "PUT",
                "/v1/walls/layouts",
                Some(json!({"baseRevision":2,"layout":layout,"activationDelayMs":300})),
            );
            assert_eq!(status, 200, "{value}");
            let updated = f.phase(&mut output, "tile-mixed-updated.json");
            let expected = json!([[255, 0, 0, 255], [193, 29, 71, 255], [255, 255, 0, 255]]);
            assert_eq!(updated["pbgra"], expected);
            expected
        };
        let current = sessions.get(&session).unwrap();
        assert!(
            current.source_connected && current.last_frame_id > original_frame && !current.closed
        );
        assert!(current.session.viewer_devices.is_empty() && current.viewer_connections.is_empty());
        assert!(current.session.controller_device.is_none());
        let mut proof = json!({"first":first,"expectedPbgra":expected,"sourceMenu":ready,"threeSourcesChanged":!lifecycle,"noController":true,"noSurfaceViewer":true});
        if revoked {
            assert_eq!(admin(f.port,"PUT",&format!("/v1/devices/{device}"),Some(json!({"name":"Mixed Tile","kind":"computer","address":"127.0.0.1","enabled":false}))).0,200);
        } else if lifecycle {
            proof["frozen"] = f.present(&mut output, "frozen");
            f.phase(&mut output, "tile-mixed-frozen.json");
            while sessions.media_connections.load(Ordering::SeqCst) != 1 {
                f.alive(&mut output);
                thread::sleep(Duration::from_millis(5));
            }
            assert!(sessions.get(&session).unwrap().source_connected);
            proof["black"] = f.present(&mut output, "black");
            f.phase(&mut output, "tile-mixed-black.json");
            proof["resumed"] = f.present(&mut output, "running");
            let resumed = f.phase(&mut output, "tile-mixed-resumed.json");
            assert_eq!(resumed["pbgra"], expected);
            proof["resumedPixels"] = resumed["pbgra"].clone();
            fs::write(f.root.join("tile-stop"), b"stop").unwrap();
        } else {
            fs::write(f.root.join("tile-stop"), b"stop").unwrap();
        }
        f.joined(&mut output);
        let complete: Value =
            serde_json::from_slice(&fs::read(f.root.join("tile-output-complete.json")).unwrap())
                .unwrap();
        assert_eq!(
            complete,
            json!({"threeSources":true,"changed":!lifecycle,"lifecycle":lifecycle,"revoked":revoked,"pixelsReleased":true,"sharedReleased":true,"threadsJoined":true,"ordinaryOutputExe":false,"finalSwapChainReadback":false})
        );
        while read(&f, &instance)["attachments"]
            .as_object()
            .unwrap()
            .len()
            != 1
        {
            assert!(Instant::now() < f.deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let preserved = read(&f, &instance);
        assert!(preserved["attachments"].get(&attachment).is_some());
        assert_eq!(preserved["authoritativeState"]["value"], 7);
        fs::write(f.root.join("source-stop"), b"stop").unwrap();
        f.phase(&mut source, "source-complete.json");
        f.joined(&mut source);
        while sessions.media_connections.load(Ordering::SeqCst) != 0 {
            assert!(Instant::now() < f.deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(sessions.get(&session).unwrap().closed);
        assert_eq!(f.state()["endpoints"][0]["online"], false);
        proof["cleanup"] = complete;
        proof["sourcePreserved"] = json!(true);
        proof["mediaConnections"] = json!(0);
        server.finish().unwrap();
        if let Some(dir) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
            let dir = PathBuf::from(dir);
            assert!(dir.is_absolute());
            fs::create_dir(&dir).unwrap();
            fs::write(
                dir.join("tile-mixed-service-oracle.json"),
                serde_json::to_vec_pretty(&proof).unwrap(),
            )
            .unwrap();
            for name in [
                "live-source.stdout.log",
                "live-source.stderr.log",
                "mixed_and_cleanup.stdout.log",
                "mixed_and_cleanup.stderr.log",
            ] {
                fs::copy(f.root.join(name), dir.join(name)).unwrap();
            }
        }
    }
}
