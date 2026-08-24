//! Cloud HTTP fixtures.

use super::*;

pub(super) const CLOUD_FIXTURE_IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";
pub(super) const CLOUD_FIXTURE_IMAGE_ALT: &str =
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAEElEQVR4AQEFAPr/AAoUHv8BpAE8tOS4KAAAAABJRU5ErkJggg==";

pub(super) fn fixture_image_bytes() -> Vec<u8> {
    loom_image_io::decode_data_url_bytes(CLOUD_FIXTURE_IMAGE)
        .expect("decode fixture image data url")
}

pub(super) fn fixture_alt_image_bytes() -> Vec<u8> {
    loom_image_io::decode_data_url_bytes(CLOUD_FIXTURE_IMAGE_ALT)
        .expect("decode alternate fixture image data url")
}

#[derive(Clone, Copy)]
pub(super) enum CloudFixtureMode {
    Text,
    Image,
    Error,
    MultipartText,
    DelayedHeaders,
    DelayedBody,
}

pub(super) struct CloudFixture {
    port: u16,
    worker: Option<JoinHandle<()>>,
    pub(super) captured_request: Arc<Mutex<Option<String>>>,
}

impl CloudFixture {
    pub(super) fn start(mode: CloudFixtureMode) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind cloud fixture");
        let port = listener.local_addr().expect("cloud fixture address").port();
        let captured_request = Arc::new(Mutex::new(None));
        let worker_captured_request = Arc::clone(&captured_request);
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept cloud fixture request");
            let request = read_http_request(&mut stream);
            *worker_captured_request
                .lock()
                .expect("lock cloud request capture") = Some(request.clone());
            let Some((_, body)) = request.split_once("\r\n\r\n") else {
                return;
            };
            let prompt = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|value| {
                    value
                        .get("prompt")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            if matches!(
                mode,
                CloudFixtureMode::DelayedHeaders | CloudFixtureMode::DelayedBody
            ) {
                let response = serde_json::json!({
                    "content": [{ "type": "text", "text": "too late" }]
                })
                .to_string();
                match mode {
                    CloudFixtureMode::DelayedHeaders => {
                        thread::sleep(Duration::from_millis(500));
                        write_http_response(&mut stream, "200 OK", "application/json", &response);
                    }
                    CloudFixtureMode::DelayedBody => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            response.len()
                        );
                        let split = response.len() / 2;
                        let _ = stream.write_all(&response.as_bytes()[..split]);
                        let _ = stream.flush();
                        thread::sleep(Duration::from_millis(500));
                        let _ = stream.write_all(&response.as_bytes()[split..]);
                        let _ = stream.flush();
                    }
                    _ => unreachable!(),
                }
                return;
            }
            let response = match mode {
                CloudFixtureMode::Text => serde_json::json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("cloud saw {prompt}")
                        }
                    ]
                }),
                CloudFixtureMode::Image => serde_json::json!({
                    "content": [
                        {
                            "type": "image",
                            "data": CLOUD_FIXTURE_IMAGE,
                            "mimeType": "image/png"
                        }
                    ]
                }),
                CloudFixtureMode::MultipartText => serde_json::json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "cloud saw multipart"
                        }
                    ]
                }),
                CloudFixtureMode::Error => {
                    write_http_response(
                        &mut stream,
                        "500 Internal Server Error",
                        "text/plain",
                        "fixture cloud error",
                    );
                    return;
                }
                CloudFixtureMode::DelayedHeaders | CloudFixtureMode::DelayedBody => {
                    unreachable!()
                }
            };
            write_http_response(
                &mut stream,
                "200 OK",
                "application/json",
                &response.to_string(),
            );
        });
        Self {
            port,
            worker: Some(worker),
            captured_request,
        }
    }

    pub(super) fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    pub(super) fn request(&self) -> String {
        self.captured_request
            .lock()
            .expect("lock cloud request capture")
            .clone()
            .expect("captured cloud request")
    }
}

impl Drop for CloudFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}
