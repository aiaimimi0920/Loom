use super::*;

#[test]
fn relay_streams_large_object_when_receiver_temporarily_applies_backpressure() {
    const SIZE: usize = 80_000_000;
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    let remote = format!("http://{}/selected", upstream.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            assert!(request.len() < 8192);
        }
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {SIZE}\r\nConnection: close\r\n\r\n").unwrap();
        let block = [37u8; 65536];
        let mut remaining = SIZE;
        while remaining > 0 {
            let count = remaining.min(block.len());
            stream.write_all(&block[..count]).unwrap();
            remaining -= count;
        }
    });
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let relay = Relay::start(
        client.clone(),
        remote,
        "host-token".into(),
        "host-ticket".into(),
        SIZE as u64,
    )
    .unwrap();
    let mut response = client.get(&relay.url).send().unwrap();
    assert_eq!(response.status(), 200);
    thread::sleep(Duration::from_millis(250));
    let mut total = 0;
    let mut block = [0u8; 65536];
    loop {
        let count = response.read(&mut block).unwrap();
        if count == 0 {
            break;
        }
        total += count;
        assert!(block[..count].iter().all(|byte| *byte == 37));
    }
    assert_eq!(total, SIZE);
    drop(relay);
    worker.join().unwrap();
}

#[test]
fn relay_forwards_only_selected_object_and_range_with_host_credentials() {
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    let remote = format!("http://{}/selected", upstream.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let request = String::from_utf8(request).unwrap().to_lowercase();
        assert!(request.starts_with("get /selected http/1.1"));
        assert!(request.contains("range: bytes=1-3\r\n"));
        assert!(request.contains("authorization: bearer host-token\r\n"));
        assert!(request.contains("x-download-ticket: host-ticket\r\n"));
        assert!(!request.contains("attacker"));
        stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Type: application/zip\r\nContent-Range: bytes 1-3/4\r\nContent-Length: 3\r\nConnection: close\r\n\r\nbcd").unwrap();
    });
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let relay = Relay::start(
        client.clone(),
        remote,
        "host-token".into(),
        "host-ticket".into(),
        4,
    )
    .unwrap();
    let bad = client.get(format!("{}/wrong", relay.url)).send().unwrap();
    assert_eq!(bad.status(), 404);
    let response = client
        .get(&relay.url)
        .header("Range", "bytes=1-3")
        .header("Authorization", "attacker")
        .send()
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.text().unwrap(), "bcd");
    drop(relay);
    worker.join().unwrap();
}

#[test]
fn relay_never_passes_remote_redirect_to_engine() {
    let upstream = TcpListener::bind("127.0.0.1:0").unwrap();
    let remote = format!("http://{}/selected", upstream.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = [0; 4096];
        let _ = stream.read(&mut bytes).unwrap();
        stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://example.com/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let relay = Relay::start(
        client.clone(),
        remote,
        "host-token".into(),
        "host-ticket".into(),
        4,
    )
    .unwrap();
    let response = client.get(&relay.url).send().unwrap();
    assert_eq!(response.status(), 403);
    assert!(!response.headers().contains_key("location"));
    drop(relay);
    worker.join().unwrap();
}
