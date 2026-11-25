// 双仓真实联合门；只启动显式指定的 Otter 测试程序，不改生产入口或持久用户配置。
mod joint {
    use super::*;
    use ed25519_dalek::SigningKey;
    const SOURCE: &[u8] = br#"
let label;
NeuroSurface.define({async mount({root}) {
 label=document.createElement('span');root.appendChild(label);
 const result=await NeuroSurface.service('capability:publisher.example:local-service.run',{},5000);
 label.textContent='calls:'+result.calls;
 const button=document.createElement('button');button.textContent='run';root.appendChild(button);
 button.addEventListener('click',async()=>{
  const result=await NeuroSurface.service('capability:publisher.example:local-service.run',{null:true},5000);
  label.textContent=result===null?'null-complete':'wrong';
 });
}});
    "#;
    const IMAGE_SOURCE: &[u8] = br#"
NeuroSurface.define({async mount({root}) {
 const label=document.createElement('span');root.appendChild(label);
 const result=await NeuroSurface.serviceImages('capability:publisher.example:local-service.run',{readImage:true},[NeuroSurface.image('input:1')],5000);
 label.textContent='image:'+result.pngHex;
 const button=document.createElement('button');button.textContent='run';root.appendChild(button);
 button.addEventListener('click',async()=>{
  const result=await NeuroSurface.service('capability:publisher.example:local-service.run',{null:true},5000);
  label.textContent=result===null?'null-complete':'wrong';
 });
}});
    "#;
    const CANVAS_SOURCE: &[u8] = br#"
NeuroSurface.define({async mount({root}) {
 const c=document.createElement('canvas');c.width=2;c.height=1;const g=c.getContext('2d');
 g.fillStyle='red';g.fillRect(0,0,1,1);g.fillStyle='rgba(0,0,255,0.5)';g.fillRect(1,0,1,1);
 const label=document.createElement('span');root.appendChild(label);
 const result=await NeuroSurface.serviceCanvases('capability:publisher.example:local-service.run',{readImage:true},[c],5000);
 label.textContent='image:'+result.pngHex;
 const button=document.createElement('button');button.textContent='run';root.appendChild(button);
 button.addEventListener('click',async()=>{const result=await NeuroSurface.service('capability:publisher.example:local-service.run',{null:true},5000);label.textContent=result===null?'null-complete':'wrong';});
}});
"#;
    const IMAGE_OUTPUT_SOURCE: &[u8] = br#"
NeuroSurface.define({async mount({root}) {
 const input=document.createElement('canvas');input.width=1;input.height=1;input.getContext('2d').fillRect(0,0,1,1);
 const result=await NeuroSurface.serviceImage('capability:publisher.example:local-service.run',{returnImage:true},{canvases:[input],timeoutMs:5000});
 const label=document.createElement('span');label.textContent='calls:'+result.output.calls;root.appendChild(label);
 const button=document.createElement('button');button.textContent='run';root.appendChild(button);
 button.addEventListener('click',async()=>{const value=await NeuroSurface.service('capability:publisher.example:local-service.run',{null:true},5000);label.textContent=value===null?'null-complete':'wrong';});
 const c=document.createElement('canvas');c.width=2;c.height=2;root.appendChild(c);c.getContext('2d').drawImage(result.image,0,0);
}});
"#;
    pub(super) struct Otter(pub(super) std::process::Child);
    impl Drop for Otter {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    #[test]
    #[ignore = "explicit cross-repository real daemon and Otter child integration"]
    fn local_art_joint_otter_signed_service_and_approval_revocation() {
        run_joint(0);
    }
    #[test]
    #[ignore = "explicit cross-repository real HTTPS image service integration"]
    fn local_art_joint_otter_images_signed_service_and_approval_revocation() {
        run_joint(1);
    }
    #[test]
    #[ignore = "explicit cross-repository real HTTPS Canvas image service integration"]
    fn local_art_joint_otter_canvas_signed_service_and_approval_revocation() {
        run_joint(2);
    }
    #[test]
    #[ignore = "explicit real HTTPS signed service image result and native drawImage"]
    fn local_art_joint_otter_image_output_and_approval_revocation() { run_joint(3); }
    #[test]
    #[ignore = "explicit real HTTPS sensitive service with trusted test host, not user UI"]
    fn local_art_joint_otter_confirmation_and_approval_revocation() { run_joint(4); }
    fn run_joint(mode: u8) {
        let images = matches!(mode, 1..=3);
        let executable = PathBuf::from(
            std::env::var_os("OTTER_NATIVE_TEST_EXE").expect("explicit Otter test EXE"),
        );
        assert!(executable.is_absolute() && executable.is_file());
        let confirmed_source = std::str::from_utf8(SOURCE).unwrap()
            .replace("NeuroSurface.service(", "NeuroSurface.serviceWithConfirmation(")
            .replace(",5000)", ",{timeoutMs:5000})");
        let source = match mode { 0 => SOURCE, 1 => IMAGE_SOURCE, 2 => CANVAS_SOURCE, 3 => IMAGE_OUTPUT_SOURCE, _ => confirmed_source.as_bytes() };
        let mut f = if mode == 4 {
            Fixture::build_package_service(&package("local.js", &sha256_bytes(source), &[SERVICE_ID], source), true, false, true)
        } else if images {
            Fixture::build_package_images(
                &package("local.js", &sha256_bytes(source), &[SERVICE_ID], source),
                true,
                true,
            )
        } else {
            Fixture::build_source("local.js", &sha256_bytes(source), true, source)
        };
        let key = SigningKey::generate(&mut OsRng);
        let (device, token) =
            super::super::wall_http::pair_with_key(f.port, "Otter joint service", &key);
        f.device = device.clone();
        f.token = token;
        f.request["binding"]["deviceId"] = json!(device);
        // 使用真正的生产 HTTPS 入口；不为联合测试放宽 Otter 的 HTTPS origin 合同。
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let tls_port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let (status, lan) = admin(
            f.port,
            "PUT",
            "/v1/otter/connections/lan",
            Some(json!({"enabled":true,"host":"127.0.0.1","port":tls_port})),
        );
        assert_eq!(status, 200, "{lan}");
        assert_eq!(lan["listening"], true);
        f.request["binding"]["origin"] = lan["endpoint"].clone();
        let (status, identity_info) = admin(f.port, "GET", "/v1/otter/identity", None);
        assert_eq!(status, 200);
        assert_eq!(identity_info["endpoint"], lan["endpoint"]);
        assert_eq!(f.approve(0, true).0, 200);
        let descriptor = json!({"imageService":mode==1,"canvasService":mode==2,"imageOutputService":mode==3,"confirmationService":mode==4,"request":f.request,"issuer":f.issuer,"certificate":identity_info["certificate"],"identity":{
            "schemaVersion":1,"deviceId":f.device,"privateKey":BASE64.encode(key.to_bytes()),
            "publicKey":BASE64.encode(key.verifying_key().to_bytes())}});
        let input = f.root.join("otter-joint-input.json");
        // 一次性合成身份只存在隔离测试目录；不通过命令行或日志交付凭据。
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&input)
            .unwrap();
        file.write_all(&serde_json::to_vec(&descriptor).unwrap())
            .unwrap();
        file.sync_all().unwrap();
        drop(file);
        let stdout = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(f.root.join("otter-joint.stdout.log"))
            .unwrap();
        let stderr = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(f.root.join("otter-joint.stderr.log"))
            .unwrap();
        let mut child = Otter(Command::new(executable)
            .args(["device_session_client::tests::live_control::art_prepare::joint::art_service_joint_real_loom", "--exact", "--ignored", "--test-threads=1", "--nocapture"])
            .env("OTTER_LOCAL_ART_JOINT_INPUT", &input)
            .stdout(stdout).stderr(stderr).spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(30);
        let ready = f.root.join("otter-joint-ready.json");
        while !ready.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "Otter exited before ready; evidence: {}",
                f.root.display()
            );
            assert!(
                Instant::now() < deadline,
                "Otter did not reach service result; evidence: {}",
                f.root.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fs::read(&ready).unwrap(), b"{\"calls\":1,\"null\":true}");
        let cache: Value =
            serde_json::from_slice(&fs::read(f.root.join("otter-joint-cache.json")).unwrap())
                .unwrap();
        assert_eq!(cache["sourceDigest"], sha256_bytes(source));
        assert_eq!(cache["originalRequest"], f.request["binding"]["requestId"]);
        assert_eq!(
            cache["currentRequest"],
            format!(
                "{}:cached",
                f.request["binding"]["requestId"].as_str().unwrap()
            )
        );
        assert_eq!(cache["cachedPrepare"], true);
        assert_eq!(f.approve(1, false).0, 200);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(f.root.join("otter-joint-revoked"))
            .unwrap();
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "Otter failed; evidence: {}",
                    f.root.display()
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Otter did not reap; evidence: {}",
                f.root.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            fs::read(f.root.join("otter-joint-complete")).unwrap(),
            b"revocation-rejected"
        );
        if images {
            super::wait_image_cleanup(&f);
        }
        eprintln!("local-art-joint: actual-device-session=true actual-signed-plugin=true calls=1 null=true approval-revoked=true otter-exited=true");
    }
}
