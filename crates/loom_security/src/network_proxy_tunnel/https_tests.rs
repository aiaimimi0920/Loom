use super::*;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn run_https_proxy(trust_proxy: bool, correct_proxy_name: bool) -> bool {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![if correct_proxy_name {
            "127.0.0.1".into()
        } else {
            "different-proxy.test".into()
        }])
        .unwrap();
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
    let proxy = Url::parse(&format!("https://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        if let Ok(mut tls) = acceptor.accept(stream).await {
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                assert!(header.len() < 8192);
                header.push(tls.read_u8().await.unwrap());
            }
            assert!(header.starts_with(b"CONNECT 8.8.8.8:443 HTTP/1.1\r\n"));
            tls.write_all(b"HTTP/1.1 200 Connection established\r\n\r\nsecure-proxy")
                .await
                .unwrap();
        }
    });
    let mut tls = transport::tls_config();
    if trust_proxy {
        // Test-only CA installation; production uses the standard WebPKI roots.
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert.der().clone()).unwrap();
        tls = Arc::new(
            rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth(),
        );
    }
    let result = connect_with_config(
        &intercept(&proxy),
        "8.8.8.8:443".parse().unwrap(),
        &OutboundPolicy::default(),
        Duration::from_secs(2),
        tls,
    )
    .await;
    let connected = match result {
        Ok(mut stream) => {
            let mut body = [0; 12];
            stream.read_exact(&mut body).await.unwrap();
            assert_eq!(&body, b"secure-proxy");
            true
        }
        Err(_) => false,
    };
    server.await.unwrap();
    connected
}

#[tokio::test]
async fn https_proxy_connect_requires_trusted_certificate_and_matching_proxy_name() {
    tokio::time::timeout(Duration::from_secs(10), async {
        assert!(run_https_proxy(true, true).await);
        assert!(!run_https_proxy(false, true).await);
        assert!(!run_https_proxy(true, false).await);
    })
    .await
    .expect("HTTPS proxy fixture deadline");
}
