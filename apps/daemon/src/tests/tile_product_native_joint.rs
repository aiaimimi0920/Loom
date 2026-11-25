// 普通Otter EXE真实物理输出：服务ACK与外部DWM桌面像素分开核，绝不把源RGBA当呈现证据。
#[cfg(windows)]
mod visible_product {
    use super::*;
    include!("tile_product_mixed_native_joint.rs");
    fn driver(f: &Joint, test: &str) -> Native {
        use std::os::windows::fs::OpenOptionsExt;
        let executable = PathBuf::from(std::env::var_os("OTTER_TILE_PRODUCT_TEST_EXE").unwrap());
        let canonical = executable.canonicalize().unwrap();
        let repo_tmp = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tmp");
        let allowed = [
            repo_tmp.as_path(),
            Path::new("C:/Users/Public/BuildStaging"),
            Path::new("C:/Users/Public/Project/Release"),
            Path::new("C:/Users/Public/nas_home/AI/GameEditor/linshi"),
        ];
        assert!(
            executable.is_absolute()
                && allowed
                    .iter()
                    .filter_map(|candidate| candidate.canonicalize().ok())
                    .any(|base| canonical != base && canonical.starts_with(&base))
        );
        let mut locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&executable)
            .unwrap();
        assert!(locked.metadata().unwrap().len() <= 128 * 1024 * 1024);
        let mut hash = sha2::Sha256::default();
        std::io::copy(&mut locked, &mut hash).unwrap();
        use sha2::Digest;
        assert_eq!(
            format!("{:x}", hash.finalize()),
            std::env::var("OTTER_TILE_PRODUCT_TEST_EXE_SHA256")
                .unwrap()
                .to_ascii_lowercase()
        );
        Native(
            Command::new(executable)
                .args([
                    // libtest --exact matches the integration module path as well as the leaf.
                    &format!("tile_product_runtime::{test}"),
                    "--exact",
                    "--ignored",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env("OTTER_TILE_VISIBLE_GATE", "1")
                .env("OTTER_TILE_VISIBLE_ACTIVATE", "1")
                .env("OTTER_TILE_JOINT_INPUT", &f.input)
                .env("OTTER_LOOM_TLS_ORIGIN", &f.origin)
                .env("OTTER_LOOM_TLS_CA_FILE", f.root.join("tile-ca.pem"))
                .stdout(fs::File::create(f.root.join("product-driver.stdout.log")).unwrap())
                .stderr(fs::File::create(f.root.join("product-driver.stderr.log")).unwrap())
                .spawn()
                .unwrap(),
        )
    }
    fn request(f: &Joint, child: &mut Native, name: &str, revision: Value, rgb: Value) -> Value {
        let pending = f.root.join(format!("product-request-{name}.pending"));
        fs::write(
            &pending,
            serde_json::to_vec(&json!({"revision":revision,"rgb":rgb})).unwrap(),
        )
        .unwrap();
        fs::rename(pending, f.root.join(format!("product-request-{name}.json"))).unwrap();
        if name == "stop" {
            return Value::Null;
        }
        let actual = f.phase(child, &format!("product-pixels-{name}.json"));
        assert_eq!(actual["expectedRgb"], rgb);
        assert_eq!(actual["requestRevision"], revision);
        assert_eq!(actual["ordinaryOutputExe"], true);
        assert_eq!(actual["desktopCompositedPixels"], true);
        assert_eq!(actual["directSwapChainReadback"], false);
        assert_eq!(actual["occlusionChecked"], true);
        let expected: Vec<Vec<u8>> = serde_json::from_value(rgb).unwrap();
        let received: Vec<Vec<u8>> = serde_json::from_value(actual["actualRgb"].clone()).unwrap();
        assert!(received
            .iter()
            .flatten()
            .zip(expected.iter().flatten())
            .all(|(a, b)| a.abs_diff(*b) <= 1));
        actual
    }
    fn layout(
        f: &Joint,
        child: &mut Native,
        endpoint: &Value,
        image: &Value,
        rotation: &str,
        crop: Value,
    ) -> Value {
        let base = f.state()["revision"].as_u64().unwrap();
        let revision = base + 1;
        let layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":revision,"bounds":{"x":0,"y":0,"width":2,"height":2},
            "tiles":[{"tileId":"physical-tile","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":2,"height":2},"rotation":rotation}],
            "placements":[{"placementId":"ordinary-image","source":{"kind":"image","id":image["resource"]["resourceId"]},"rect":{"x":0,"y":0,"width":2,"height":2},"sourceCrop":crop,"zIndex":0,"interactive":false}]});
        let (status, value) = admin(
            f.port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision":base,"layout":layout})),
        );
        assert_eq!(status, 200, "{value}");
        f.until(child, |s| {
            s["endpoints"][0]["appliedRevision"] == revision
                && s["endpoints"][0]["scene"]["prepared"] == true
        })
    }
    #[test]
    #[ignore = "explicit ordinary product visible physical output; brief owned full-screen color patterns, not a hidden HWND substitute"]
    fn real_lan_ordinary_tile_product_visible_pixels_black_resume_and_stop() {
        let _env = lock_ignoring_poison(&ENV_LOCK);
        let (f, device, mut server, _sessions) = setup();
        let mut child = driver(&f, "ordinary_tile_product_visible_pixels_and_stop");
        let registered = f.until(&mut child, |s| {
            s["endpoints"]
                .as_array()
                .is_some_and(|e| e.len() == 1 && e[0]["online"] == true)
        });
        let endpoint = registered["endpoints"][0]["endpoint"].clone();
        assert_eq!(endpoint["deviceId"], device);
        let window = f.phase(&mut child, "product-window.json");
        assert_eq!(window["ordinaryOutputExe"], true);
        assert_eq!(window["noUserInputInjection"], true);
        assert_eq!(window["driverNoFocusOrZOrderApi"], false);
        assert_eq!(window["noFocusOrZOrderChange"], false);
        assert_eq!(window["externalActivationRequired"], true);
        let activation = f.phase(&mut child, "product-activated.json");
        assert_eq!(activation["observedOwnedForeground"], true);
        assert_eq!(activation["driverNoActivationApi"], false);
        assert_eq!(activation["observedOwnedLauncher"], true);
        assert_eq!(
            activation["mode"],
            "externalComputerUseLauncherThenOwnedChildTransfer"
        );
        assert_eq!(endpoint["outputId"], window["output"]["outputId"]);
        assert_eq!(
            endpoint["pixelSize"],
            json!({"width":window["output"]["width"],"height":window["output"]["height"]})
        );
        // 四象限各2×2同色；quarter采样落在同色内部，不靠放宽容差掩盖crop线性滤波。
        let rgba = [
            255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 0,
            0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255,
        ];
        let (status, image) = admin(
            f.port,
            "POST",
            "/v1/surfaces/resources",
            Some(
                json!({"kind":"image","mime":"application/x-neuro-rgba8","width":4,"height":4,
            "dataBase64":BASE64.encode(rgba)}),
            ),
        );
        assert_eq!(status, 201, "{image}");
        let full = json!({"x":0,"y":0,"width":1,"height":1});
        let mut proof =
            json!({"registered":registered,"window":window,"externalActivation":activation});
        let running = layout(&f, &mut child, &endpoint, &image, "deg0", full.clone());
        proof["running"] = request(
            &f,
            &mut child,
            "running",
            running["revision"].clone(),
            json!([[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]]),
        );
        let rotated = layout(&f, &mut child, &endpoint, &image, "deg90", full.clone());
        proof["rotated"] = request(
            &f,
            &mut child,
            "rotated",
            rotated["revision"].clone(),
            json!([[0, 255, 0], [255, 255, 255], [255, 0, 0], [0, 0, 255]]),
        );
        let black = f.present(&mut child, "black");
        proof["black"] = request(
            &f,
            &mut child,
            "black",
            black["revision"].clone(),
            json!([[0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0]]),
        );
        let resumed = f.present(&mut child, "running");
        proof["resumed"] = request(
            &f,
            &mut child,
            "resumed",
            resumed["revision"].clone(),
            json!([[0, 255, 0], [255, 255, 255], [255, 0, 0], [0, 0, 255]]),
        );
        let cropped = layout(
            &f,
            &mut child,
            &endpoint,
            &image,
            "deg0",
            json!({"x":0.5,"y":0,"width":0.5,"height":1}),
        );
        let crop_pixels = json!([[0, 255, 0], [0, 255, 0], [255, 255, 255], [255, 255, 255]]);
        proof["cropped"] = request(
            &f,
            &mut child,
            "cropped",
            cropped["revision"].clone(),
            crop_pixels.clone(),
        );
        let frozen = f.present(&mut child, "frozen");
        proof["frozen"] = request(
            &f,
            &mut child,
            "frozen",
            frozen["revision"].clone(),
            crop_pixels,
        );
        request(
            &f,
            &mut child,
            "stop",
            frozen["revision"].clone(),
            Value::Null,
        );
        f.joined(&mut child);
        proof["cleanup"] = f.phase(&mut child, "product-complete.json");
        assert_eq!(proof["cleanup"]["normalCloseExit"], true);
        assert_eq!(proof["cleanup"]["ownedWindowGone"], true);
        proof["final"] = f.state();
        assert_eq!(proof["final"]["endpoints"][0]["online"], false);
        assert!(proof["final"]["endpoints"][0]["appliedRevision"].is_null());
        server.finish().unwrap();
        if let Some(path) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
            let path = PathBuf::from(path);
            assert!(path.is_absolute());
            fs::create_dir_all(&path).unwrap();
            fs::write(
                path.join("product-visible-service-oracle.json"),
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
            ] {
                fs::copy(f.root.join(name), path.join(name)).unwrap();
            }
            for name in [
                "running", "rotated", "black", "resumed", "cropped", "frozen",
            ] {
                let file = format!("product-pixels-{name}.json");
                fs::copy(f.root.join(&file), path.join(file)).unwrap();
            }
        }
    }
}
