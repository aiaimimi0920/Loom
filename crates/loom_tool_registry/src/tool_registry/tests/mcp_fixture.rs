//! Executable MCP fixture server.

use super::*;

#[test]
pub(super) fn mcp_registry_fixture_server() {
    if std::env::var("LOOM_TOOL_REGISTRY_MCP_FIXTURE_SERVER")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }

    run_mcp_fixture_server();
    std::process::exit(0);
}

pub(super) fn current_test_binary_fixture_config() -> loom_mcp::McpServerConfig {
    let exe = std::env::current_exe().expect("current test executable");
    loom_mcp::McpServerConfig::new("fixture", "Fixture MCP", exe.display().to_string())
        .arg("tool_registry::tests::mcp_fixture::mcp_registry_fixture_server")
        .arg("--exact")
        .arg("--nocapture")
        .env("LOOM_TOOL_REGISTRY_MCP_FIXTURE_SERVER", "1")
}

pub(super) fn run_mcp_fixture_server() {
    if std::env::var("LOOM_TOOL_REGISTRY_MCP_FIXTURE_MODE")
        .ok()
        .as_deref()
        == Some("hang")
    {
        thread::sleep(Duration::from_secs(30));
        return;
    }
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let fixture_image_url = std::env::var("LOOM_MCP_FIXTURE_IMAGE_URL").ok();
    let fixture_image_url_alt = std::env::var("LOOM_MCP_FIXTURE_IMAGE_URL_ALT").ok();
    let mut counter = 0_u64;

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
                            "name": "tool-registry-fixture",
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
                                "name": "counter",
                                "description": "Count calls in this fixture process",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {},
                                    "additionalProperties": false
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
                    "counter" => {
                        counter += 1;
                        write_fixture_response(
                            &mut stdout,
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": request["id"].clone(),
                                "result": {
                                    "content": [{ "type": "text", "text": counter.to_string() }]
                                }
                            }),
                        );
                    }
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

pub(super) fn write_fixture_response(stdout: &mut impl Write, response: serde_json::Value) {
    writeln!(
        stdout,
        "\n{}",
        serde_json::to_string(&response).expect("serialize fixture response")
    )
    .expect("write fixture response");
    stdout.flush().expect("flush fixture response");
}
