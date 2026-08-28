// Loom daemon tests fragment 18; included into the shared crate test module.
#[test]
fn daemon_hook_bridge_executes_mcp_image_search_art_node_image_output() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("mcp-image-search-art-node");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let image_data = test_png_base64();
    let image_fixture = HttpImageFixture::start(
        "image/png",
        loom_image_io::decode_data_url_bytes(&image_data).expect("decode test image"),
    );
    let fixture = current_test_binary_mcp_fixture_config_with_env(&[(
        "LOOM_DAEMON_MCP_FIXTURE_IMAGE_URL",
        image_fixture.url("/fixture.png"),
    )]);

    let saved_server = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/mcp/servers/fixture",
                &[],
                Some(&fixture.to_string()),
            ),
        ),
        200,
    );
    assert_eq!(saved_server["server"]["id"], "fixture");

    let saved_tool = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "PUT",
                "/v1/tools/fixture-image-search-art",
                &[],
                Some(
                    r#"{
              "id": "fixture-image-search-art",
              "name": "图片搜索",
              "description": "Execute fixture MCP image search through Hook bridge",
              "enabled": true,
              "execution": {
                "type": "mcp",
                "serverId": "fixture",
                "toolName": "brave_image_search"
              },
              "outputs": [
                {
                  "name": "output",
                  "label": "output",
                  "type": "image",
                  "execution_type": "image_buffer"
                }
              ],
              "metadata": {
                "permissionPolicy": { "network": { "allowLocalhost": true } }
              }
            }"#,
                ),
            ),
        ),
        200,
    );
    assert_eq!(saved_tool["tool"]["id"], "fixture-image-search-art");

    let started = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
    let mut socket = connect_hook_bridge_websocket(bridge_port);

    socket
        .send(tungstenite::Message::Text(formal_art_execute_request(
            "mcp-image-search",
            "node-mcp-image-search",
            "fixture-image-search-art",
            None,
            json!({
                "query": "fixture cat",
                "count": 1,
                "safesearch": "off",
                "spellcheck": true
            }),
        )))
        .expect("send MCP image-search execute art node");
    let response = read_hook_terminal_response(&mut socket, "mcp-image-search");

    assert_eq!(response["status"], "succeeded", "response={response}");
    assert_eq!(response["data"]["nodeId"], "node-mcp-image-search");
    assert!(response["data"]["outputs"]["output"]["handle"].is_string());

    drop(socket);
    let stopped = stop_test_hook_bridge(&runtime);
    assert_eq!(stopped["running"], false);

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup mcp image-search art node root");
}

#[test]
fn daemon_shared_image_api_create_list_get_delete_contract() {
    let root = unique_temp_dir("shared-images-api");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let created = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/shared-images",
                &[],
                Some(r#"{"width":1,"height":1,"format":"rgba8","data":[10,20,30,255]}"#),
            ),
        ),
        200,
    );
    let handle = created["image"]["handle"]
        .as_str()
        .expect("created shared image handle")
        .to_owned();

    assert_eq!(created["image"]["size"], 4);
    assert_eq!(created["image"]["width"], 1);
    assert_eq!(created["image"]["height"], 1);
    assert_eq!(created["image"]["format"], "rgba8");

    let listed = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/shared-images", &[], None),
        ),
        200,
    );
    assert_eq!(listed["images"].as_array().expect("images").len(), 1);
    assert_eq!(listed["images"][0]["handle"], handle);

    let fetched = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", &format!("/v1/shared-images/{handle}"), &[], None),
        ),
        200,
    );
    assert_eq!(fetched["image"]["handle"], handle);
    assert_eq!(fetched["data"], serde_json::json!([10, 20, 30, 255]));
    assert!(fetched["dataBase64"]
        .as_str()
        .expect("png data URL")
        .starts_with("data:image/png;base64,"));

    let deleted = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("DELETE", &format!("/v1/shared-images/{handle}"), &[], None),
        ),
        200,
    );
    assert_eq!(deleted["deleted"], true);
    let listed = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request("GET", "/v1/shared-images", &[], None),
        ),
        200,
    );
    assert!(listed["images"].as_array().expect("images").is_empty());

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup shared images root");
}

#[test]
fn daemon_image_helper_converts_base64_to_rgba_buffer() {
    let root = unique_temp_dir("image-helper-base64");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let response = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/image-helpers/convert",
                &[],
                Some(
                    &serde_json::json!({
                        "sourceType": "image_base64",
                        "targetType": "image_buffer",
                        "data": test_png_base64()
                    })
                    .to_string(),
                ),
            ),
        ),
        200,
    );

    assert_eq!(response["image"]["width"], 1);
    assert_eq!(response["image"]["height"], 1);
    assert_eq!(response["image"]["format"], "rgba8");
    assert_eq!(response["image"]["size"], 4);
    assert_eq!(response["data"], serde_json::json!([10, 20, 30, 255]));

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup image helper base64 root");
}

#[test]
fn daemon_image_helper_converts_path_to_base64() {
    let root = unique_temp_dir("image-helper-path");
    let image_path = root.join("pixel.png");
    let data_url = test_png_base64();
    fs::write(
        &image_path,
        BASE64
            .decode(data_url.split_once(',').expect("data URL").1)
            .expect("decode test png"),
    )
    .expect("write image fixture");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let response = expect_json_text_route_response(
        route_request(
            &runtime,
            &parsed_request(
                "POST",
                "/v1/image-helpers/convert",
                &[],
                Some(
                    &serde_json::json!({
                        "sourceType": "image_path",
                        "targetType": "image_base64",
                        "path": image_path.display().to_string()
                    })
                    .to_string(),
                ),
            ),
        ),
        200,
    );

    assert!(response["dataBase64"]
        .as_str()
        .expect("data URL")
        .starts_with("data:image/png;base64,"));
    let rgba = loom_image_io::decode_image_base64_to_rgba8(
        response["dataBase64"].as_str().expect("data URL"),
    )
    .expect("decode converted path");
    assert_eq!(rgba.data, vec![10, 20, 30, 255]);

    drop(runtime);
    fs::remove_dir_all(root).expect("cleanup image helper root");
}

#[test]
fn daemon_hook_bridge_ocr_image_fixture_provider_returns_success() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_fixture = std::env::var("LOOM_OCR_FIXTURE_TEXT").ok();
    std::env::set_var("LOOM_OCR_FIXTURE_TEXT", "hello loom ocr");
    let root = unique_temp_dir("ocr-fixture");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));

    let capabilities = run_hook_bridge_text(
        &runtime,
        &serde_json::json!({
            "method": loom_protocol::HOOK_METHOD_ENHANCEMENTS_GET,
            "params": { "requestId": "fixture-enhancements" }
        })
        .to_string(),
    );
    assert_eq!(capabilities["status"], "succeeded");
    assert_eq!(capabilities["data"]["ocr"], true);

    let response = run_hook_bridge_text(
        &runtime,
        &serde_json::json!({
            "method": loom_protocol::HOOK_METHOD_OCR_EXECUTE,
            "params": {
                "requestId": "fixture-ocr",
                "imageBase64": test_png_base64()
            }
        })
        .to_string(),
    );

    assert_eq!(response["status"], "succeeded", "response={response}");
    assert_eq!(response["data"]["fullText"], "hello loom ocr");
    assert_eq!(response["data"]["textBlocks"], serde_json::json!([]));
    assert_eq!(response["data"]["scaleFactor"], 1.0);
    assert_eq!(response["data"]["width"], 1);
    assert_eq!(response["data"]["height"], 1);

    drop(runtime);
    restore_env("LOOM_OCR_FIXTURE_TEXT", previous_fixture);
    fs::remove_dir_all(root).expect("cleanup ocr fixture root");
}

#[test]
fn daemon_hook_bridge_ocr_image_serializes_nested_layout_evidence() {
    let result = loom_ocr::OcrDetectResult {
        text_blocks: vec![loom_ocr::EnhancedTextBlock {
            box_points: vec![
                loom_ocr::OcrPoint { x: 10, y: 20 },
                loom_ocr::OcrPoint { x: 90, y: 20 },
                loom_ocr::OcrPoint { x: 90, y: 50 },
                loom_ocr::OcrPoint { x: 10, y: 50 },
            ],
            box_score: 0.99,
            text: "Alt+2".to_owned(),
            text_score: 0.98,
            color_hex: "#ffffff".to_owned(),
            bg_color_hex: "#101010".to_owned(),
            raw_text: Some("A1t+2".to_owned()),
            line_geometry: Some(loom_ocr::OcrLineGeometry {
                baseline: [
                    loom_ocr::OcrMetricPoint { x: 10.0, y: 44.6 },
                    loom_ocr::OcrMetricPoint { x: 90.0, y: 44.6 },
                ],
                angle_degrees: 0.0,
                source: loom_ocr::OcrGeometrySource::EstimatedFromRapidOcrLineQuad,
            }),
            character_spans: vec![loom_ocr::OcrTextSpan {
                text: "A".to_owned(),
                box_points: [
                    loom_ocr::OcrMetricPoint { x: 10.0, y: 20.0 },
                    loom_ocr::OcrMetricPoint { x: 24.0, y: 20.0 },
                    loom_ocr::OcrMetricPoint { x: 24.0, y: 50.0 },
                    loom_ocr::OcrMetricPoint { x: 10.0, y: 50.0 },
                ],
                score: 0.97,
                source: loom_ocr::OcrTextSpanSource::CtcAlignedFromRecognitionTimesteps,
            }],
            word_spans: vec![loom_ocr::OcrTextSpan {
                text: "A1t".to_owned(),
                box_points: [
                    loom_ocr::OcrMetricPoint { x: 10.0, y: 20.0 },
                    loom_ocr::OcrMetricPoint { x: 50.0, y: 20.0 },
                    loom_ocr::OcrMetricPoint { x: 50.0, y: 50.0 },
                    loom_ocr::OcrMetricPoint { x: 10.0, y: 50.0 },
                ],
                score: 0.96,
                source: loom_ocr::OcrTextSpanSource::CtcAlignedFromRecognitionTimesteps,
            }],
        }],
        scale_factor: 1.0,
        full_text: "Alt+2".to_owned(),
        width: 100,
        height: 60,
    };

    let serialized = serde_json::to_value(result).expect("serialize OCR result");
    let block = &serialized["textBlocks"][0];
    assert_eq!(block["rawText"], "A1t+2");
    assert_eq!(block["text"], "Alt+2");
    assert_eq!(block["lineGeometry"]["source"], "estimatedFromRapidOcrLineQuad");
    assert_eq!(block["lineGeometry"]["baseline"][1]["x"], 90.0);
    assert_eq!(
        block["characterSpans"][0]["source"],
        "ctcAlignedFromRecognitionTimesteps"
    );
    assert_eq!(block["characterSpans"][0]["boxPoints"][1]["x"], 24.0);
    assert_eq!(block["wordSpans"][0]["text"], "A1t");
    assert!(block.get("raw_text").is_none());
    assert!(block.get("line_geometry").is_none());
}

#[test]
fn daemon_hook_bridge_translate_text_uses_configured_provider() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let fixture = TranslateFixture::start();
    let previous_endpoint = std::env::var("LOOM_TRANSLATE_ENDPOINT").ok();
    std::env::set_var("LOOM_TRANSLATE_ENDPOINT", fixture.url("/translate"));
    let root = unique_temp_dir("translate-provider");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let response = run_hook_bridge_text(
        &runtime,
        &serde_json::json!({
            "method": loom_protocol::HOOK_METHOD_TRANSLATION_EXECUTE,
            "params": {
                "requestId": "fixture-translation",
                "text": "hello loom",
                "targetLanguage": "zh"
            }
        })
        .to_string(),
    );

    assert_eq!(response["status"], "succeeded", "response={response}");
    assert_eq!(
        response["data"]["translatedText"],
        "translated:hello loom:zh"
    );
    let request = fixture.request();
    assert!(request.starts_with("POST /translate HTTP/1.1"));
    assert!(request.contains(r#""text":"hello loom""#));
    assert!(request.contains(r#""target_lang":"zh""#));

    drop(runtime);
    restore_env("LOOM_TRANSLATE_ENDPOINT", previous_endpoint);
    fs::remove_dir_all(root).expect("cleanup translate provider root");
}

#[test]
#[cfg_attr(
    not(windows),
    ignore = "packaged OCR validation requires the bundled Windows ONNX Runtime"
)]
fn daemon_hook_bridge_ocr_image_real_provider_returns_success() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let previous_fixture = std::env::var("LOOM_OCR_FIXTURE_TEXT").ok();
    let previous_model_dir = std::env::var("LOOM_OCR_MODEL_DIR").ok();
    std::env::remove_var("LOOM_OCR_FIXTURE_TEXT");
    std::env::set_var("LOOM_OCR_MODEL_DIR", workspace_ocr_resources());
    let root = unique_temp_dir("ocr-real");
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_control_plane_root(&root))
        .expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let started = http_json_post(address.port(), "/v1/hook-bridge/start", r#"{"port":0}"#);
    let bridge_port = started["port"].as_u64().expect("bridge port") as u16;
    let mut socket = connect_hook_bridge_websocket(bridge_port);

    socket
        .send(tungstenite::Message::Text(
            serde_json::json!({
                "method": loom_protocol::HOOK_METHOD_ENHANCEMENTS_GET,
                "params": { "requestId": "real-enhancements" }
            })
            .to_string(),
        ))
        .expect("send capabilities request");
    let capabilities = read_hook_bridge_json(&mut socket);
    assert_eq!(capabilities["data"]["ocr"], true);

    let image_data = packaged_ocr_fixture_base64();
    socket
        .send(tungstenite::Message::Text(
            serde_json::json!({
                "method": loom_protocol::HOOK_METHOD_OCR_EXECUTE,
                "params": {
                    "requestId": "real-ocr",
                    "imageBase64": image_data
                }
            })
            .to_string(),
        ))
        .expect("send ocr request");
    let response = read_hook_terminal_response(&mut socket, "real-ocr");

    assert_eq!(response["status"], "succeeded", "response={response}");
    assert!(
        !response["data"]["fullText"]
            .as_str()
            .expect("fullText")
            .trim()
            .is_empty(),
        "response={response}"
    );
    assert!(
        !response["data"]["textBlocks"]
            .as_array()
            .expect("textBlocks")
            .is_empty(),
        "response={response}"
    );
    let first_block = &response["data"]["textBlocks"][0];
    assert_eq!(
        first_block["lineGeometry"]["source"],
        "estimatedFromRapidOcrLineQuad",
        "response={response}"
    );
    assert_eq!(
        first_block["lineGeometry"]["baseline"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "response={response}"
    );
    assert!(
        first_block["characterSpans"]
            .as_array()
            .is_some_and(|spans| !spans.is_empty()),
        "response={response}"
    );
    assert!(
        first_block["wordSpans"]
            .as_array()
            .is_some_and(|spans| !spans.is_empty()),
        "response={response}"
    );
    assert_eq!(response["data"]["width"], 678);
    assert_eq!(response["data"]["height"], 108);

    drop(socket);
    let stopped = http_json_post(address.port(), "/v1/hook-bridge/stop", "{}");
    assert_eq!(stopped["running"], false);

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
    restore_env("LOOM_OCR_MODEL_DIR", previous_model_dir);
    restore_env("LOOM_OCR_FIXTURE_TEXT", previous_fixture);
    remove_test_dir(&root);
}
