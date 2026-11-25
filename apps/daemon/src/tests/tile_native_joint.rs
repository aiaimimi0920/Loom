// 真实LAN/签名授权/原生呈现联合；管理员只布置图像，不伪造终端register/connect/heartbeat。
mod tile_native_joint {
    use super::*;
    use std::process::{Child, Command};

    struct Native(Child);
    impl Drop for Native {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    struct Joint {
        root: PathBuf,
        port: u16,
        input: PathBuf,
        origin: String,
        deadline: Instant,
    }
    impl Joint {
        fn child(&self, mode: &str, revoked: bool) -> Native {
            let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
            assert!(executable.is_absolute() && executable.is_file());
            let mut command = Command::new(executable);
            command
                .args([
                    &format!("desktop::tile_terminal::manager::joint::tile_native_joint_{mode}"),
                    "--exact",
                    "--ignored",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env("OTTER_TILE_JOINT_INPUT", &self.input)
                .env("OTTER_LOOM_TLS_ORIGIN", &self.origin)
                .env("OTTER_LOOM_TLS_CA_FILE", self.root.join("tile-ca.pem"))
                .stdout(fs::File::create(self.root.join(format!("{mode}.stdout.log"))).unwrap())
                .stderr(fs::File::create(self.root.join(format!("{mode}.stderr.log"))).unwrap());
            if revoked {
                command.env("OTTER_TILE_JOINT_REVOKE", "1");
            } else {
                command.env_remove("OTTER_TILE_JOINT_REVOKE");
            }
            Native(command.spawn().unwrap())
        }
        fn alive(&self, child: &mut Native) {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "Tile child exited: {}",
                self.root.display()
            );
            assert!(
                Instant::now() < self.deadline,
                "Tile joint deadline: {}",
                self.root.display()
            );
        }
        fn phase(&self, child: &mut Native, name: &str) -> Value {
            while !self.root.join(name).exists() {
                self.alive(child);
                thread::sleep(Duration::from_millis(10));
            }
            serde_json::from_slice(&fs::read(self.root.join(name)).unwrap()).unwrap()
        }
        fn joined(&self, child: &mut Native) {
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    assert!(
                        status.success(),
                        "Tile child failure: {}",
                        self.root.display()
                    );
                    return;
                }
                assert!(
                    Instant::now() < self.deadline,
                    "Tile child reap deadline: {}",
                    self.root.display()
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
        fn state(&self) -> Value {
            let (status, value) = admin(self.port, "GET", "/v1/walls/state", None);
            assert_eq!(status, 200, "{value}");
            value
        }
        fn until(&self, child: &mut Native, check: impl Fn(&Value) -> bool) -> Value {
            loop {
                self.alive(child);
                let value = self.state();
                if check(&value) {
                    return value;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        fn present(&self, child: &mut Native, mode: &str) -> Value {
            let base = self.state()["revision"].as_u64().unwrap();
            let (status, value) = admin(
                self.port,
                "PUT",
                "/v1/walls/presentation",
                Some(json!({"baseRevision":base,"wallId":"native-image-wall","mode":mode})),
            );
            assert_eq!(status, 200, "{value}");
            let revision = value["revision"].as_u64().unwrap();
            self.until(child, |s| {
                if mode == "running" {
                    s["endpoints"][0]["appliedRevision"] == revision
                        && s["endpoints"][0]["scene"]["prepared"] == true
                } else {
                    s["endpoints"][0]["presentation"]["revision"] == revision
                        && s["endpoints"][0]["presentation"]["outcome"] == "applied"
                }
            })
        }
    }
    #[test]
    #[ignore = "explicit real Loom LAN and independent native Tile manager/output gate"]
    fn real_lan_native_tile_images_freeze_black_resume_and_stop() {
        run(false, false);
    }
    #[test]
    #[ignore = "explicit real Loom LAN device revocation and native Tile cleanup gate"]
    fn real_lan_native_tile_revocation_clears_and_reclaims_output() {
        run(true, false);
    }

    #[test]
    #[ignore = "explicit real Loom LAN Tile resize/missing/return owner recovery gate"]
    fn real_lan_native_tile_output_rebinds_after_resize_and_absence() {
        run(false, true);
    }

    #[test]
    #[ignore = "explicit real Loom LAN recovered Tile revocation remains terminal gate"]
    fn real_lan_native_tile_recovered_output_cannot_revive_after_revocation() {
        run(true, true);
    }

    include!("tile_live_native_joint.rs");
    include!("tile_surface_native_joint.rs");
    include!("tile_identification_native_joint.rs");
    include!("tile_surface_action_native_joint.rs");
    include!("tile_mixed_native_joint.rs");
    include!("tile_product_native_joint.rs");
    fn setup() -> (
        Joint,
        String,
        ConcurrencyTestFixture,
        SharedLiveSessionStore,
    ) {
        setup_configure(false)
    }
    fn setup_configure(
        art: bool,
    ) -> (
        Joint,
        String,
        ConcurrencyTestFixture,
        SharedLiveSessionStore,
    ) {
        let packages = art.then(|| {
            let scene = json!({"protocolVersion":"loom.surface.v1","scene":{"id":"root","type":"view","layout":{"width":640,"height":480},"style":{"background":"#ff0000"}},"authoritativeState":{"value":7}});
            (wall_confirmation_package(&scene), framework_package_zip("process", "1.0.0"))
        });
        setup_packages(packages)
    }
    fn setup_packages(
        packages: Option<(Vec<u8>, Vec<u8>)>,
    ) -> (
        Joint,
        String,
        ConcurrencyTestFixture,
        SharedLiveSessionStore,
    ) {
        // 故障现场和合成profile保留在本次TEMP；不使用Root的自动删除策略。
        let root = unique_temp_dir(&format!("tile-native-joint-{}", Uuid::new_v4()));
        let (port, server, sessions) = start_with_setup(&root, |daemon| {
            if let Some((package, framework)) = packages {
                daemon
                    .runtime
                    .framework_registry
                    .install_framework_package_from_zip(&framework)
                    .unwrap();
                loom_tool_registry::install::install_art_from_zip(
                    &package,
                    &root,
                    &daemon.runtime.framework_registry,
                    &daemon.runtime.tool_registry,
                )
                .unwrap();
            }
        });
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let tls_port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let (status, lan) = admin(
            port,
            "PUT",
            "/v1/otter/connections/lan",
            Some(json!({"enabled":true,"host":"127.0.0.1","port":tls_port})),
        );
        assert_eq!(status, 200, "{lan}");
        assert_eq!(lan["listening"], true);
        let (_, info) = admin(port, "GET", "/v1/otter/identity", None);
        fs::write(
            root.join("tile-ca.pem"),
            format!(
                "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
                info["certificate"].as_str().unwrap()
            ),
        )
        .unwrap();
        let input = root.join("tile-input.json");
        fs::write(
            &input,
            serde_json::to_vec(&json!({"origin":lan["endpoint"]})).unwrap(),
        )
        .unwrap();
        let f = Joint {
            root,
            port,
            input,
            origin: lan["endpoint"].as_str().unwrap().to_owned(),
            deadline: Instant::now() + Duration::from_secs(30),
        };
        let mut manager = f.child("manager_pair_approve_and_renew", false);
        let pairing = f.phase(&mut manager, "tile-pairing.json");
        let device = pairing["deviceId"].as_str().unwrap();
        let (status, approved) = admin(
            port,
            "POST",
            &format!("/v1/devices/{device}/approve"),
            Some(json!({})),
        );
        assert_eq!(status, 200, "{approved}");
        fs::write(f.root.join("tile-approved"), b"approved").unwrap();
        f.joined(&mut manager);
        let complete: Value =
            serde_json::from_slice(&fs::read(f.root.join("tile-manager-complete.json")).unwrap())
                .unwrap();
        assert_eq!(
            complete,
            json!({"deviceId":device,"pairing":true,"approved":true,"signedRenewal":true,"managerClosed":true})
        );
        (f, device.to_owned(), server, sessions)
    }
    fn run(revoked: bool, recovery: bool) {
        let (f, device, mut server, _sessions) = setup();
        let port = f.port;
        let output_mode = if recovery {
            "output_recovery_and_cleanup"
        } else {
            "output_present_and_cleanup"
        };
        let mut output = f.child(output_mode, revoked);
        let registered = f.until(&mut output, |s| {
            s["endpoints"]
                .as_array()
                .is_some_and(|v| v.len() == 1 && v[0]["online"] == true)
        });
        assert_eq!(
            registered["revision"], 1,
            "only the native Runtime registered this endpoint"
        );
        let endpoint = &registered["endpoints"][0]["endpoint"];
        assert_eq!(endpoint["deviceId"], device);
        assert_eq!(
            endpoint["renderModes"],
            json!(["image", "raw_bgra", "surface_v1"])
        );
        assert_eq!(endpoint["inputCapabilities"], json!(["pointer"]));
        assert_eq!(endpoint["scheduledPresentation"], true);
        let (status, image) = admin(
            port,
            "POST",
            "/v1/surfaces/resources",
            Some(
                json!({"kind":"image","mime":"application/x-neuro-rgba8","width":2,"height":2,"dataBase64":BASE64.encode([255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255])}),
            ),
        );
        assert_eq!(status, 201, "{image}");
        let layout = json!({"protocolVersion":"loom.wall.v1","wallId":"native-image-wall","revision":2,"bounds":{"x":0,"y":0,"width":2,"height":2},
            "tiles":[{"tileId":"native-tile","endpointId":endpoint["endpointId"],"rect":{"x":0,"y":0,"width":2,"height":2},"rotation":"deg0"}],
            "placements":[{"placementId":"native-picture","source":{"kind":"image","id":image["resource"]["resourceId"]},"rect":{"x":0,"y":0,"width":2,"height":2},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":false}]});
        let (status, scheduled) = admin(
            port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision":1,"layout":layout,"activationDelayMs":1000})),
        );
        assert_eq!(status, 200, "{scheduled}");
        assert!(scheduled["endpoints"][0]["appliedRevision"].is_null());
        let applied = f.until(&mut output, |s| {
            s["endpoints"][0]["appliedRevision"] == 2
                && s["endpoints"][0]["scene"]["prepared"] == true
        });
        let scene = &applied["endpoints"][0]["scene"];
        assert_eq!(scene["revision"], 2);
        assert!(
            scene["appliedAtMs"].as_u64().unwrap()
                >= scheduled["timing"]["scenes"][0]["activateAtMs"]
                    .as_u64()
                    .unwrap()
        );
        assert!(scene["clockUncertaintyMs"].as_u64().unwrap() <= 1000);
        let pixels = f.phase(&mut output, "tile-native-running.json");
        assert_eq!(pixels["revision"], 2);
        assert_eq!(
            pixels["sourcePbgra"],
            json!([0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255])
        );
        let mut proof = json!({"registered":registered,"running":applied,"native":pixels,"manager":serde_json::from_slice::<Value>(&fs::read(f.root.join("tile-manager-complete.json")).unwrap()).unwrap()});
        if recovery {
            recover(&f, &mut output, &mut proof, &device, revoked);
        } else if revoked {
            let (status, value) = admin(
                port,
                "PUT",
                &format!("/v1/devices/{device}"),
                Some(
                    json!({"name":"Tile isolated","kind":"computer","address":"127.0.0.1","enabled":false}),
                ),
            );
            assert_eq!(status, 200, "{value}");
        } else {
            let frozen = f.present(&mut output, "frozen");
            assert_eq!(frozen["endpoints"][0]["appliedRevision"], 2);
            let native = f.phase(&mut output, "tile-native-frozen.json");
            assert_eq!(native["revision"], 2);
            let black = f.present(&mut output, "black");
            assert!(black["endpoints"][0]["appliedRevision"].is_null());
            let native = f.phase(&mut output, "tile-native-black.json");
            assert_eq!(native["revision"], black["revision"]);
            let resumed = f.present(&mut output, "running");
            let native = f.phase(&mut output, "tile-native-resumed.json");
            assert_eq!(native["revision"], resumed["revision"]);
            proof["frozen"] = frozen;
            proof["black"] = black;
            proof["resumed"] = resumed;
            fs::write(f.root.join("tile-stop"), b"stop").unwrap();
        }
        f.joined(&mut output);
        let complete: Value =
            serde_json::from_slice(&fs::read(f.root.join("tile-output-complete.json")).unwrap())
                .unwrap();
        let expected = if recovery {
            json!({"recovery":true,"revoked":revoked,"pixelsReleased":true,"threadsJoined":true,"sameEndpoint":true,"differentLeases":true,"stopSticky":true,"physicalHotplug":false,"ordinaryOutputExe":false})
        } else {
            json!({"revoked":revoked,"pixelsReleased":true,"threadsJoined":true,"physicalCrossDevice":false,"ordinaryOutputExe":false})
        };
        assert_eq!(complete, expected);
        let final_state = f.state();
        assert_eq!(final_state["endpoints"][0]["online"], false);
        assert!(final_state["endpoints"][0]["appliedRevision"].is_null());
        proof["final"] = final_state;
        proof["cleanup"] = complete;
        fs::write(
            f.root.join("tile-service-oracle.json"),
            serde_json::to_vec_pretty(&proof).unwrap(),
        )
        .unwrap();
        server.finish().unwrap();
        if let Some(evidence) = std::env::var_os("OTTER_TILE_JOINT_EVIDENCE_DIR") {
            let evidence = PathBuf::from(evidence);
            assert!(evidence.is_absolute());
            fs::create_dir(&evidence).unwrap();
            for name in [
                "tile-service-oracle.json".to_owned(),
                "manager_pair_approve_and_renew.stdout.log".to_owned(),
                "manager_pair_approve_and_renew.stderr.log".to_owned(),
                format!("{output_mode}.stdout.log"),
                format!("{output_mode}.stderr.log"),
            ] {
                fs::copy(f.root.join(&name), evidence.join(name)).unwrap();
            }
        }
    }

    fn recover(f: &Joint, output: &mut Native, proof: &mut Value, device: &str, revoked: bool) {
        let identity = proof["registered"]["endpoints"][0]["endpoint"]["endpointId"].clone();
        fs::write(f.root.join("tile-resize"), b"resize").unwrap();
        let resized = f.phase(output, "tile-native-resized.json");
        assert_eq!(resized["revision"], 3);
        let state = f.until(output, |s| {
            s["endpoints"][0]["endpoint"]["pixelSize"] == json!({"width":4,"height":2})
                && s["endpoints"][0]["appliedRevision"] == 3
        });
        assert_eq!(state["endpoints"].as_array().unwrap().len(), 1);
        assert_eq!(state["endpoints"][0]["endpoint"]["endpointId"], identity);
        assert_eq!(state["layouts"][0]["revision"], 3);
        proof["resized"] = state;
        proof["resizedNative"] = resized;
        fs::write(f.root.join("tile-remove"), b"remove").unwrap();
        let missing = f.phase(output, "tile-native-missing.json");
        assert_eq!(
            missing,
            json!({"oldGenerationReclaimed":true,"differentMonitorRejected":true})
        );
        proof["absent"] = f.until(output, |s| s["endpoints"][0]["online"] == false);
        fs::write(f.root.join("tile-return"), b"return").unwrap();
        let returned = f.phase(output, "tile-native-returned.json");
        assert_eq!(returned["revision"], 3);
        let state = f.until(output, |s| s["endpoints"][0]["appliedRevision"] == 3);
        assert_eq!(
            state["revision"], 3,
            "same descriptor cannot re-register unchanged catalog"
        );
        assert_eq!(state["endpoints"][0]["endpoint"]["endpointId"], identity);
        proof["returned"] = state;
        proof["returnedNative"] = returned;
        if revoked {
            let (status, value) = admin(
                f.port,
                "PUT",
                &format!("/v1/devices/{device}"),
                Some(
                    json!({"name":"Tile isolated","kind":"computer","address":"127.0.0.1","enabled":false}),
                ),
            );
            assert_eq!(status, 200, "{value}");
        } else {
            fs::write(f.root.join("tile-stop"), b"stop").unwrap();
        }
    }
}
