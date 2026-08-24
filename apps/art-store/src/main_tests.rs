// HTTP parser limits and client-safe error response contracts.
use super::*;

fn parse_request(bytes: Vec<u8>) -> anyhow::Result<Option<Request>> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?;
    let client = std::thread::spawn(move || -> std::io::Result<()> {
        let mut stream = TcpStream::connect(address)?;
        stream.write_all(&bytes)
    });
    let (mut server, _) = listener.accept()?;
    let result = read_request(&mut server);
    client.join().expect("client thread")?;
    result
}

#[test]
fn request_parser_preserves_path_contract_and_rejects_ambiguous_framing() {
    let request = parse_request(
        b"POST /publish?source=test HTTP/1.1\r\nContent-Length: 3\r\n\r\nzip".to_vec(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/publish");
    assert_eq!(request.body, b"zip");

    assert!(parse_request(
        b"POST /publish HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec()
    )
    .is_err());
    assert!(parse_request(
        b"POST /publish HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx".to_vec()
    )
    .is_err());
    assert!(parse_request(b"GET /health\r\n\r\n".to_vec()).is_err());
    assert!(parse_request(b"GET health HTTP/1.1\r\n\r\n".to_vec()).is_err());
    assert!(parse_request(b"GET /health HTTP/1.1\r\nMissing-Colon\r\n\r\n".to_vec()).is_err());
    assert!(parse_request(b"GET /health HTTP/1.1\r\nContent-Length : 0\r\n\r\n".to_vec()).is_err());
    assert!(parse_request(b"GET /health HTTP/1.1\r\nX-Bad: \xff\r\n\r\n".to_vec()).is_err());
}

#[test]
fn request_parser_rejects_header_and_body_lengths_before_unbounded_reads() {
    let mut oversized_header = b"GET /health HTTP/1.1\r\nX-Fill: ".to_vec();
    oversized_header.extend(std::iter::repeat_n(b'x', MAX_REQUEST_HEADER_BYTES + 1));
    oversized_header.extend_from_slice(b"\r\n\r\n");
    assert!(parse_request(oversized_header).is_err());

    let oversized_body = format!(
        "POST /publish HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        MAX_REQUEST_BODY_BYTES + 1
    )
    .into_bytes();
    assert!(parse_request(oversized_body).is_err());

    assert!(
        parse_request(b"POST /publish HTTP/1.1\r\nContent-Length: 4\r\n\r\nzip".to_vec()).is_err()
    );
    assert!(parse_request(
        b"POST /publish HTTP/1.1\r\nContent-Length: 3\r\n\r\nzip-extra".to_vec()
    )
    .is_err());
}

#[test]
fn publisher_registration_requires_a_loopback_peer() {
    let request = Request {
        method: "POST".to_owned(),
        path: "/publishers/register".to_owned(),
        headers: Vec::new(),
        body: br#"{"userId":"L0000000000","keyId":"test","publicKey":"test"}"#.to_vec(),
        peer_is_loopback: false,
    };
    let response = route(&request, std::path::Path::new("unused"));
    assert_eq!(response.status, 403);
    assert_eq!(
        String::from_utf8(response.body).unwrap(),
        r#"{"error":"publisher registration requires a local connection"}"#
    );
}

#[test]
fn store_errors_do_not_expose_filesystem_or_parser_details() {
    let io = store_error_response(StoreError::Io(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        r"C:\private\publisher-directory.json",
    )));
    assert_eq!(io.status, 500);
    let body = String::from_utf8(io.body).unwrap();
    assert_eq!(body, r#"{"error":"Art Store internal error"}"#);
    assert!(!body.contains("private"));

    let json = store_error_response(StoreError::Json(
        serde_json::from_slice::<serde_json::Value>(b"{").unwrap_err(),
    ));
    assert_eq!(json.status, 400);
    assert_eq!(
        String::from_utf8(json.body).unwrap(),
        r#"{"error":"invalid package or JSON document"}"#
    );
}
