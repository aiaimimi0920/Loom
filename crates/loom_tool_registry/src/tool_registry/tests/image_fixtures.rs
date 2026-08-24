//! Image and retry HTTP fixtures.

use super::*;

pub(super) struct HttpImageFixture {
    port: u16,
    worker: Option<JoinHandle<()>>,
}

impl HttpImageFixture {
    pub(super) fn start(content_type: &'static str, body: Vec<u8>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP image fixture");
        let port = listener
            .local_addr()
            .expect("HTTP image fixture address")
            .port();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("accept HTTP image fixture request");
            let _ = read_http_request(&mut stream);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        });
        Self {
            port,
            worker: Some(worker),
        }
    }

    pub(super) fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

impl Drop for HttpImageFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

pub(super) struct HeaderAwareHttpImageFixture {
    port: u16,
    worker: Option<JoinHandle<()>>,
}

impl HeaderAwareHttpImageFixture {
    pub(super) fn start(
        content_type: &'static str,
        body: Vec<u8>,
        required_header: &'static str,
    ) -> Self {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("bind guarded HTTP image fixture");
        let port = listener
            .local_addr()
            .expect("guarded HTTP image fixture address")
            .port();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("accept guarded HTTP image fixture request");
            let request = read_http_request(&mut stream);
            if request.to_ascii_lowercase().contains(required_header) {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            } else {
                write_http_response(
                    &mut stream,
                    "403 Forbidden",
                    "text/plain",
                    "missing required header",
                );
            }
        });
        Self {
            port,
            worker: Some(worker),
        }
    }

    pub(super) fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

impl Drop for HeaderAwareHttpImageFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

pub(super) struct ExactPathHttpImageFixture {
    port: u16,
    worker: Option<JoinHandle<()>>,
}

impl ExactPathHttpImageFixture {
    pub(super) fn start(
        content_type: &'static str,
        body: Vec<u8>,
        expected_path: &'static str,
    ) -> Self {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("bind exact-path HTTP image fixture");
        let port = listener
            .local_addr()
            .expect("exact-path HTTP image fixture address")
            .port();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("accept exact-path HTTP image fixture request");
            let request = read_http_request(&mut stream);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or_default();
            if path == expected_path {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            } else {
                write_http_response(&mut stream, "404 Not Found", "text/plain", "not found");
            }
        });
        Self {
            port,
            worker: Some(worker),
        }
    }

    pub(super) fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

impl Drop for ExactPathHttpImageFixture {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

/// An HTTP image fixture that keeps answering requests, 404ing every path but one.
///
/// The single-connection fixture cannot express a retry, and a retry is the whole point of a test
/// about a download that asks for the wrong URL first. This one serves until it is dropped, so a
/// regression that stops after the first attempt fails the assertion instead of hanging the suite.
pub(super) struct RetryingExactPathHttpImageFixture {
    port: u16,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl RetryingExactPathHttpImageFixture {
    pub(super) fn start(
        content_type: &'static str,
        body: Vec<u8>,
        expected_path: &'static str,
    ) -> Self {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("bind retrying HTTP image fixture");
        let port = listener
            .local_addr()
            .expect("retrying HTTP image fixture address")
            .port();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stop = std::sync::Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                if worker_stop.load(Ordering::Acquire) {
                    break;
                }
                let request = read_http_request(&mut stream);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or_default();
                if path == expected_path {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                } else {
                    write_http_response(&mut stream, "404 Not Found", "text/plain", "not found");
                }
            }
        });
        Self {
            port,
            stop,
            worker: Some(worker),
        }
    }

    pub(super) fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

impl Drop for RetryingExactPathHttpImageFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            let _ = worker.join();
        }
    }
}

pub(super) fn read_http_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let read = stream
            .read(&mut buffer)
            .expect("read fixture request headers");
        if read == 0 {
            return String::from_utf8_lossy(&request).to_string();
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };

    let content_length = String::from_utf8_lossy(&request[..header_end])
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or_default();
    let expected_length = header_end + content_length;

    while request.len() < expected_length {
        let read = stream.read(&mut buffer).expect("read fixture request body");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
    }

    String::from_utf8_lossy(&request).to_string()
}

pub(super) fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}
