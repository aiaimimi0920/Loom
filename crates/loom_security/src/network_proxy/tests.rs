use super::*;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn headers(stream: &mut (impl AsyncRead + Unpin)) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 256 * 1024);
        bytes.push(stream.read_u8().await.unwrap());
    }
    String::from_utf8(bytes).unwrap()
}

fn protected_client(proxy: String, policy: OutboundPolicy) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .proxy(
            protected_proxy_with_mode(policy, RuntimeProxy::Custom(proxy))
                .unwrap()
                .unwrap(),
        )
        .timeout(Duration::from_secs(3))
}

fn local_policy() -> OutboundPolicy {
    OutboundPolicy {
        allow_http_loopback: true,
        ..OutboundPolicy::default()
    }
}

#[test]
fn custom_proxy_normalizes_idna_without_silently_selecting_direct() {
    for scheme in ["http", "https"] {
        let routing = Routing::snapshot(RuntimeProxy::Custom(format!(
            "{scheme}://bücher.example:8080"
        )))
        .unwrap();
        for target in ["http://target.test/", "https://target.test/"] {
            let selected = routing.select(&Url::parse(target).unwrap()).unwrap();
            assert_eq!(selected.uri().host(), Some("xn--bcher-kva.example"));
            assert_eq!(selected.uri().port_u16(), Some(8080));
        }
    }
    for invalid in ["not a URL", "http://", "socks5://proxy.test:1080"] {
        assert!(Routing::snapshot(RuntimeProxy::Custom(invalid.into())).is_err());
    }
}

#[tokio::test]
async fn protected_http_client_preserves_mcp_header_budget_and_managed_overhead() {
    for count in [1, 64] {
        let configured: Vec<_> = (0..count)
            .map(|index| {
                (
                    format!("x-config-{index}"),
                    "v".repeat(if index < 7 { 16 * 1024 } else { 250 }),
                )
            })
            .collect();
        // Match MCP's public 64-field, 16 KiB/value, 128 KiB aggregate budget.
        assert!(
            configured
                .iter()
                .map(|(name, value)| name.len() + value.len() + 4)
                .sum::<usize>()
                <= 128 * 1024
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        let expected = configured.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            assert!(headers(&mut stream)
                .await
                .starts_with("CONNECT 127.0.0.1:45678 "));
            stream
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await
                .unwrap();
            let request = headers(&mut stream).await;
            for (name, value) in expected {
                assert!(request.contains(&format!("{name}: {value}\r\n")));
            }
            assert!(request.contains("mcp-protocol-version: 2025-03-26\r\n"));
            assert!(!request.to_ascii_lowercase().contains("proxy-authorization"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
        });
        let client = protected_client(proxy, local_policy()).build().unwrap();
        let mut request = client
            .get("http://127.0.0.1:45678/mcp")
            .header("mcp-protocol-version", "2025-03-26")
            .header("authorization", "Bearer managed")
            .header("accept", "application/json, text/event-stream");
        for (name, value) in configured {
            request = request.header(name, value);
        }
        assert!(request.send().await.unwrap().status().is_success());
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn protected_http_client_uses_numeric_proxy_connect_and_does_not_leak_proxy_auth() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!(
        "http://operator:test-secret@{}",
        listener.local_addr().unwrap()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = headers(&mut stream).await;
        assert!(connect.starts_with("CONNECT 127.0.0.1:45678 HTTP/1.1\r\n"));
        assert!(connect
            .to_ascii_lowercase()
            .contains("proxy-authorization: basic "));
        stream
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .unwrap();
        let request = headers(&mut stream).await;
        assert!(request.starts_with("GET /resource?value=1 HTTP/1.1\r\n"));
        assert!(request
            .to_ascii_lowercase()
            .contains("host: 127.0.0.1:45678\r\n"));
        assert!(!request.to_ascii_lowercase().contains("proxy-authorization"));
        assert!(!request.contains("test-secret"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
            .await
            .unwrap();
    });
    let client = protected_client(proxy, local_policy()).build().unwrap();
    assert_eq!(
        client
            .get("http://127.0.0.1:45678/resource?value=1")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "OK"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn protected_client_rejects_loopback_resolution_without_proxy_or_direct_io() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let victim = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = victim.local_addr().unwrap().port();
    let client = protected_client(
        format!("http://{}", listener.local_addr().unwrap()),
        OutboundPolicy::default(),
    )
    .build()
    .unwrap();
    for url in [
        format!("https://localhost:{port}/"),
        format!("https://[::ffff:127.0.0.1]:{port}/"),
    ] {
        assert!(client.get(url).send().await.is_err());
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), victim.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejected_proxy_never_falls_back_to_an_approved_direct_peer() {
    let victim = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = protected_client(
        format!("http://{}", upstream.local_addr().unwrap()),
        local_policy(),
    )
    .build()
    .unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        headers(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
    });
    assert!(client
        .get(format!("https://{}/", victim.local_addr().unwrap()))
        .send()
        .await
        .is_err());
    server.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), victim.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn protected_https_client_preserves_original_sni_and_certificate_name() {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert.der().clone()], key)
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let connect = headers(&mut stream).await;
        assert!(
            connect.starts_with("CONNECT 127.0.0.1:45678 HTTP/1.1\r\n")
                || connect.starts_with("CONNECT [::1]:45678 HTTP/1.1\r\n")
        );
        assert!(!connect.contains("localhost"));
        stream
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .unwrap();
        let mut tls = tokio_rustls::TlsAcceptor::from(Arc::new(config))
            .accept(stream)
            .await
            .unwrap();
        assert_eq!(tls.get_ref().1.server_name(), Some("localhost"));
        let request = headers(&mut tls).await;
        assert!(request
            .to_ascii_lowercase()
            .contains("host: localhost:45678"));
        assert!(!request.to_ascii_lowercase().contains("proxy-authorization"));
        tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
            .await
            .unwrap();
    });
    let client = protected_client(proxy, local_policy())
        .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
        .build()
        .unwrap();
    assert_eq!(
        client
            .get("https://localhost:45678/")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "OK"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn unauthenticated_local_connection_cannot_reach_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let routing = Arc::new(
        Routing::snapshot(RuntimeProxy::Custom(format!(
            "http://{}",
            upstream.local_addr().unwrap()
        )))
        .unwrap(),
    );
    let relay = crate::network_proxy_relay::Lease::start(routing, local_policy()).unwrap();
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", relay.url.port().unwrap()))
        .await
        .unwrap();
    stream
        .write_all(b"CONNECT localhost:443 HTTP/1.1\r\nProxy-Authorization: wrong\r\n\r\n")
        .await
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(2), headers(&mut stream))
        .await
        .unwrap();
    assert!(response.starts_with("HTTP/1.1 407 "));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), upstream.accept())
            .await
            .is_err()
    );
}

#[test]
fn proxy_routing_keeps_no_proxy_and_per_scheme_selection() {
    let routing = Routing(
        Matcher::builder()
            .http("http://127.0.0.1:8000")
            .https("https://127.0.0.1:8443")
            .no("internal.example,127.0.0.0/8")
            .build(),
    );
    for url in ["https://internal.example/", "http://127.0.0.2/"] {
        assert!(routing.select(&Url::parse(url).unwrap()).is_none());
    }
    assert_eq!(
        routing
            .select(&Url::parse("http://external.example/").unwrap())
            .unwrap()
            .uri()
            .port_u16(),
        Some(8000)
    );
    assert_eq!(
        routing
            .select(&Url::parse("https://external.example/").unwrap())
            .unwrap()
            .uri()
            .port_u16(),
        Some(8443)
    );
    assert!(
        protected_proxy_with_mode(local_policy(), RuntimeProxy::Disabled)
            .unwrap()
            .is_none()
    );
    assert!(protected_proxy_with_mode(
        local_policy(),
        RuntimeProxy::Custom("ftp://proxy.test".into())
    )
    .is_err());
}
