use super::*;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn read_headers(stream: &mut (impl AsyncRead + Unpin)) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 8192, "bounded test proxy headers");
        bytes.push(stream.read_u8().await.unwrap());
    }
    String::from_utf8(bytes).unwrap()
}

async fn mock_proxy() -> (TcpListener, Url) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    (listener, url)
}

#[tokio::test]
async fn connect_uses_numeric_authority_and_preserves_upgrade_bytes() {
    for peer in ["8.8.8.8:443", "8.8.8.8:80", "[2606:4700:4700::1111]:443"] {
        let (listener, proxy) = mock_proxy().await;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let headers = read_headers(&mut stream).await;
            stream
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\ntunnel-data")
                .await
                .unwrap();
            headers
        });
        let mut tunnel = connect(
            &proxy,
            peer.parse().unwrap(),
            &OutboundPolicy::default(),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        let mut data = Vec::new();
        tunnel.read_to_end(&mut data).await.unwrap();
        assert_eq!(data, b"tunnel-data");
        assert!(server
            .await
            .unwrap()
            .starts_with(&format!("CONNECT {peer} HTTP/1.1\r\n")));
    }
}

#[tokio::test]
async fn forbidden_peer_never_connects_to_proxy() {
    let (listener, proxy) = mock_proxy().await;
    for peer in [
        "127.0.0.1:443",
        "[::ffff:127.0.0.1]:443",
        "169.254.169.254:80",
    ] {
        assert!(connect(
            &proxy,
            peer.parse().unwrap(),
            &OutboundPolicy::default(),
            Duration::from_secs(1),
        )
        .await
        .is_err());
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn proxy_auth_failure_is_not_a_direct_fallback_or_secret_leak() {
    let (listener, mut proxy) = mock_proxy().await;
    proxy.set_username("operator").unwrap();
    proxy.set_password(Some("test-proxy-secret")).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let headers = read_headers(&mut stream).await;
        assert!(headers
            .to_ascii_lowercase()
            .contains("proxy-authorization: basic "));
        stream
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
    });
    let error = connect(
        &proxy,
        "8.8.8.8:443".parse().unwrap(),
        &OutboundPolicy::default(),
        Duration::from_secs(2),
    )
    .await
    .err()
    .expect("proxy authentication must fail");
    assert_eq!(
        error.to_string(),
        "outbound proxy CONNECT returned status 407"
    );
    server.await.unwrap();
}

async fn target_tls_handshake(name: &'static str) -> bool {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["original-target.test".into()]).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into();
    let server_config = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], key)
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let client_config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let (listener, proxy) = mock_proxy().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        assert!(read_headers(&mut stream)
            .await
            .starts_with("CONNECT 8.8.8.8:443 HTTP/1.1\r\n"));
        stream
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
        if let Ok(mut tls) = acceptor.accept(stream).await {
            let (_, session) = tls.get_ref();
            assert_eq!(session.server_name(), Some("original-target.test"));
            tls.write_all(b"verified-target").await.unwrap();
        }
    });
    let tunnel = connect(
        &proxy,
        "8.8.8.8:443".parse().unwrap(),
        &OutboundPolicy::default(),
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
    let result = connector.connect(name.try_into().unwrap(), tunnel).await;
    let valid = match result {
        Ok(mut tls) => {
            let mut body = [0; 15];
            tls.read_exact(&mut body).await.unwrap();
            assert_eq!(&body, b"verified-target");
            true
        }
        Err(_) => false,
    };
    server.await.unwrap();
    valid
}

#[tokio::test]
async fn numeric_connect_keeps_original_target_sni_and_certificate_validation() {
    assert!(target_tls_handshake("original-target.test").await);
    assert!(!target_tls_handshake("wrong-target.test").await);
    assert!(!target_tls_handshake("8.8.8.8").await);
}

#[tokio::test]
async fn upstream_response_headers_and_handshake_time_are_bounded() {
    let (listener, proxy) = mock_proxy().await;
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_headers(&mut stream).await;
        let mut oversized = b"HTTP/1.1 200 OK\r\nX-Padding: ".to_vec();
        oversized.extend(vec![b'a'; 16 * 1024]);
        oversized.extend_from_slice(b"\r\n\r\n");
        let _ = stream.write_all(&oversized).await;
    });
    let error = connect(
        &proxy,
        "8.8.8.8:80".parse().unwrap(),
        &OutboundPolicy::default(),
        Duration::from_secs(2),
    )
    .await
    .err()
    .expect("oversized proxy headers must fail");
    assert!(error.to_string().contains("header limit"));
    server.await.unwrap();

    let (listener, proxy) = mock_proxy().await;
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let error = connect(
        &proxy,
        "8.8.8.8:443".parse().unwrap(),
        &OutboundPolicy::default(),
        Duration::from_millis(100),
    )
    .await
    .err()
    .expect("silent proxies must hit a deadline");
    assert!(error.to_string().contains("deadline"));
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
}
