use super::*;
use crate::network::RuntimeProxy;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn headers(stream: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 16384);
        bytes.push(stream.read_u8().await.unwrap());
    }
    bytes
}

#[tokio::test]
async fn dropping_lease_cancels_active_tunnel_and_releases_listener() {
    tokio::time::timeout(Duration::from_secs(3), async {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let routing = Arc::new(Routing::snapshot(RuntimeProxy::Custom(format!(
            "http://{}",
            upstream.local_addr().unwrap()
        ))));
        let lease = Lease::start(routing, OutboundPolicy::default()).unwrap();
        let port = lease.url.port().unwrap();
        let auth =
            base64::engine::general_purpose::STANDARD.encode(format!("loom:{}", lease.token));
        let client = tokio::spawn(async move {
            let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            client
                .write_all(
                    format!(
                        "CONNECT 8.8.8.8:443 HTTP/1.1\r\nProxy-Authorization: Basic {auth}\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            assert!(headers(&mut client).await.starts_with(b"HTTP/1.1 200 "));
            client
        });
        let (mut upstream, _) = upstream.accept().await.unwrap();
        assert!(headers(&mut upstream)
            .await
            .starts_with(b"CONNECT 8.8.8.8:443 "));
        upstream
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .unwrap();
        let mut client = client.await.unwrap();
        drop(lease);
        let mut byte = [0];
        assert_eq!(client.read(&mut byte).await.unwrap(), 0);
        assert_eq!(upstream.read(&mut byte).await.unwrap(), 0);
        // EOF above proves the abort was observed, rather than only requested.
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    })
    .await
    .expect("dropping a proxy lease must promptly stop every child connection");
}

#[tokio::test]
async fn unauthenticated_idle_connections_have_a_per_client_cap() {
    let routing = Arc::new(Routing::snapshot(RuntimeProxy::Disabled));
    let lease = Lease::start(routing, OutboundPolicy::default()).unwrap();
    let mut readers = JoinSet::new();
    for _ in 0..=MAX_CLIENT_CONNECTIONS {
        let mut connection = TcpStream::connect(("127.0.0.1", lease.url.port().unwrap()))
            .await
            .unwrap();
        readers.spawn(async move {
            let mut byte = [0];
            connection.read(&mut byte).await
        });
    }
    let rejected = tokio::time::timeout(Duration::from_secs(2), readers.join_next())
        .await
        .expect("excess connections must not queue behind idle unauthenticated peers")
        .unwrap()
        .unwrap();
    assert!(matches!(rejected, Ok(0)) || rejected.is_err());
    drop(lease);
    readers.abort_all();
    while readers.join_next().await.is_some() {}
}
