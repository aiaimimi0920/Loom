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

#[test]
fn unified_store_catalog_projects_multi_platform_matrix_and_mcp_capability_packages() {
    use std::io::Write;
    let root = std::env::temp_dir().join(format!(
        "loom-unified-store-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();

    let make_zip = |entry_name: &str, json_body: serde_json::Value| -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::SimpleFileOptions = zip::write::FileOptions::default();
            writer.start_file(entry_name, opts).unwrap();
            writer
                .write_all(serde_json::to_string(&json_body).unwrap().as_bytes())
                .unwrap();
            writer.finish().unwrap();
        }
        buf
    };

    let art_zip = make_zip(
        "manifest.json",
        serde_json::json!({
            "id": "multi-art",
            "name": "Multi Platform Art",
            "description": "supports windows and linux",
            "execution": { "type": "framework_art", "framework": "process" },
            "binary": "pingo.exe",
            "metadata": {
                "dependencies": { "framework": "process" },
                "platforms": {
                    "windows-x64": { "binary": "win/pingo.exe" },
                    "linux-x64": { "binary": "linux/pingo" },
                    "linux-arm64": { "binary": "linux-arm64/pingo" }
                },
                "packageSecurity": {
                    "version": "1.0.0",
                    "publisher": { "id": "neuro.official" }
                }
            }
        }),
    );
    let mcp_zip = make_zip(
        "mcp.server.json",
        serde_json::json!({
            "schemaVersion": 1,
            "id": "demo-mcp",
            "name": "Demo MCP",
            "description": "cross-platform mcp",
            "version": "1.1.0",
            "publisher": { "id": "neuro.official", "name": "Neuro" },
            "transport": "stdio",
            "entry": {
                "command": "runtime/demo.ps1",
                "platforms": {
                    "linux": { "command": "runtime/demo.mjs" }
                }
            },
            "tools": ["demo_tool"]
        }),
    );

    loom_art_store::store_published_zip(&root, Some("multi-art"), &art_zip).unwrap();
    let published_mcp =
        loom_art_store::store_published_zip(&root, Some("demo-mcp"), &mcp_zip).unwrap();
    assert_eq!(published_mcp.kind, StorePackageKind::McpServer);

    let catalog_resp = route(
        &Request {
            method: "GET".to_owned(),
            path: "/catalog".to_owned(),
            headers: Vec::new(),
            body: Vec::new(),
            peer_is_loopback: true,
        },
        &root,
    );
    assert_eq!(catalog_resp.status, 200);
    let catalog_json: serde_json::Value = serde_json::from_slice(&catalog_resp.body).unwrap();
    assert_eq!(catalog_json["arts"].as_array().unwrap().len(), 1);
    assert_eq!(catalog_json["packages"].as_array().unwrap().len(), 2);
    assert_eq!(
        catalog_json["arts"][0]["platforms"]["linux-x64"]["binary"],
        "linux/pingo"
    );
    assert_eq!(catalog_json["arts"][0]["binary"], "pingo.exe");

    let dl_resp = route(
        &Request {
            method: "GET".to_owned(),
            path: "/packages/mcp_server/demo-mcp/1.1.0.zip".to_owned(),
            headers: Vec::new(),
            body: Vec::new(),
            peer_is_loopback: true,
        },
        &root,
    );
    assert_eq!(dl_resp.status, 200);
    assert_eq!(dl_resp.body, mcp_zip);

    let _ = std::fs::remove_dir_all(root);
}
