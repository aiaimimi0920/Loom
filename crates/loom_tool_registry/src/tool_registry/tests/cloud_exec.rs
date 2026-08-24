//! Cloud execution and response coverage.

use super::*;

#[test]
pub(super) fn execute_cloud_api_tool_posts_json_arguments_to_fixture() {
    let fixture = CloudFixture::start(CloudFixtureMode::Text);
    let mut tool = ToolDefinition::new(
        "fixture-cloud",
        "Fixture Cloud",
        "Call fixture cloud API",
        ToolExecution::CloudApi {
            endpoint: fixture.url("/text"),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    tool.metadata = Some(loopback_cloud_metadata());

    let result = execute_tool(&tool, &[], serde_json::json!({ "prompt": "hello cloud" }))
        .expect("execute cloud API tool");

    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(result["content"][0]["text"], "cloud saw hello cloud");
}

#[test]
pub(super) fn a_cancelled_cloud_run_sends_no_request() {
    let fixture = CloudFixture::start(CloudFixtureMode::Text);
    let mut tool = ToolDefinition::new(
        "fixture-cloud",
        "Fixture Cloud",
        "Call fixture cloud API",
        ToolExecution::CloudApi {
            endpoint: fixture.url("/text"),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    tool.metadata = Some(loopback_cloud_metadata());
    let cancellation = AtomicBool::new(true);

    let error = execute_tool_with_timeout_and_cancellation(
        &tool,
        &[],
        serde_json::json!({ "prompt": "hello cloud" }),
        Duration::from_secs(5),
        &cancellation,
    )
    .expect_err("a cancelled run does not execute");

    assert!(
        matches!(error, ToolRegistryError::ExecutionCancelled { ref id } if id == "fixture-cloud"),
        "unexpected error: {error}"
    );
    assert!(
        fixture
            .captured_request
            .lock()
            .expect("lock cloud request capture")
            .is_none(),
        "the fixture received a request from a cancelled run"
    );
}

#[test]
pub(super) fn a_cloud_run_cancels_while_waiting_for_response_headers() {
    assert_delayed_cloud_run_is_cancellable(CloudFixtureMode::DelayedHeaders);
}

#[test]
pub(super) fn a_cloud_run_cancels_while_waiting_for_response_body() {
    assert_delayed_cloud_run_is_cancellable(CloudFixtureMode::DelayedBody);
}

pub(super) fn assert_delayed_cloud_run_is_cancellable(mode: CloudFixtureMode) {
    let fixture = CloudFixture::start(mode);
    let mut tool = ToolDefinition::new(
        "fixture-cloud-cancel",
        "Fixture Cloud Cancel",
        "Cancel a delayed cloud API request",
        ToolExecution::CloudApi {
            endpoint: fixture.url("/delayed"),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    tool.metadata = Some(loopback_cloud_metadata());
    let cancellation = Arc::new(AtomicBool::new(false));
    let trigger = Arc::clone(&cancellation);
    let trigger_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        trigger.store(true, Ordering::Release);
    });

    let started = Instant::now();
    let error = execute_tool_with_timeout_and_cancellation(
        &tool,
        &[],
        serde_json::json!({ "prompt": "cancel me" }),
        Duration::from_secs(5),
        cancellation.as_ref(),
    )
    .expect_err("the delayed cloud request must be cancelled");
    let elapsed = started.elapsed();

    trigger_thread
        .join()
        .expect("join cloud cancellation trigger");
    assert!(matches!(
        error,
        ToolRegistryError::ExecutionCancelled { ref id } if id == "fixture-cloud-cancel"
    ));
    assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
}

#[test]
pub(super) fn image_response_accumulator_streams_base64_across_chunk_boundaries() {
    let mut raw = fixture_image_bytes().to_vec();
    raw.extend((0_u8..=250).cycle().take(4 * 1024 * 1024 - raw.len()));
    let mut accumulator = CloudBodyAccumulator::new(Some("image/png"), Some(raw.len() as u64));
    for chunk in raw.chunks(65_537) {
        accumulator.push(chunk);
    }

    let CloudResponseBody::ImageDataUrl(data_url) = accumulator
        .finish()
        .expect("finish streamed image response")
    else {
        panic!("image accumulator returned text");
    };
    assert_eq!(
        data_url,
        format!("data:image/png;base64,{}", BASE64.encode(&raw))
    );
}

#[test]
pub(super) fn cloud_binary_images_require_a_supported_mime_and_raster_signature() {
    assert_eq!(
        cloud_image_mime_type("IMAGE/PNG; charset=binary"),
        Some("image/png")
    );
    assert_eq!(cloud_image_mime_type("image/svg+xml"), None);

    let mut spoofed = CloudBodyAccumulator::new(Some("image/png"), None);
    spoofed.push(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    assert!(matches!(
        spoofed.finish(),
        Err(CloudTransportError::InvalidImage)
    ));
}

#[test]
pub(super) fn maximum_text_response_reuses_its_single_byte_allocation() {
    let mut accumulator = CloudBodyAccumulator::new(None, Some(MAX_CLOUD_RESPONSE_BYTES as u64));
    let chunk = [b'x'; 64 * 1024];
    for _ in 0..(MAX_CLOUD_RESPONSE_BYTES / chunk.len()) {
        accumulator.push(&chunk);
    }
    let allocation = match &accumulator {
        CloudBodyAccumulator::Text(bytes) => bytes.as_ptr(),
        CloudBodyAccumulator::Image { .. } => panic!("text accumulator returned image"),
    };

    let CloudResponseBody::Text(text) = accumulator
        .finish()
        .expect("finish maximum valid UTF-8 response")
    else {
        panic!("text accumulator returned image");
    };
    assert_eq!(text.len(), MAX_CLOUD_RESPONSE_BYTES);
    assert_eq!(
        text.as_ptr(),
        allocation,
        "UTF-8 conversion allocated a copy"
    );
}

#[test]
pub(super) fn invalid_utf8_text_is_rejected_without_a_lossy_full_size_copy() {
    let mut accumulator = CloudBodyAccumulator::new(None, Some(3));
    accumulator.push(&[b'a', 0xff, b'b']);

    assert!(matches!(
        accumulator.finish(),
        Err(CloudTransportError::InvalidUtf8)
    ));
}

#[test]
pub(super) fn execute_cloud_api_tool_normalizes_image_json_response() {
    let fixture = CloudFixture::start(CloudFixtureMode::Image);
    let mut tool = ToolDefinition::new(
        "fixture-cloud-image",
        "Fixture Cloud Image",
        "Call fixture cloud image API",
        ToolExecution::CloudApi {
            endpoint: fixture.url("/image"),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    tool.metadata = Some(loopback_cloud_metadata());

    let result = execute_tool(
        &tool,
        &[],
        serde_json::json!({ "input_base64": CLOUD_FIXTURE_IMAGE }),
    )
    .expect("execute cloud image API tool");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
}

#[test]
pub(super) fn execute_cloud_api_tool_reports_http_errors() {
    let fixture = CloudFixture::start(CloudFixtureMode::Error);
    let mut tool = ToolDefinition::new(
        "fixture-cloud-error",
        "Fixture Cloud Error",
        "Call fixture cloud API that fails",
        ToolExecution::CloudApi {
            endpoint: fixture.url("/error"),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );
    tool.metadata = Some(loopback_cloud_metadata());

    let error =
        execute_tool(&tool, &[], serde_json::json!({})).expect_err("cloud API HTTP error fails");

    assert!(error.to_string().contains("cloud API"));
}

#[test]
pub(super) fn a_cloud_art_without_a_declared_network_policy_cannot_call_loopback() {
    let fixture = CloudFixture::start(CloudFixtureMode::Text);
    let tool = ToolDefinition::new(
        "fixture-cloud-undeclared",
        "Fixture Cloud Undeclared",
        "Call a loopback endpoint without declaring it",
        ToolExecution::CloudApi {
            endpoint: fixture.url("/text"),
            method: "POST".to_owned(),
            content_type: None,
            headers: None,
            body: None,
        },
    );

    let error = execute_tool(&tool, &[], serde_json::json!({ "prompt": "hello cloud" }))
        .expect_err("undeclared loopback is refused");

    let message = error.to_string();
    assert!(
        message.contains("loopback") || message.contains("HTTP is only allowed"),
        "unexpected refusal reason: {message}"
    );
}
