use super::*;
use ed25519_dalek::{Signer as _, SigningKey};

struct ProjectionRoot(PathBuf);
impl ProjectionRoot {
    fn new() -> Self {
        Self(unique_temp_dir(&format!(
            "projection-http-{}",
            Uuid::new_v4()
        )))
    }
}
impl Drop for ProjectionRoot {
    fn drop(&mut self) {
        remove_test_dir(&self.0);
    }
}

fn start(root: &Path) -> (u16, ConcurrencyTestFixture) {
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_control_plane_root(root)
            .with_bounded_request_executor(4, 16),
    )
    .expect("bind projection test daemon");
    let port = daemon.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(rx));
    (port, ConcurrencyTestFixture::new(tx, server))
}

fn response(raw: String) -> (u16, Value) {
    (
        raw.split_whitespace().nth(1).unwrap().parse().unwrap(),
        response_json_body(&raw),
    )
}
fn admin(port: u16, path: &str, body: Value) -> (u16, Value) {
    response(http_request(port, "POST", path, Some(&body.to_string())))
}
fn public(port: u16, path: &str, body: Value) -> (u16, Value) {
    response(http_request_without_auth(
        port,
        "POST",
        path,
        Some(&body.to_string()),
    ))
}
struct Identity {
    id: String,
    key: SigningKey,
    token: String,
}
impl Identity {
    fn pair(port: u16, name: &str) -> Self {
        let key = SigningKey::generate(&mut OsRng);
        let (status, pending) = public(
            port,
            "/v1/devices/requests",
            json!({
                "name": name, "kind": "computer", "address": "127.0.0.1",
                "publicKey": BASE64.encode(key.verifying_key().to_bytes()),
            }),
        );
        assert_eq!(status, 200);
        let id = pending["pending"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            admin(port, &format!("/v1/devices/{id}/approve"), json!({})).0,
            200
        );
        let mut identity = Self {
            id,
            key,
            token: String::new(),
        };
        identity.session(port);
        identity
    }

    fn session(&mut self, port: u16) {
        let (status, challenge) = public(
            port,
            "/v1/device-sessions/challenges",
            json!({"deviceId": self.id}),
        );
        assert_eq!(status, 201);
        let nonce = Uuid::new_v4().to_string();
        let message = device_session_signature_message(
            &self.id,
            challenge["challengeId"].as_str().unwrap(),
            challenge["challenge"].as_str().unwrap(),
            &nonce,
        );
        let (status, session) = public(
            port,
            "/v1/device-sessions",
            json!({
                "deviceId": self.id, "challengeId": challenge["challengeId"], "clientNonce": nonce,
                "signature": BASE64.encode(self.key.sign(message.as_bytes()).to_bytes()),
            }),
        );
        assert_eq!(status, 201);
        self.token = session["token"].as_str().unwrap().to_owned();
    }

    fn post(&self, port: u16, operation: &str, body: Value) -> (u16, Value) {
        let headers = format!(
            "Authorization: Device {}\r\nX-Loom-Device-Nonce: {}\r\n",
            self.token,
            Uuid::new_v4()
        );
        response(http_request_with_extra_headers(
            port,
            "POST",
            &format!("/v1/projections/{operation}"),
            Some(&body.to_string()),
            &headers,
        ))
    }

    fn invitation(&self, snapshot: &ProjectionSnapshot, expires_at_ms: u64) -> ProjectionEnvelope {
        let mut envelope: ProjectionEnvelope = serde_json::from_value(json!({
            "protocol": loom_protocol::projection::QR_PROJECTION_PROTOCOL,
            "projectionId": format!("projection:{}", Uuid::new_v4().simple()),
            "serverOrigin": "https://loom.example.test",
            "source": {"deviceId": self.id, "sessionId": "session-a", "unitId": "source", "revision": 1},
            "content": {"kind": "art", "digest": sha256_bytes(&BASE64.decode(&snapshot.image_base64).unwrap())},
            "expiresAtMs": expires_at_ms, "nonce": Uuid::new_v4().simple().to_string(),
            "signature": {"algorithm": "ed25519", "keyId": self.id, "value": "A".repeat(86)},
        })).unwrap();
        envelope.signature.value = BASE64_URL.encode(
            self.key
                .sign(envelope.signature_message().as_bytes())
                .to_bytes(),
        );
        envelope
    }
}

fn png(red: u8) -> ProjectionSnapshot {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([red, 20, 40, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    ProjectionSnapshot {
        image_base64: BASE64.encode(bytes.into_inner()),
        width: 2,
        height: 2,
    }
}

fn acceptance(envelope: &ProjectionEnvelope, unit_id: &str) -> Value {
    json!({"envelope": envelope, "expectedRevision": 1, "expectedDigest": envelope.content.digest,
        "receiverUnitId": unit_id, "confirmed": true})
}
fn update(envelope: &ProjectionEnvelope, prior: u64, snapshot: &ProjectionSnapshot) -> Value {
    json!({"projectionId": envelope.projection_id, "sourceSessionId": envelope.source.session_id,
        "priorRevision": prior, "revision": prior + 1,
        "digest": sha256_bytes(&BASE64.decode(&snapshot.image_base64).unwrap()), "snapshot": snapshot})
}
fn error_code(response: &(u16, Value)) -> &str {
    response.1["error"]["code"].as_str().unwrap()
}
