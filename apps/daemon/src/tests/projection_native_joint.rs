// Explicit dual-repository joint gate; real HTTPS/device identity and independent received PNG oracle without altering production protocol or user config.
mod native_joint {
    use super::*;
    pub(super) struct Otter(pub(super) std::process::Child);
    impl Drop for Otter {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    pub(super) fn write_once(path: &Path, bytes: &[u8]) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }
    pub(super) fn wait(root: &Path, name: &str, child: &mut Otter) -> Value {
        let end = Instant::now() + Duration::from_secs(15);
        let path = root.join(name);
        while !path.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "Otter exited before {name}; evidence {}",
                root.display()
            );
            assert!(
                Instant::now() < end,
                "Otter marker deadline {name}; evidence {}",
                root.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }
    #[test]
    #[ignore = "explicit isolated real HTTPS daemon plus native Otter sender child"]
    fn projection_native_sender_updates_reads_and_stops_via_real_https() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = PathBuf::from(
            std::env::var_os("OTTER_PROJECTION_JOINT_DIR").expect("explicit unused test directory"),
        );
        let exe = PathBuf::from(
            std::env::var_os("OTTER_NATIVE_TEST_EXE").expect("explicit Otter lib test EXE"),
        );
        assert!(root.is_absolute() && exe.is_absolute() && exe.is_file());
        assert!(root
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::create_dir(&root).unwrap();
        let data = root.join("loom");
        fs::create_dir(&data).unwrap();
        let (port, mut server) = start(&data);
        let source = Identity::pair(port, "Native projection sender");
        let target = Identity::pair(port, "Independent projection receiver");
        assert_eq!(
            target.post(port, "inbox", json!({"policy":"confirm"})).0,
            200
        );
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let tls_port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let enabled = response(http_request(
            port,
            "PUT",
            "/v1/otter/connections/lan",
            Some(&json!({"enabled":true,"host":"127.0.0.1","port":tls_port}).to_string()),
        ));
        assert_eq!(enabled.0, 200, "{:?}", enabled);
        assert_eq!(enabled.1["listening"], true);
        let info = response(http_request_without_auth(
            port,
            "GET",
            "/v1/otter/identity",
            None,
        ))
        .1;
        let origin = info["endpoint"].as_str().unwrap();
        let certificate = BASE64
            .decode(info["certificate"].as_str().unwrap())
            .unwrap();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(8))
            .add_root_certificate(reqwest::Certificate::from_der(&certificate).unwrap())
            .build()
            .unwrap();
        let post = |identity: &Identity, operation: &str, body: Value| {
            let result = client
                .post(format!("{origin}/v1/projections/{operation}"))
                .header("Authorization", format!("Device {}", identity.token))
                .header("X-Loom-Device-Nonce", Uuid::new_v4().to_string())
                .json(&body)
                .send()
                .unwrap();
            (result.status().as_u16(), result.json::<Value>().unwrap())
        };
        let input = root.join("input.json");
        // Ephemeral synthetic private key is written only to the isolated input file and never exposed in CLI args, logs, or public result manifests.
        write_once(&input,&serde_json::to_vec(&json!({"origin":origin,"certificate":info["certificate"],
            "targetDeviceId":target.id,"identity":{"schemaVersion":1,"deviceId":source.id,
            "privateKey":BASE64.encode(source.key.to_bytes()),"publicKey":BASE64.encode(source.key.verifying_key().to_bytes())}})).unwrap());
        let stdout = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("otter.stdout.log"))
            .unwrap();
        let stderr = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("otter.stderr.log"))
            .unwrap();
        let mut child = Otter(Command::new(exe)
            .args(["desktop::application::projection_sender::tests::joint::native_sender_joint_real_loom_updates_reads_stops_and_restores","--exact","--ignored","--test-threads=1","--nocapture"])
            .env("OTTER_PROJECTION_JOINT_INPUT",&input).stdout(stdout).stderr(stderr).spawn().unwrap());
        let created = wait(&root, "created.json", &mut child);
        let envelope: ProjectionEnvelope =
            serde_json::from_value(created["envelope"].clone()).unwrap();
        let inspected = post(&target, "inspect", json!({"envelope":envelope}));
        assert_eq!(inspected.0, 200, "{:?}", inspected);
        let first = BASE64
            .decode(inspected.1["snapshot"]["imageBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(sha256_bytes(&first), created["digest"]);
        let image = image::load_from_memory(&first).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (32, 32));
        assert!(image.pixels().all(|pixel| pixel.0 == [0, 0, 255, 255]));
        let mut missing = update(&envelope, 1, &png(20));
        missing.as_object_mut().unwrap().remove("digest");
        let rejected = post(&source, "update", missing);
        assert_eq!(rejected.0, 400);
        assert_eq!(error_code(&rejected), "projection_invalid_request");
        assert_eq!(
            post(&target, "accept", acceptance(&envelope, "native-receiver")).0,
            200
        );
        write_once(&root.join("accepted"), b"accepted");
        let updated = wait(&root, "updated.json", &mut child);
        assert_eq!(updated["envelope"], created["envelope"]);
        assert_eq!(updated["revision"], 2);
        let read = post(
            &target,
            "read",
            json!({"projectionId":envelope.projection_id,"knownRevision":1}),
        );
        assert_eq!(read.0, 200, "{:?}", read);
        assert_eq!(read.1["revision"], 2);
        assert_eq!(read.1["digest"], updated["digest"]);
        assert_eq!(
            (
                read.1["snapshot"]["width"].as_u64().unwrap(),
                read.1["snapshot"]["height"].as_u64().unwrap()
            ),
            (16, 8)
        );
        let bytes = BASE64
            .decode(read.1["snapshot"]["imageBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(sha256_bytes(&bytes), updated["digest"]);
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (16, 8));
        assert!(image.pixels().all(|pixel| pixel.0 == [0, 255, 0, 255]));
        write_once(&root.join("receiver-updated.png"), &bytes);
        write_once(&root.join("receiver-updated.json"),&serde_json::to_vec(&json!({"width":16,"height":8,
            "revision":2,"digest":sha256_bytes(&bytes),"independentPixelsMatch":true,"missingDigestRejected":true,
            "actualHttps":true,"actualDeviceSession":true,"receiverUiPresented":false})).unwrap());
        let unchanged = post(
            &target,
            "read",
            json!({"projectionId":envelope.projection_id,"knownRevision":2}),
        );
        assert_eq!(unchanged.0, 200);
        assert!(unchanged.1["snapshot"].is_null());
        write_once(&root.join("read-verified"), b"verified");
        let stopped = wait(&root, "stopped.json", &mut child);
        assert_eq!(stopped["restoredReceipt"], true);
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < end, "Otter did not exit");
            thread::sleep(Duration::from_millis(10));
        }
        let gone = post(
            &source,
            "read",
            json!({"projectionId":envelope.projection_id,"knownRevision":2}),
        );
        assert_eq!(gone.0, 410);
        assert_eq!(error_code(&gone), "projection_unlinked");
        server.finish().unwrap();
        eprintln!("projection-native-joint: actual-https=true menu=true update=2 receiver-pixels=true stopped=true restored=true visible-ui=false");
    }
}
