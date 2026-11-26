// Real HTTPS issuance -> Otter process exits -> Loom stops -> second Otter process executes offline without live device or configuration.
mod resume_joint {
    use super::*;
    use ed25519_dalek::SigningKey;
    const SOURCE:&[u8]=b"NeuroSurface.define({async mount({root}){let offline=false;try{await NeuroSurface.service('capability:publisher.example:local-service.run',{},5000)}catch(e){if(e.message!=='offline')throw e;offline=true}if(!offline)throw Error('expected offline failure');let s=document.createElement('span');s.textContent='offline-answer';root.appendChild(s);NeuroSurface.commitResult({answer:{kind:'value',value:42}})}});";
    fn child(executable: &Path, root: &Path, input: &Path, stage: &str) {
        let stdout = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join(format!("resume-{stage}.stdout.log")))
            .unwrap();
        let stderr = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join(format!("resume-{stage}.stderr.log")))
            .unwrap();
        let mut child=super::joint::Otter(Command::new(executable).args(["device_session_client::tests::live_control::art_prepare::joint::resume_joint::art_resume_joint_real_loom","--exact","--ignored","--test-threads=1","--nocapture"])
            .env("OTTER_LOCAL_ART_JOINT_INPUT",input).env("OTTER_ART_RESUME_STAGE",stage).stdout(stdout).stderr(stderr).spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "{stage} failed; evidence {}",
                    root.display()
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{stage} timed out; evidence {}",
                root.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    #[ignore = "explicit cross-repository HTTPS and separate-process offline acceptance"]
    fn local_art_joint_resume_across_otter_processes_with_loom_stopped() {
        let executable = PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").unwrap());
        assert!(executable.is_absolute() && executable.is_file());
        let mut f = Fixture::build_scoped("local.js", &sha256_bytes(SOURCE), true, SOURCE, true);
        // Manifest support for formal results is necessary but not sufficient: both the prepare request and device approval must explicitly request the scope.
        f.request["permissions"]
            .as_array_mut()
            .unwrap()
            .push(json!("workspace_result"));
        let key = SigningKey::generate(&mut OsRng);
        let (device, token) =
            super::super::wall_http::pair_with_key(f.port, "Otter resume joint", &key);
        f.device = device.clone();
        f.token = token;
        f.request["binding"]["deviceId"] = device.into();
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let tls_port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let (status, lan) = admin(
            f.port,
            "PUT",
            "/v1/otter/connections/lan",
            Some(json!({"enabled":true,"host":"127.0.0.1","port":tls_port})),
        );
        assert_eq!(status, 200);
        assert_eq!(lan["listening"], true);
        f.request["binding"]["origin"] = lan["endpoint"].clone();
        let (_, identity_info) = admin(f.port, "GET", "/v1/otter/identity", None);
        assert_eq!(f.approve(0, true).0, 200);
        let descriptor = json!({"request":f.request,"issuer":f.issuer,"certificate":identity_info["certificate"],"identity":{"schemaVersion":1,"deviceId":f.device,"privateKey":BASE64.encode(key.to_bytes()),"publicKey":BASE64.encode(key.verifying_key().to_bytes())}});
        let input = f.root.join("resume-joint-input.json");
        fs::write(&input, serde_json::to_vec(&descriptor).unwrap()).unwrap();
        child(&executable, &f.root, &input, "online");
        assert!(f.root.join("otter-resume-online.json").is_file());
        f.server.finish().unwrap();
        child(&executable, &f.root, &input, "offline");
        let result: Value =
            serde_json::from_slice(&fs::read(f.root.join("otter-resume-offline.json")).unwrap())
                .unwrap();
        assert_eq!(result["freshHostBinding"], true);
        assert_eq!(result["formalResult"], true);
        assert_eq!(result["deviceSession"], false);
        assert_eq!(result["offlineService"], true);
        // Copy only safe receipts and child logs; never copy inputs or profiles containing synthetic device private keys.
        if let Some(output) = std::env::var_os("OTTER_ART_RESUME_EVIDENCE_DIR") {
            let output = PathBuf::from(output);
            assert!(output.is_absolute());
            fs::create_dir(&output).unwrap();
            for name in [
                "otter-resume-online.json",
                "otter-resume-offline.json",
                "resume-online.stdout.log",
                "resume-online.stderr.log",
                "resume-offline.stdout.log",
                "resume-offline.stderr.log",
            ] {
                fs::copy(f.root.join(name), output.join(name)).unwrap();
            }
        }
        println!("offline acceptance: {result}");
    }
}
