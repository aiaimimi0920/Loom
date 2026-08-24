// Windows end-to-end image-search MCP framework execution.
#[cfg(windows)]
#[test]
fn independent_image_search_server_executes_through_mcp_framework() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture image API");
    let address = listener.local_addr().expect("fixture image API address");
    let fixture = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept image API request");
        let mut reader = BufReader::new(stream.try_clone().expect("clone fixture stream"));
        let mut request = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read request line");
            if line == "\r\n" || line.is_empty() {
                break;
            }
            request.push_str(&line);
        }
        assert!(request.starts_with(
            "GET /res/v1/images/search?q=loom%20framework&count=2&safesearch=strict HTTP/1.1\r\n"
        ));
        assert!(request
            .to_ascii_lowercase()
            .contains("x-subscription-token: fixture-api-key\r\n"));

        let body = json!({
            "results": [
                {
                    "title": "Loom first",
                    "url": "https://cdn.example.test/image-1.png",
                    "source": "https://example.test/source/1",
                    "thumbnail": { "src": "https://cdn.example.test/thumb-1.jpg" },
                    "properties": {
                        "url": "https://cdn.example.test/image-1.png",
                        "width": 640,
                        "height": 480
                    }
                },
                {
                    "title": "Loom second",
                    "url": "https://cdn.example.test/image-2.png",
                    "source": "https://example.test/source/2",
                    "thumbnail": { "src": "https://cdn.example.test/thumb-2.jpg" },
                    "properties": {}
                }
            ]
        })
        .to_string();
        write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write fixture response");
        stream.flush().expect("flush fixture response");
    });

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let art_dir = std::env::temp_dir().join(format!(
        "loom-image-search-mcp-framework-{}-{suffix}",
        std::process::id()
    ));
    fs::create_dir_all(&art_dir).expect("create staged Art directory");
    let art_source_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../art-packages/samples/image-search");
    let mcp_source_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mcp-server-packages/image-search");
    let manifest: Value = serde_json::from_slice(
        &fs::read(art_source_dir.join("manifest.json")).expect("read image-search manifest"),
    )
    .expect("parse image-search manifest");
    fs::write(
        art_dir.join("manifest.json"),
        serde_json::to_vec(&manifest).expect("serialize staged manifest"),
    )
    .expect("write staged manifest");

    let mut request = request();
    request.art_id = "custom-image-search".to_owned();
    request.art_dir = art_dir.clone();
    request.inputs = json!({});
    request.params = json!({
        "query": "loom framework",
        "count": "2",
        "result_index": 1,
        "__exec_manualTrigger": 123,
        "force_update": 456
    });
    request.context.credentials = vec![CredentialGrant {
        name: "brave_api_key".to_owned(),
        value: "fixture-api-key".to_owned(),
        expires_at: None,
    }];
    request.context.mcp_server = Some(FrameworkMcpServer {
        id: "neuro-image-search".to_owned(),
        package_id: "neuro.official/neuro-image-search".to_owned(),
        version: "0.1.0".to_owned(),
        transport: "stdio".to_owned(),
        command: mcp_source_dir
            .join("runtime/image-search-mcp.ps1")
            .display()
            .to_string(),
        // The endpoint is fixed in the server, because whoever picks it picks where the Brave
        // subscription key is sent. Pointing it at the fixture goes through the loopback-only
        // environment override, which a package manifest cannot set.
        env: BTreeMap::from([(
            "LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE".to_owned(),
            format!("http://{address}/res/v1/images/search"),
        )]),
        credential_env: BTreeMap::from([("BRAVE_API_KEY".to_owned(), "brave_api_key".to_owned())]),
        ..FrameworkMcpServer::default()
    });

    let execution = execute(&request, &art_dir).expect("execute independent MCP server");
    assert_eq!(execution.server_id, "neuro-image-search");
    assert_eq!(execution.tool_name.as_deref(), Some("brave_image_search"));
    let result = execution.result.as_ref().expect("legacy MCP result");
    assert_eq!(result["structuredContent"]["count"], 2);
    assert!(
        serde_json::to_value(&execution)
            .expect("serialize MCP execution")
            .get("arguments")
            .is_none(),
        "MCP arguments must not be echoed into the Art runtime payload"
    );
    assert_eq!(
        result["structuredContent"]["candidates"][0]["imageUrl"],
        "https://cdn.example.test/image-1.png"
    );
    assert_eq!(
        result["structuredContent"]["candidates"][1]["imageUrl"],
        "https://cdn.example.test/image-2.png"
    );

    fixture.join().expect("image API fixture thread");
    fs::remove_dir_all(&art_dir).expect("remove staged image-search Art");
}
