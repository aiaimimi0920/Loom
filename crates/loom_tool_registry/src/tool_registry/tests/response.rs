//! Response normalization and error text coverage.

use super::*;

#[test]
pub(super) fn data_url_candidate_is_decoded_and_identified_from_its_bytes() {
    let response = image_response_from_image_data_url(CLOUD_FIXTURE_IMAGE)
        .expect("data URL candidate resolves");

    assert_eq!(response["content"][0]["type"], "image");
    assert_eq!(response["content"][0]["mimeType"], "image/png");
    assert_eq!(
        response["content"][0]["data"],
        format!(
            "data:image/png;base64,{}",
            BASE64.encode(fixture_image_bytes())
        )
    );
}

#[test]
pub(super) fn data_url_candidate_mime_type_comes_from_the_bytes_not_the_url() {
    let mislabelled = format!(
        "data:image/webp;base64,{}",
        BASE64.encode(fixture_image_bytes())
    );

    let response =
        image_response_from_image_data_url(&mislabelled).expect("mislabelled data URL resolves");

    assert_eq!(response["content"][0]["mimeType"], "image/png");
}

#[test]
pub(super) fn malformed_or_non_raster_data_url_candidates_are_rejected() {
    let svg = format!(
        "data:image/svg+xml;base64,{}",
        BASE64.encode(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>")
    );

    for value in [
        "data:image/png;base64,not valid base64",
        "data:image/png;base64,",
        "data:image/png,%3Csvg%3E",
        svg.as_str(),
    ] {
        assert!(
            image_response_from_image_data_url(value).is_none(),
            "`{value}` should not resolve as an image"
        );
    }
}

#[test]
pub(super) fn svg_urls_and_mime_types_are_not_accepted_as_images() {
    assert!(!looks_like_image_url("https://host/logo.svg"));
    assert!(looks_like_image_url("https://host/logo.png"));
    assert!(infer_image_mime_type_from_url("https://host/logo.svg").is_none());
    assert!(!is_supported_image_mime_type("image/svg+xml"));
    assert!(is_supported_image_mime_type("IMAGE/PNG"));
}

#[test]
pub(super) fn short_borrowed_error_text_is_kept_whole() {
    assert_eq!(
        bounded_error_text("  quota exceeded for this key  "),
        "quota exceeded for this key"
    );
    assert_eq!(bounded_error_text(""), "");
    let exact = "e".repeat(MAX_BORROWED_ERROR_TEXT_BYTES);
    assert_eq!(bounded_error_text(&exact), exact);
}

#[test]
pub(super) fn long_borrowed_error_text_keeps_its_head_and_says_what_it_dropped() {
    let body = format!("failure: {}", "x".repeat(64 * 1024));

    let bounded = bounded_error_text(&body);

    assert!(bounded.starts_with("failure: xxx"));
    assert!(bounded.contains(&format!(
        "[{} more bytes omitted]",
        body.len() - MAX_BORROWED_ERROR_TEXT_BYTES
    )));
    assert!(bounded.len() < body.len());
}

#[test]
pub(super) fn borrowed_error_text_is_cut_on_a_character_boundary() {
    // A multi-byte character straddling the bound would panic a naive slice, and the text that
    // reaches here — an API error message, a runtime's stderr — is regularly not ASCII.
    let body = "配额已用尽".repeat(4096);

    let bounded = bounded_error_text(&body);

    assert!(bounded.starts_with("配额已用尽"));
    assert!(bounded.contains("more bytes omitted"));
    assert!(bounded.len() <= MAX_BORROWED_ERROR_TEXT_BYTES + 64);
}

#[test]
pub(super) fn a_failed_call_after_a_successful_listing_reports_only_itself() {
    let error = mcp_call_error(
        loom_mcp::McpError::Protocol("tool rejected input".into()),
        None,
    );

    let message = error.to_string();
    assert!(message.contains("tool rejected input"));
    assert!(!message.contains("tool listing failed"));
}

#[test]
pub(super) fn a_direct_mcp_call_failure_stays_bounded() {
    let error = mcp_call_error(
        loom_mcp::McpError::Protocol(format!("failure: {}", "x".repeat(64 * 1024))),
        None,
    );

    let message = error.to_string();
    assert!(message.contains("MCP protocol error: failure:"));
    assert!(message.contains("more bytes omitted"));
    assert!(message.len() < 4 * 1024);
}

#[test]
pub(super) fn a_failed_call_after_a_failed_listing_reports_both() {
    let error = mcp_call_error(
        loom_mcp::McpError::Protocol("unknown argument `query`".into()),
        Some("MCP request timed out after 5000ms; stderr: "),
    );

    let message = error.to_string();
    assert!(message.contains("unknown argument `query`"));
    assert!(message.contains("tool listing failed first"));
    assert!(message.contains("timed out after 5000ms"));
}

#[test]
pub(super) fn a_folded_listing_failure_stays_bounded() {
    let error = mcp_call_error(
        loom_mcp::McpError::Protocol("x".repeat(64 * 1024)),
        Some(&"y".repeat(64 * 1024)),
    );

    let message = error.to_string();
    assert!(message.contains("more bytes omitted"));
    assert!(message.len() < 8 * 1024);
}

#[test]
pub(super) fn cloud_json_data_string_stays_text_without_an_image_signal() {
    let response = normalize_cloud_json_value(serde_json::json!({ "data": "completed" }));

    assert_eq!(response["content"][0]["type"], "text");
    assert!(response["content"][0]["text"]
        .as_str()
        .expect("text content")
        .contains("completed"));
}

#[test]
pub(super) fn cloud_json_nested_output_data_stays_text_without_an_image_signal() {
    let response = normalize_cloud_json_value(
        serde_json::json!({ "output": { "data": "req_01HX9ZQK7T2M4V8N" } }),
    );

    assert_eq!(response["content"][0]["type"], "text");
}

#[test]
pub(super) fn cloud_json_image_label_is_confirmed_from_the_payload_bytes() {
    let data = BASE64.encode(fixture_image_bytes());
    let response =
        normalize_cloud_json_value(serde_json::json!({ "data": data, "mime_type": "image/jpeg" }));

    assert_eq!(response["content"][0]["type"], "image");
    assert_eq!(response["content"][0]["mimeType"], "image/png");
    assert_eq!(
        response["content"][0]["data"],
        format!("data:image/png;base64,{data}")
    );
}

#[test]
pub(super) fn cloud_json_data_url_is_an_image_whatever_its_length() {
    let response = normalize_cloud_json_value(serde_json::json!({ "data": CLOUD_FIXTURE_IMAGE }));

    assert_eq!(response["content"][0]["type"], "image");
    assert_eq!(response["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
}

#[test]
pub(super) fn cloud_json_rejects_svg_and_spoofed_raster_labels() {
    let svg_bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
    let svg_base64 = BASE64.encode(svg_bytes);
    let svg_data_url = format!("data:image/svg+xml;base64,{svg_base64}");

    for value in [
        serde_json::json!({ "data": svg_data_url }),
        serde_json::json!({ "data": svg_base64, "mime_type": "image/svg+xml" }),
        serde_json::json!({ "data": svg_base64, "mime_type": "image/png" }),
    ] {
        let response = normalize_cloud_json_value(value);
        assert_eq!(response["content"][0]["type"], "text");
    }
}

#[test]
pub(super) fn mcp_text_result_is_not_an_image_just_because_it_is_alphanumeric() {
    let value = serde_json::json!({
        "content": [{ "type": "text", "text": "completed" }]
    });

    assert!(!mcp_result_already_contains_image(&value));
}
