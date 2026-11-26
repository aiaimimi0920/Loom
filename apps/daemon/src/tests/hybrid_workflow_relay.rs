// Integration tests for unified multi-platform store catalog and intra-workflow terminal relay.

#[test]
fn daemon_relays_terminal_gpu_subgraph_node_during_workflow_execution() {
    let server = EditionTestDaemon::start(
        DaemonConfig::localhost(0)
            .with_bearer_token("m3-relay-token")
            .with_bounded_request_executor(1, 16),
    );
    let port = server.port;

    let raw_post = |path: &str, payload: &Value| -> (u16, Value) {
        let body = serde_json::to_string(payload).expect("json");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let req = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer m3-relay-token\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(req.as_bytes()).expect("write");
        let mut resp = String::new();
        stream.read_to_string(&mut resp).expect("read");
        let (head, json_str) = resp.split_once("\r\n\r\n").expect("http split");
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        (
            status,
            serde_json::from_str(json_str).unwrap_or(Value::Null),
        )
    };

    let raw_put = |path: &str, payload: &Value| -> u16 {
        let body = serde_json::to_string(payload).expect("json");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let req = format!(
            "PUT {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer m3-relay-token\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(req.as_bytes()).expect("write");
        let mut resp = String::new();
        stream.read_to_string(&mut resp).expect("read");
        resp.split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0)
    };

    // 1. Verify unified multi-platform store catalog endpoint.
    let catalog_raw = raw_http_get(
        port,
        "/v1/store/catalog",
        &format!("Host: 127.0.0.1:{port}\r\nAuthorization: Bearer m3-relay-token\r\n"),
    );
    assert!(catalog_raw.starts_with("HTTP/1.1 200 OK"));
    let catalog: Value =
        serde_json::from_str(catalog_raw.split_once("\r\n\r\n").unwrap().1).expect("catalog json");
    assert!(catalog["supportedPlatforms"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "linux-x64"));

    // 2. Register workflows & tools: step1 (Loom) -> step2 (Otter Terminal Shader) -> step3 (Loom).
    assert_eq!(
        raw_put("/v1/workflows/loom_noop", &json!({ "data": "nodes: []\n" })),
        200
    );
    assert_eq!(
        raw_put(
            "/v1/workflows/hybrid_dag",
            &json!({
                "data": concat!(
                    "nodes:\n",
                    "  - id: loom_step1\n",
                    "    uses: neuro.official/loom-step\n",
                    "  - id: otter_gpu_step2\n",
                    "    uses: neuro.official/otter-shader\n",
                    "    placement: otter_terminal\n",
                    "    needs: [loom_step1]\n",
                    "    with:\n",
                    "      upstream: ${{ nodes.loom_step1.outputs.text }}\n",
                    "  - id: loom_step3\n",
                    "    uses: neuro.official/loom-step\n",
                    "    placement: loom_central\n",
                    "    needs: [otter_gpu_step2]\n"
                )
            })
        ),
        200
    );
    assert_eq!(
        raw_put(
            "/v1/tools/loom-step",
            &json!({
                "id": "loom-step",
                "name": "Loom Step",
                "description": "Central step",
                "enabled": true,
                "execution": { "type": "workflow", "workflowId": "loom_noop" },
                "metadata": {
                    "packageSecurity": { "publisher": { "id": "neuro.official" } }
                }
            })
        ),
        200
    );
    assert_eq!(
        raw_put(
            "/v1/tools/otter-shader",
            &json!({
                "id": "otter-shader",
                "name": "Otter Shader",
                "description": "Terminal GPU step",
                "enabled": true,
                "execution": { "type": "framework_art", "framework": "shader" },
                "metadata": {
                    "packageSecurity": { "publisher": { "id": "neuro.official" } }
                }
            })
        ),
        200
    );
    assert_eq!(
        raw_put(
            "/v1/tools/hybrid-art",
            &json!({
                "id": "hybrid-art",
                "name": "Hybrid Workflow Art",
                "description": "Cross-end workflow",
                "enabled": true,
                "execution": {
                    "type": "workflow",
                    "workflowId": "hybrid_dag",
                    "workflowBindings": {
                        "inputs": [],
                        "primaryOutput": {
                            "nodeId": "otter_gpu_step2",
                            "kind": "node_result",
                            "output": ""
                        }
                    }
                }
            })
        ),
        200
    );

    // 3. Register simulated Otter terminal worker and spawn background poller.
    assert_eq!(
        raw_post(
            "/v1/otter/relay/register",
            &json!({ "deviceId": "device-lan-otter-1" })
        )
        .0,
        200
    );
    let worker = thread::spawn(move || {
        for _ in 0..40 {
            let pending_raw = raw_http_get(
                port,
                "/v1/otter/relay/pending?deviceId=device-lan-otter-1",
                &format!("Host: 127.0.0.1:{port}\r\nAuthorization: Bearer m3-relay-token\r\n"),
            );
            if let Some((_, body)) = pending_raw.split_once("\r\n\r\n") {
                if let Ok(parsed) = serde_json::from_str::<Value>(body) {
                    if let Some(first) = parsed["relays"].as_array().and_then(|arr| arr.first()) {
                        let relay_id = first["relayId"].as_str().unwrap().to_owned();
                        let submit_payload = json!({
                            "relayId": relay_id,
                            "status": "succeeded",
                            "output": {
                                "content": [{ "type": "text", "text": "rendered-on-otter-gpu" }]
                            }
                        });
                        let submit_body = serde_json::to_string(&submit_payload).unwrap();
                        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
                        let req = format!(
                            "POST /v1/otter/relay/results HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer m3-relay-token\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{submit_body}",
                            submit_body.len()
                        );
                        s.write_all(req.as_bytes()).unwrap();
                        let mut out = String::new();
                        let _ = s.read_to_string(&mut out);
                        return;
                    }
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("terminal poller never observed pending relay");
    });

    let (exec_status, exec_json) =
        raw_post("/v1/tools/hybrid-art/execute", &json!({ "arguments": {} }));
    worker.join().expect("join terminal worker");
    assert_eq!(exec_status, 200, "exec failed: {exec_json}");
    assert_eq!(
        exec_json["result"]["content"][0]["text"],
        "rendered-on-otter-gpu"
    );
    let trace = exec_json["result"]["loomMetadata"]["placementTrace"]
        .as_array()
        .expect("placement trace");
    assert_eq!(trace.len(), 3);
    assert_eq!(trace[0]["target"], "loom_central");
    assert_eq!(trace[1]["target"], "otter_terminal");
    assert_eq!(trace[2]["target"], "loom_central");

    server.stop();
}

#[test]
fn inline_http_rejects_terminal_relay_without_blocking_central_execution() {
    let server = EditionTestDaemon::start(
        DaemonConfig::localhost(0).with_bearer_token("inline-relay-token"),
    );
    let request = |method: &str, path: &str, value: Value| {
        let body = value.to_string();
        let mut stream = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer inline-relay-token\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            server.port, body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        let status: u16 = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, serde_json::from_str::<Value>(body).unwrap())
    };
    assert_eq!(
        request(
            "POST",
            "/v1/otter/relay/register",
            json!({"deviceId": "inline-worker"})
        )
        .0,
        200
    );
    for (name, nodes) in [
        ("central", "nodes: []\n"),
        (
            "terminal",
            "nodes:\n  - id: terminal\n    uses: __sticker__\n    placement: otter_terminal\n",
        ),
    ] {
        assert_eq!(
            request(
                "PUT",
                &format!("/v1/workflows/{name}"),
                json!({"data": nodes})
            )
            .0,
            200
        );
        assert_eq!(
            request(
                "PUT",
                &format!("/v1/tools/{name}"),
                json!({
                    "id": name, "name": name, "description": "Inline relay regression", "enabled": true,
                    "execution": {"type": "workflow", "workflowId": name}
                })
            )
            .0,
            200
        );
    }
    let started = Instant::now();
    let (status, body) = request(
        "POST",
        "/v1/tools/terminal/execute",
        json!({"arguments": {}}),
    );
    assert_eq!(status, 500, "{body}");
    assert_eq!(body["error"]["code"], "workflow_runtime_error", "{body}");
    assert!(body.to_string().contains("terminal"), "{body}");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        request(
            "POST",
            "/v1/tools/central/execute",
            json!({"arguments": {}})
        )
        .0,
        200
    );
    server.stop();
}
