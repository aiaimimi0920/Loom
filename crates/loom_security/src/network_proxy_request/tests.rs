use super::*;

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
