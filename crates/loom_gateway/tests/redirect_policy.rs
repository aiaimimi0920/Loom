//! Redirect responses must never relay chat payloads to another destination.
use loom_gateway::{
    GatewayChatMessage, GatewayChatRequest, GatewayClient, GatewayClientConfig, GatewayError,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

fn consume_request(stream: &mut TcpStream) -> String {
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
    let headers = String::from_utf8(bytes).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    assert!(length < 8192);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    assert!(String::from_utf8(body)
        .unwrap()
        .contains("private-workflow-context"));
    headers
}

#[test]
fn gateway_redirects_never_forward_requests_or_credentials() {
    for status in [301, 302, 303, 307, 308] {
        for same_origin in [false, true] {
            let gateway = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = gateway.local_addr().unwrap();
            let target = if same_origin {
                gateway.try_clone().unwrap()
            } else {
                TcpListener::bind("127.0.0.1:0").unwrap()
            };
            let location = if same_origin {
                "/redirected".to_owned()
            } else {
                format!("http://{}/redirected", target.local_addr().unwrap())
            };
            let server = thread::spawn(move || {
                let (mut stream, _) = gateway.accept().unwrap();
                let headers = consume_request(&mut stream);
                assert!(headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer gateway-fixture"));
                write!(stream, "HTTP/1.1 {status} Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            });
            let client = GatewayClient::new(
                GatewayClientConfig::new_loopback(format!("http://{address}"))
                    .unwrap()
                    .without_proxy()
                    .with_auth_token("gateway-fixture")
                    .with_timeout(Duration::from_secs(2)),
            )
            .unwrap();
            let result = client.chat(GatewayChatRequest {
                model: "fixture".to_owned(),
                messages: vec![GatewayChatMessage::user("private-workflow-context")],
                stream: false,
                temperature: None,
                response_format: None,
            });
            server.join().unwrap();
            target.set_nonblocking(true).unwrap();
            assert!(
                matches!(target.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
                "redirect {status}, same_origin={same_origin} opened a destination connection"
            );
            assert!(
                matches!(result, Err(GatewayError::HttpStatus { status: returned, .. }) if returned == status)
            );
        }
    }
}
