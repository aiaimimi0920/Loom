//! 仅转发一个已授权对象；aria2 不持有远端凭据，也不能选择远端地址。
use super::{error, Result};
#[cfg(test)]
#[path = "aria2_relay_tests.rs"]
mod tests;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub(super) struct Relay {
    pub url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Relay {
    pub fn start(
        client: reqwest::blocking::Client,
        remote: String,
        token: String,
        ticket: String,
        size: u64,
    ) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|_| error(503, "asset_library_engine_start_failed"))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| error(503, "asset_library_engine_start_failed"))?;
        let path = format!("/{}", uuid::Uuid::new_v4().simple());
        let url = format!(
            "http://{}{}",
            listener
                .local_addr()
                .map_err(|_| error(503, "asset_library_engine_start_failed"))?,
            path
        );
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = thread::spawn(move || {
            let mut workers: Vec<JoinHandle<()>> = Vec::new();
            while !stopped.load(Ordering::Acquire) {
                let mut index = 0;
                while index < workers.len() {
                    if workers[index].is_finished() {
                        let _ = workers.swap_remove(index).join();
                    } else {
                        index += 1;
                    }
                }
                match listener.accept() {
                    Ok((stream, _)) if workers.len() < 4 => {
                        let (client, remote, token, ticket, path) = (
                            client.clone(),
                            remote.clone(),
                            token.clone(),
                            ticket.clone(),
                            path.clone(),
                        );
                        workers.push(thread::spawn(move || {
                            let _ = forward(stream, client, &remote, &token, &ticket, &path, size);
                        }));
                    }
                    Ok((mut stream, _)) => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 503 Busy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                let _ = worker.join();
            }
        });
        Ok(Self {
            url,
            stop,
            thread: Some(thread),
        })
    }
}

fn forward(
    mut stream: TcpStream,
    client: reqwest::blocking::Client,
    remote: &str,
    token: &str,
    ticket: &str,
    path: &str,
    size: u64,
) -> std::io::Result<()> {
    // Windows 的 accept 会继承监听器非阻塞模式；转发线程需要有超时的阻塞 I/O。
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = Vec::new();
    let deadline = std::time::Instant::now();
    while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 8192 {
        if deadline.elapsed() > Duration::from_secs(2) {
            return Ok(());
        }
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
    }
    let request = std::str::from_utf8(&bytes).unwrap_or("");
    let mut lines = request.split("\r\n");
    let first = lines.next().unwrap_or("");
    let head = first == format!("HEAD {path} HTTP/1.1");
    if !bytes.ends_with(b"\r\n\r\n") || (!head && first != format!("GET {path} HTTP/1.1")) {
        return stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    }
    let mut request = client
        .request(
            if head {
                reqwest::Method::HEAD
            } else {
                reqwest::Method::GET
            },
            remote,
        )
        .bearer_auth(token)
        .header("X-Download-Ticket", ticket)
        .timeout(Duration::from_secs(30));
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("range") || name.eq_ignore_ascii_case("if-range") {
                request = request.header(name, value.trim());
            }
        }
    }
    let response = match request.send() {
        Ok(response) => response,
        Err(_) => {
            return stream.write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
        }
    };
    if !matches!(response.status().as_u16(), 200 | 206) {
        return stream.write_all(
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    }
    if response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        != Some("application/zip")
    {
        return stream.write_all(
            b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    }
    write!(
        stream,
        "HTTP/1.1 {} OK\r\nConnection: close\r\n",
        response.status().as_u16()
    )?;
    for name in [
        "content-length",
        "content-type",
        "content-range",
        "etag",
        "accept-ranges",
    ] {
        if let Some(value) = response.headers().get(name).and_then(|v| v.to_str().ok()) {
            write!(stream, "{name}: {value}\r\n")?;
        }
    }
    stream.write_all(b"\r\n")?;
    if !head {
        std::io::copy(&mut response.take(size + 1), &mut stream)?;
    }
    Ok(())
}
