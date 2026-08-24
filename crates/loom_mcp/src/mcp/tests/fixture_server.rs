//! Executable stdio MCP fixture server.

use super::*;

#[test]
pub(super) fn mcp_fixture_server() {
    if std::env::var("LOOM_MCP_FIXTURE_SERVER").ok().as_deref() != Some("1") {
        return;
    }

    run_mcp_fixture_server();
    std::process::exit(0);
}

pub(super) fn current_test_binary_fixture_config() -> McpServerConfig {
    let exe = std::env::current_exe().expect("current test executable");
    McpServerConfig::new("fixture", "Fixture MCP", exe.display().to_string())
        .arg("mcp::tests::fixture_server::mcp_fixture_server")
        .arg("--exact")
        .arg("--nocapture")
        .env("LOOM_MCP_FIXTURE_SERVER", "1")
}

pub(super) fn run_mcp_fixture_server() {
    let fixture_mode = std::env::var("LOOM_MCP_FIXTURE_MODE").ok();
    match fixture_mode.as_deref() {
        Some("hang") => {
            std::thread::sleep(Duration::from_secs(30));
            return;
        }
        Some("stderr-flood") => {
            let mut stderr = std::io::stderr().lock();
            for _ in 0..256 {
                stderr
                    .write_all(&[b'e'; 8192])
                    .expect("write stderr fixture chunk");
            }
            stderr.flush().expect("flush stderr fixture");
        }
        Some("stderr-secret") => {
            let secret = std::env::var("FIXTURE_SECRET").expect("fixture secret");
            eprintln!("fixture failed with credential {secret}");
            std::thread::sleep(Duration::from_millis(50));
            return;
        }
        _ => {}
    }
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = line.expect("fixture stdin line");
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let method = request["method"].as_str().unwrap_or_default();
        if fixture_mode.as_deref() == Some("invalid-json-flood") {
            for _ in 0..=MCP_MAX_MALFORMED_MESSAGES {
                writeln!(stdout, "not-json").expect("write malformed fixture response");
            }
            stdout.flush().expect("flush malformed fixture responses");
            continue;
        }
        match method {
            "initialize" => {
                let requested_version = request["params"]["protocolVersion"]
                    .as_str()
                    .unwrap_or_default();
                let reject = fixture_mode.as_deref() == Some("reject-all-protocols")
                    || (fixture_mode.as_deref() == Some("reject-preferred")
                        && requested_version == MCP_PREFERRED_PROTOCOL_VERSION);
                let response = if reject {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"].clone(),
                        "error": {
                            "code": -32602,
                            "message": format!("unsupported protocol version {requested_version}")
                        }
                    })
                } else {
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"].clone(),
                        "result": {
                            "protocolVersion": if fixture_mode.as_deref() == Some("reject-preferred") {
                                requested_version
                            } else {
                                "2024-11-05"
                            },
                            "capabilities": { "tools": {} },
                            "serverInfo": {
                                "name": "loom-fixture",
                                "version": "0.1.0"
                            }
                        }
                    })
                };
                write_fixture_response(&mut stdout, response);
            }
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
                            }
                        ]
                    }
                }),
            ),
            "tools/call" => {
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
