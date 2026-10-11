//! Cleartext development requests must stay on an explicit local IP, not a proxy.
use loom_gateway::{
    configure_runtime_proxy, GatewayChatMessage, GatewayChatRequest, GatewayClient,
    GatewayClientConfig, GatewayError,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

fn request() -> GatewayChatRequest {
    GatewayChatRequest {
        model: "fixture".into(),
        messages: vec![GatewayChatMessage::user("private prompt")],
        stream: false,
        temperature: None,
        response_format: None,
    }
}

fn read_headers(stream: &mut TcpStream) -> String {
    // Windows accepted sockets can inherit the listener's nonblocking mode.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 8192);
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).unwrap()
}

#[test]
fn remote_cleartext_is_rejected_even_without_a_bearer_token() {
    for host in [
        "example.test",
        "localhost",
        "localhost.",
        "10.0.0.2",
        "192.168.1.1",
        "169.254.169.254",
        "0.0.0.0",
        "8.8.8.8",
        "[::]",
        "[2001:db8::1]",
        "[::ffff:10.0.0.2]",
    ] {
        for token in ["", "private-token"] {
            let result = GatewayClient::new(
                GatewayClientConfig::new(format!("http://{host}:4200"))
                    .with_auth_token(token)
                    .without_proxy(),
            );
            assert!(result.is_err(), "cleartext accepted for {host}");
            let error = result.unwrap_err();
            assert!(matches!(error, GatewayError::InsecureTransport));
            assert!(!error.to_string().contains("private-token"));
        }
    }
    for origin in [
        "https://example.test",
        "http://127.0.0.1:4200",
        "http://127.0.0.2:4200",
        "http://[::1]:4200",
        "http://[::ffff:127.0.0.1]:4200",
    ] {
        assert!(GatewayClient::new(GatewayClientConfig::new(origin).without_proxy()).is_ok());
    }
}

#[test]
fn cleartext_loopback_is_direct_but_https_keeps_custom_proxy() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_address = proxy.local_addr().unwrap();
    configure_runtime_proxy("custom", "http", &proxy_address.to_string()).unwrap();
    let gateway = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = GatewayClient::new(
        GatewayClientConfig::new(format!("http://{}", gateway.local_addr().unwrap()))
            .with_auth_token("private-token")
            .with_timeout(Duration::from_secs(2)),
    )
    .unwrap();
    let worker = std::thread::spawn(move || client.chat(request()));
    gateway.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut socket = loop {
        match gateway.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "local HTTP was routed through proxy"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("{error}"),
        }
    };
    let headers = read_headers(&mut socket).to_ascii_lowercase();
    assert!(headers.contains("authorization: bearer private-token"));
    socket
        .write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .unwrap();
    assert!(worker.join().unwrap().is_err());
    proxy.set_nonblocking(true).unwrap();
    assert!(matches!(proxy.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));

    let secure = GatewayClient::new(
        GatewayClientConfig::new("https://gateway.invalid")
            .with_auth_token("private-token")
            .with_timeout(Duration::from_secs(2)),
    )
    .unwrap();
    let worker = std::thread::spawn(move || secure.chat(request()));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut socket = loop {
        match proxy.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "HTTPS proxy was bypassed"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("{error}"),
        }
    };
    let headers = read_headers(&mut socket);
    assert!(headers.starts_with("CONNECT gateway.invalid:443 "));
    assert!(!headers.contains("private-token"));
    socket
        .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    assert!(worker.join().unwrap().is_err());
    configure_runtime_proxy("system", "", "").unwrap();
}
