use super::*;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

fn request(model: &str) -> GatewayChatRequest {
    GatewayChatRequest {
        model: model.to_owned(),
        messages: vec![
            GatewayChatMessage::system("return JSON"),
            GatewayChatMessage::user("hello"),
        ],
        stream: false,
        temperature: None,
        response_format: None,
    }
}

fn read_request(socket: &mut std::net::TcpStream) -> String {
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set mock read timeout");
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end;
    loop {
        let bytes = socket.read(&mut chunk).expect("read request");
        assert!(bytes > 0, "mock request ended before headers");
        buffer.extend_from_slice(&chunk[..bytes]);
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("content length"))
        })
        .unwrap_or(0);
    while buffer.len() < header_end + content_length {
        let bytes = socket.read(&mut chunk).expect("read request body");
        assert!(bytes > 0, "mock request ended before body");
        buffer.extend_from_slice(&chunk[..bytes]);
    }
    String::from_utf8(buffer).expect("request is UTF-8")
}

fn write_response(mut socket: std::net::TcpStream, status: &str, body: &str) {
    socket
        .write_all(
            format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-Request-Id: request-test\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .expect("write mock response");
}

#[test]
fn gateway_client_posts_openai_chat_request_with_auth_header() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock gateway");
    let address = listener.local_addr().expect("mock address");
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept request");
        let request = read_request(&mut socket);
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request_lower.contains("authorization: bearer test-token"));
        let body = r#"{
            "id":"chatcmpl-test",
            "model":"gpt-test-resolved",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hello from gateway"},"finish_reason":"stop"}]
        }"#;
        write_response(socket, "200 OK", body);
    });

    let client = GatewayClient::new(
        GatewayClientConfig::new(format!("http://{address}")).with_auth_token("test-token"),
    )
    .expect("create client");
    assert_eq!(client.origin_kind(), GatewayOriginKind::Loopback);
    let response = client.chat(request("gpt-test")).expect("chat response");
    assert_eq!(response.content, "hello from gateway");
    assert_eq!(response.model, "gpt-test-resolved");
    assert_eq!(response.request_id.as_deref(), Some("request-test"));
    server.join().expect("mock server");
}

#[test]
fn gateway_origin_policy_allows_private_lan_http_and_blocks_public_cleartext() {
    assert_eq!(
        classify_gateway_origin("http://127.0.0.1:4200").expect("loopback"),
        GatewayOriginKind::Loopback
    );
    assert_eq!(
        classify_gateway_origin("http://192.168.1.25:4200").expect("rfc1918 192.168"),
        GatewayOriginKind::PrivateLan
    );
    assert_eq!(
        classify_gateway_origin("http://10.0.0.8:4200").expect("rfc1918 10"),
        GatewayOriginKind::PrivateLan
    );
    assert_eq!(
        classify_gateway_origin("http://172.16.4.9:4200").expect("rfc1918 172.16"),
        GatewayOriginKind::PrivateLan
    );
    assert_eq!(
        classify_gateway_origin("http://[fd12:3456::1]:4200").expect("ipv6 ula"),
        GatewayOriginKind::PrivateLan
    );
    assert_eq!(
        classify_gateway_origin("https://gateway.example.com").expect("remote https"),
        GatewayOriginKind::RemoteTls
    );
    assert!(matches!(
        GatewayClient::new(GatewayClientConfig::new("http://8.8.8.8:4200")),
        Err(GatewayError::InsecureTransport)
    ));
    assert!(matches!(
        GatewayClient::new(GatewayClientConfig::new("http://gateway.example.com:4200")),
        Err(GatewayError::InsecureTransport)
    ));
}

#[test]
fn gateway_client_lists_models_and_forwards_raw_chat_completion() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock gateway");
    let address = listener.local_addr().expect("mock address");
    let server = thread::spawn(move || {
        let (mut s1, _) = listener.accept().expect("accept models");
        let r1 = read_request(&mut s1);
        assert!(r1.starts_with("GET /v1/models HTTP/1.1"));
        write_response(
            s1,
            "200 OK",
            r#"{"object":"list","data":[{"id":"qwen-lan"}]}"#,
        );

        let (mut s2, _) = listener.accept().expect("accept chat");
        let r2 = read_request(&mut s2);
        assert!(r2.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(r2.contains("\"stream\":false"));
        write_response(
            s2,
            "200 OK",
            r#"{"id":"cmpl-1","model":"qwen-lan","choices":[{"message":{"role":"assistant","content":"ok"}}]}"#,
        );
    });

    let client =
        GatewayClient::new(GatewayClientConfig::new(format!("http://{address}"))).expect("client");
    let models = client.list_models().expect("list models");
    assert_eq!(models["data"][0]["id"], "qwen-lan");

    let (completion, req_id) = client
        .chat_completion(&json!({
            "model": "qwen-lan",
            "messages": [{"role": "user", "content": "ping"}],
            "stream": true
        }))
        .expect("chat completion");
    assert_eq!(completion["choices"][0]["message"]["content"], "ok");
    assert_eq!(req_id.as_deref(), Some("request-test"));
    server.join().expect("mock server");
}

#[test]
fn loopback_config_rejects_non_loopback_hosts() {
    assert!(GatewayClientConfig::new_loopback("http://127.0.0.1:4200").is_ok());
    assert!(GatewayClientConfig::new_loopback("http://[::1]:4200").is_ok());
    assert!(GatewayClientConfig::new_loopback("http://10.0.0.2:4200").is_err());
    assert!(GatewayClientConfig::new_loopback("http://127.0.0.1:4200/api").is_err());
    assert!(GatewayClientConfig::new_loopback("ftp://127.0.0.1:4200").is_err());
}
