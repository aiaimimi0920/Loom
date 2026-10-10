use super::*;

#[tokio::test]
async fn stalled_first_proxy_peer_leaves_time_for_second_approved_peer() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let routing = Routing::snapshot(crate::network::RuntimeProxy::Custom(format!(
        "http://{}",
        listener.local_addr().unwrap()
    )));
    let proxy = routing
        .select(&Url::parse("https://target.test/").unwrap())
        .unwrap();
    let server = tokio::spawn(async move {
        let (mut stalled, _) = listener.accept().await.unwrap();
        assert!(read_headers(&mut stalled)
            .await
            .unwrap()
            .starts_with(b"CONNECT 8.8.8.8:443 "));
        // Deliberately never respond to the first CONNECT; it must be cancelled.
        let (mut reachable, _) = listener.accept().await.unwrap();
        assert!(read_headers(&mut reachable)
            .await
            .unwrap()
            .starts_with(b"CONNECT 1.1.1.1:443 "));
        reachable
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\nOK")
            .await
            .unwrap();
        let mut byte = [0];
        assert_eq!(stalled.read(&mut byte).await.unwrap(), 0);
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let result = tokio::time::timeout_at(
        deadline,
        connect_peers(
            &[
                "8.8.8.8:443".parse().unwrap(),
                "1.1.1.1:443".parse().unwrap(),
            ],
            Some(&proxy),
            &OutboundPolicy::default(),
            deadline,
        ),
    )
    .await
    .expect("the overall budget must leave time for fallback");
    let mut tunnel = result.unwrap();
    let mut body = [0; 2];
    tunnel.read_exact(&mut body).await.unwrap();
    assert_eq!(&body, b"OK");
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("cancelled peer and mock server must finish")
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn one_way_transfer_keeps_tunnel_alive_but_total_idle_closes_it() {
    let (mut client, incoming) = tokio::io::duplex(64);
    let (tunnel, mut upstream) = tokio::io::duplex(64);
    let relay = tokio::spawn(relay_streams(incoming, tunnel, Duration::from_secs(1)));
    for _ in 0..6 {
        upstream.write_all(b"data").await.unwrap();
        let mut received = [0; 4];
        client.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"data");
        tokio::time::advance(Duration::from_millis(500)).await;
    }
    assert!(
        !relay.is_finished(),
        "a silent upload is not an idle download"
    );
    tokio::time::advance(Duration::from_secs(2)).await;
    let error = relay.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("idle deadline"));
}

#[test]
fn rejects_duplicate_credentials_and_non_authority_connect_targets() {
    for raw in [
        "CONNECT allowed.test:443 HTTP/1.1\r\nProxy-Authorization: token\r\nProxy-Authorization: token\r\n\r\n",
        "CONNECT user@allowed.test:443 HTTP/1.1\r\nProxy-Authorization: token\r\n\r\n",
        "CONNECT allowed.test:443/path HTTP/1.1\r\nProxy-Authorization: token\r\n\r\n",
        "GET https://allowed.test/ HTTP/1.1\r\nProxy-Authorization: token\r\n\r\n",
    ] {
        assert!(parse_request(raw.as_bytes(), "token").is_err());
    }
}
