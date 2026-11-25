// Isolated real-daemon TLS and child fixtures; trust exchange uses public certificates only.
use super::*;
use std::io::Read;

pub(super) struct Https {
    pub origin: String,
    pub certificate: Value,
    client: reqwest::blocking::Client,
}
impl Https {
    pub(super) fn enable(port: u16) -> Self {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let tls_port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let reply = peer_admin(
            port,
            "PUT",
            "/v1/otter/connections/lan",
            json!({"enabled":true,"host":"127.0.0.1","port":tls_port}),
        );
        assert_eq!(reply.0, 200, "{reply:?}");
        assert_eq!(reply.1["listening"], true);
        let info = response(http_request_without_auth(
            port,
            "GET",
            "/v1/otter/identity",
            None,
        ))
        .1;
        let certificate = BASE64
            .decode(info["certificate"].as_str().unwrap())
            .unwrap();
        Self {
            origin: info["endpoint"].as_str().unwrap().into(),
            certificate: info["certificate"].clone(),
            client: reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(8))
                .add_root_certificate(reqwest::Certificate::from_der(&certificate).unwrap())
                .build()
                .unwrap(),
        }
    }
    pub(super) fn post(
        &self,
        identity: Option<&Identity>,
        path: &str,
        body: Value,
    ) -> (u16, Value) {
        let mut call = self
            .client
            .post(format!("{}{path}", self.origin))
            .json(&body);
        if let Some(identity) = identity {
            call = call
                .header("Authorization", format!("Device {}", identity.token))
                .header("X-Loom-Device-Nonce", Uuid::new_v4().to_string());
        }
        let reply = call.send().unwrap();
        (reply.status().as_u16(), reply.json().unwrap())
    }
    pub(super) fn offline(
        &self,
        identity: &Identity,
        operation: &str,
        body: Value,
    ) -> (u16, Value) {
        self.post(
            Some(identity),
            &format!("/v1/offline-projections/{operation}"),
            body,
        )
    }
    pub(super) fn boundary(&self, identity: &Identity) {
        assert_eq!(
            self.post(
                None,
                "/v1/offline-projections/inbox",
                json!({"policy":"confirm"})
            )
            .0,
            403
        );
        for path in [
            "/v1/projection-peers",
            "/v1/projection-peers/probe",
            "/v1/offline-projections/admin",
        ] {
            let rejected = self.post(Some(identity), path, json!({}));
            assert_eq!(rejected.0, 400);
            assert_eq!(error_code(&rejected), "lan_request_rejected");
        }
        let peer = self.post(None, "/v1/projection-peer/handshake", json!({}));
        assert!(peer.0 >= 400);
        assert_ne!(error_code(&peer), "lan_request_rejected");
        let rejected = self
            .client
            .post(format!("{}/v1/offline-projections/inbox", self.origin))
            .bearer_auth("not-an-admin-grant")
            .json(&json!({}))
            .send()
            .unwrap();
        assert_eq!(rejected.status().as_u16(), 400);
    }
    pub(super) fn input(
        &self,
        root: &Path,
        identity: &Identity,
        target: &str,
        peer: &str,
        source: &str,
        revoke: bool,
    ) {
        fs::create_dir(root).unwrap();
        // Ephemeral synthetic private keys never enter command arguments or public receipts.
        write_once(&root.join("input.json"),&serde_json::to_vec(&json!({"origin":self.origin,
            "certificate":self.certificate,"targetDeviceId":target,"peerId":peer,"sourceOrigin":source,"revokePeer":revoke,
            "identity":{"schemaVersion":1,"deviceId":identity.id,"privateKey":BASE64.encode(identity.key.to_bytes()),
            "publicKey":BASE64.encode(identity.key.verifying_key().to_bytes())}})).unwrap());
    }
}
pub(super) fn spawn(exe: &Path, root: &Path, receiver: bool) -> Otter {
    let name = if receiver {
        "receiver::native_receiver_joint_two_loom_menu_display_update_reopen_and_terminal"
    } else {
        "native_sender_joint_two_loom_offline_updates_and_stops"
    };
    let full = format!("desktop::application::projection_sender::tests::joint::{name}");
    let file = |name| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(name))
            .unwrap()
    };
    Otter(
        Command::new(exe)
            .args([
                &full,
                "--exact",
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("OTTER_PROJECTION_JOINT_INPUT", root.join("input.json"))
            .stdout(file("otter.stdout.log"))
            .stderr(file("otter.stderr.log"))
            .spawn()
            .unwrap(),
    )
}
pub(super) fn finish(child: &mut Otter) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        assert!(Instant::now() < end, "native child did not exit");
        thread::sleep(Duration::from_millis(10));
    }
}
pub(super) fn exe() -> PathBuf {
    use sha2::{Digest, Sha256};
    let path =
        PathBuf::from(std::env::var_os("OTTER_NATIVE_TEST_EXE").expect("explicit native test EXE"));
    assert!(path.is_absolute() && path.is_file());
    let expected = std::env::var("OTTER_NATIVE_TEST_SHA256").expect("explicit native test SHA256");
    let mut file = fs::File::open(&path).unwrap();
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let size = file.read(&mut buffer).unwrap();
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    assert_eq!(
        format!("{:x}", hash.finalize()),
        expected.to_ascii_lowercase()
    );
    path
}
