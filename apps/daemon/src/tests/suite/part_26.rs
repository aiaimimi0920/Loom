// Loom daemon tests fragment 26; included into the shared crate test module.
fn shared_tea_brain_provider_example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("tea-brain-provider")
            .join(name),
    )
    .expect("read standalone Tea BrainProvider fixture")
}

fn http_request(port: u16, method: &str, path: &str, body: Option<&str>) -> String {
    let token = test_bound_daemon_token(port).unwrap_or_else(|| TEST_DAEMON_AUTH_TOKEN.to_owned());
    http_request_with_bearer(port, method, path, body, &token)
}

fn http_request_without_auth(port: u16, method: &str, path: &str, body: Option<&str>) -> String {
    http_request_with_extra_headers(port, method, path, body, "")
}

fn http_request_with_bearer(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    token: &str,
) -> String {
    http_request_with_extra_headers(
        port,
        method,
        path,
        body,
        &format!("Authorization: Bearer {token}\r\n"),
    )
}

fn http_request_with_extra_headers(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    extra_headers: &str,
) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect daemon");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set timeout");
    if let Some(body) = body {
        write!(
                stream,
                "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write request");
    } else {
        write!(
                stream,
                "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{extra_headers}Connection: close\r\n\r\n"
            )
            .expect("write request");
    }

    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    response
}

fn http_request_with_declared_content_length(
    port: u16,
    method: &str,
    path: &str,
    content_length: usize,
    token: Option<&str>,
) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect daemon");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {content_length}\r\n{authorization}Connection: close\r\n\r\n",
        )
        .expect("write request");
    stream.shutdown(Shutdown::Write).expect("shutdown write");

    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    response
}

fn response_json_body(response: &str) -> serde_json::Value {
    let body = response.split_once("\r\n\r\n").expect("response body").1;
    serde_json::from_str(body).expect("json body")
}

fn restore_env(name: &str, value: Option<String>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

fn current_test_binary_mcp_fixture_config() -> serde_json::Value {
    current_test_binary_mcp_fixture_config_with_env(&[])
}

fn current_test_binary_mcp_fixture_config_with_env(
    extra_env: &[(&str, String)],
) -> serde_json::Value {
    let exe = std::env::current_exe().expect("current test executable");
    let mut env = serde_json::Map::new();
    env.insert(
        "LOOM_DAEMON_MCP_FIXTURE_SERVER".to_owned(),
        Value::String("1".to_owned()),
    );
    for (key, value) in extra_env {
        env.insert((*key).to_owned(), Value::String(value.clone()));
    }
    serde_json::json!({
        "id": "fixture",
        "name": "Fixture MCP",
        "command": exe.display().to_string(),
        "args": [
            "tests::daemon_mcp_fixture_server",
            "--exact",
            "--nocapture"
        ],
        "env": env,
        "transport": "stdio",
        "enabled": true
    })
}

fn run_mcp_fixture_server() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let fixture_image_url = std::env::var("LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL").ok();
    let fixture_image_url_alt = std::env::var("LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL_ALT").ok();

    for line in stdin.lock().lines() {
        let line = line.expect("fixture stdin line");
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let method = request["method"].as_str().unwrap_or_default();
        match method {
            "initialize" => write_fixture_response(
                &mut stdout,
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request["id"].clone(),
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": {} },
                        "serverInfo": {
                            "name": "daemon-fixture",
                            "version": "0.1.0"
                        }
                    }
                }),
            ),
            "notifications/initialized" => {}
            "tools/list" => write_fixture_response(
                &mut stdout,
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request["id"].clone(),
                    "result": {
                        "tools": [
                            {
                                "name": "echo",
                                "description": "Echo arguments",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "text": { "type": "string" }
                                    }
                                }
                            },
                            {
                                "name": "brave_image_search",
                                "description": "Return structured image-search results",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "query": { "type": "string" },
                                        "count": { "type": "integer" },
                                        "search_lang": {
                                            "type": "string",
                                            "enum": ["zh-hans", "en"]
                                        },
                                        "spellcheck": { "type": "boolean" }
                                    },
                                    "required": ["query"]
                                }
                            },
                            {
                                "name": "brave_image_search_realshape",
                                "description": "Return structured image-search results with Brave-like string-only search_lang schema",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "query": { "type": "string" },
                                        "count": { "type": "integer" },
                                        "search_lang": { "type": "string" },
                                        "spellcheck": { "type": "boolean" }
                                    },
                                    "required": ["query"]
                                }
                            }
                        ]
                    }
                }),
            ),
            "tools/call" => {
                let tool_name = request["params"]["name"].as_str().unwrap_or_default();
                match tool_name {
                    "echo" => {
                        let text = request["params"]["arguments"]["text"]
                            .as_str()
                            .unwrap_or_default();
                        write_fixture_response(
                            &mut stdout,
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": request["id"].clone(),
                                "result": {
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": text
                                        }
                                    ]
                                }
                            }),
                        );
                    }
                    "brave_image_search" | "brave_image_search_realshape" => {
                        let arguments = &request["params"]["arguments"];
                        if arguments.get("count").is_some()
                            && !arguments["count"].is_i64()
                            && !arguments["count"].is_u64()
                        {
                            write_fixture_response(
                                &mut stdout,
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": request["id"].clone(),
                                    "error": {
                                        "code": -32602,
                                        "message": "count must be an integer"
                                    }
                                }),
                            );
                            continue;
                        }
                        if arguments.get("spellcheck").is_some()
                            && !arguments["spellcheck"].is_boolean()
                        {
                            write_fixture_response(
                                &mut stdout,
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": request["id"].clone(),
                                    "error": {
                                        "code": -32602,
                                        "message": "spellcheck must be a boolean"
                                    }
                                }),
                            );
                            continue;
                        }
                        if let Some(search_lang) = arguments
                            .get("search_lang")
                            .and_then(serde_json::Value::as_str)
                        {
                            if !matches!(search_lang, "zh-hans" | "en") {
                                write_fixture_response(
                                    &mut stdout,
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": request["id"].clone(),
                                        "error": {
                                            "code": -32602,
                                            "message": "search_lang must be one of [\"zh-hans\", \"en\"]"
                                        }
                                    }),
                                );
                                continue;
                            }
                        } else if arguments.get("search_lang").is_some() {
                            write_fixture_response(
                                &mut stdout,
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": request["id"].clone(),
                                    "error": {
                                        "code": -32602,
                                        "message": "search_lang must be a string"
                                    }
                                }),
                            );
                            continue;
                        }
                        let query = request["params"]["arguments"]["query"]
                            .as_str()
                            .unwrap_or_default();
                        if query.contains("offensive fixture") {
                            write_fixture_response(
                                &mut stdout,
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": request["id"].clone(),
                                    "result": {
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": "{\"type\":\"object\",\"items\":[],\"count\":0,\"might_be_offensive\":true}"
                                            }
                                        ],
                                        "structuredContent": {
                                            "type": "object",
                                            "items": [],
                                            "count": 0,
                                            "might_be_offensive": true
                                        }
                                    }
                                }),
                            );
                            continue;
                        }
                        let image_url = fixture_image_url
                            .clone()
                            .unwrap_or_else(|| "https://example.invalid/fixture.png".to_owned());
                        let alternate_image_url = fixture_image_url_alt
                            .clone()
                            .unwrap_or_else(|| image_url.clone());
                        write_fixture_response(
                            &mut stdout,
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": request["id"].clone(),
                                "result": {
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": format!("fixture brave_image_search results for {query}")
                                        }
                                    ],
                                    "structuredContent": {
                                        "type": "object",
                                        "items": [
                                            {
                                                "title": "Fixture image",
                                                "url": "https://example.invalid/page",
                                                "properties": {
                                                    "url": image_url,
                                                    "width": 1,
                                                    "height": 1
                                                }
                                            },
                                            {
                                                "title": "Fixture image alternate",
                                                "url": "https://example.invalid/page-2",
                                                "properties": {
                                                    "url": alternate_image_url,
                                                    "width": 1,
                                                    "height": 1
                                                }
                                            }
                                        ]
                                    }
                                }
                            }),
                        );
                    }
                    _ => write_fixture_response(
                        &mut stdout,
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request["id"].clone(),
                            "error": {
                                "code": -32601,
                                "message": format!("unknown tool {tool_name}")
                            }
                        }),
                    ),
                }
            }
            _ => write_fixture_response(
                &mut stdout,
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request["id"].clone(),
                    "error": {
                        "code": -32601,
                        "message": format!("unknown method {method}")
                    }
                }),
            ),
        }
    }
}
