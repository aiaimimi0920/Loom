//! MCP image execution coverage.

use super::*;

#[test]
pub(super) fn execute_mcp_image_search_tool_downloads_structured_image_result() {
    let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
    let mut tool = ToolDefinition::new(
        "fixture-image-search",
        "Fixture Image Search",
        "Download the first MCP image-search result",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "brave_image_search".to_owned(),
        },
    );
    tool.outputs = vec![serde_json::json!({
        "name": "output",
        "label": "output",
        "type": "image",
        "execution_type": "image_buffer"
    })];
    tool.metadata = Some(loopback_cloud_metadata());
    let server = current_test_binary_fixture_config().env(
        "LOOM_MCP_FIXTURE_IMAGE_URL",
        image_fixture.url("/fixture.png"),
    );

    let result = execute_tool(
        &tool,
        &[server],
        serde_json::json!({ "query": "fixture cat", "count": 1 }),
    )
    .expect("execute MCP image-search tool");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
}

#[test]
pub(super) fn execute_mcp_image_search_tool_honors_result_index_and_preserves_candidates() {
    let first_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
    let second_fixture = HttpImageFixture::start("image/png", fixture_alt_image_bytes());
    let mut tool = ToolDefinition::new(
        "fixture-image-search-multi",
        "Fixture Image Search Multi",
        "Download the selected MCP image-search result",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "brave_image_search".to_owned(),
        },
    );
    tool.outputs = vec![serde_json::json!({
        "name": "output",
        "label": "output",
        "type": "image",
        "execution_type": "image_buffer"
    })];
    tool.metadata = Some(loopback_cloud_metadata());
    let server = current_test_binary_fixture_config()
        .env(
            "LOOM_MCP_FIXTURE_IMAGE_URL",
            first_fixture.url("/fixture-a.png"),
        )
        .env(
            "LOOM_MCP_FIXTURE_IMAGE_URL_ALT",
            second_fixture.url("/fixture-b.png"),
        );

    let result = execute_tool(
        &tool,
        &[server],
        serde_json::json!({ "query": "fixture cat", "count": 2, "result_index": 1 }),
    )
    .expect("execute MCP image-search tool with explicit result index");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE_ALT);
    assert_eq!(result["loomMetadata"]["candidates"]["selectedIndex"], 1);
    assert_eq!(result["loomMetadata"]["candidates"]["items"][0]["index"], 0);
    assert_eq!(result["loomMetadata"]["candidates"]["items"][1]["index"], 1);
}

#[test]
pub(super) fn normalize_mcp_image_search_falls_back_to_another_candidate_when_selected_one_cannot_download(
) {
    let second_fixture = HttpImageFixture::start("image/png", fixture_alt_image_bytes());
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Broken primary image",
                    "url": "https://example.invalid/broken",
                    "properties": {
                        "url": "http://127.0.0.1:9/broken.jpg",
                        "width": 1,
                        "height": 1
                    }
                },
                {
                    "title": "Working fallback image",
                    "url": "https://example.invalid/fallback",
                    "properties": {
                        "url": second_fixture.url("/fixture-b.png"),
                        "width": 1,
                        "height": 1
                    }
                }
            ],
            "count": 2
        }
    });

    let result = normalize_mcp_image_result(
        &serde_json::json!({ "result_index": 0 }),
        &value,
        &loopback_mcp_image_policy(),
    )
    .expect("fallback to another candidate image");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE_ALT);
    assert_eq!(result["loomMetadata"]["candidates"]["selectedIndex"], 1);
    assert_eq!(result["loomMetadata"]["candidates"]["items"][0]["index"], 0);
    assert_eq!(result["loomMetadata"]["candidates"]["items"][1]["index"], 1);
}

#[test]
pub(super) fn normalize_mcp_image_search_retains_candidate_metadata_when_all_downloads_fail() {
    let mut tool = ToolDefinition::new(
        "fixture-image-search-download-failure",
        "Fixture Image Search Download Failure",
        "Return a friendly text message but keep the image-search candidates",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "brave_image_search".to_owned(),
        },
    );
    tool.outputs = vec![serde_json::json!({
        "name": "output",
        "label": "output",
        "type": "image",
        "execution_type": "image_buffer"
    })];
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Broken primary image",
                    "url": "https://example.invalid/broken",
                    "properties": {
                        "url": "http://127.0.0.1:9/broken.jpg",
                        "width": 1,
                        "height": 1
                    }
                }
            ],
            "count": 1
        }
    });

    let result = normalize_mcp_result(&tool, &serde_json::json!({ "result_index": 0 }), value);

    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(
        result["content"][0]["text"],
        "图片搜索已返回候选结果，但图片下载失败，请稍后重试。"
    );
    assert_eq!(result["loomMetadata"]["candidates"]["selectedIndex"], 0);
    assert_eq!(
        result["loomMetadata"]["candidates"]["items"][0]["imageUrl"],
        "http://127.0.0.1:9/broken.jpg"
    );
}

#[test]
pub(super) fn an_in_range_candidate_request_is_reported_without_a_note() {
    let candidates = vec![
        McpImageCandidate {
            image_url: "https://example.com/a.png".to_owned(),
            ..McpImageCandidate::default()
        },
        McpImageCandidate {
            image_url: "https://example.com/b.png".to_owned(),
            ..McpImageCandidate::default()
        },
    ];
    let selection =
        selected_mcp_image_candidate_index(&serde_json::json!({ "result_index": 1 }), 2);
    let mut result = serde_json::json!({ "content": [] });

    attach_mcp_image_candidate_metadata(&mut result, &candidates, &selection, selection.index);

    let metadata = &result["loomMetadata"]["candidates"];
    assert_eq!(metadata["selectedIndex"], 1);
    assert!(metadata.get("requestedIndex").is_none());
    assert!(metadata.get("selectionNote").is_none());
}

#[test]
pub(super) fn an_out_of_range_candidate_request_reports_the_clamp_it_used() {
    let candidates = vec![
        McpImageCandidate {
            image_url: "https://example.com/a.png".to_owned(),
            ..McpImageCandidate::default()
        },
        McpImageCandidate {
            image_url: "https://example.com/b.png".to_owned(),
            ..McpImageCandidate::default()
        },
    ];
    let selection =
        selected_mcp_image_candidate_index(&serde_json::json!({ "result_index": 7 }), 2);
    assert_eq!(selection.requested, Some(7));
    assert_eq!(selection.index, 1);
    let mut result = serde_json::json!({ "content": [] });

    attach_mcp_image_candidate_metadata(&mut result, &candidates, &selection, selection.index);

    let metadata = &result["loomMetadata"]["candidates"];
    assert_eq!(metadata["selectedIndex"], 1);
    assert_eq!(metadata["requestedIndex"], 7);
    assert_eq!(
        metadata["selectionNote"],
        "requested index 7 is past the last of 2 candidates, so candidate 1 was used instead"
    );
}

#[test]
pub(super) fn a_download_fallback_reports_the_candidate_it_could_not_use() {
    // The requested candidate existed but failed to download, so the response carries a different
    // image than the one asked for. Without the note the canvas cannot tell why.
    assert_eq!(
        mcp_image_selection_note(1, 1, 0, 3).expect("a fallback is worth reporting"),
        "candidate 1 could not be downloaded, so candidate 0 was used instead"
    );

    // Both causes at once: past the end of the list *and* the clamped candidate would not download.
    let note = mcp_image_selection_note(7, 2, 0, 3).expect("both causes are worth reporting");
    assert!(note.contains("requested index 7 is past the last of 3 candidates"));
    assert!(note.contains("candidate 2 could not be downloaded"));

    // Nothing moved the choice, so there is nothing to explain.
    assert!(mcp_image_selection_note(1, 1, 1, 3).is_none());
}

#[test]
pub(super) fn normalize_mcp_image_search_falls_back_to_nested_thumbnail_when_primary_image_download_fails(
) {
    let thumbnail_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
    let thumbnail_url = thumbnail_fixture.url("/thumb.png");
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Fixture image",
                    "url": "https://example.invalid/page",
                    "thumbnail": {
                        "src": thumbnail_url,
                        "width": 1,
                        "height": 1
                    },
                    "properties": {
                        "url": "http://127.0.0.1:9/primary.jpg",
                        "width": 1,
                        "height": 1
                    }
                }
            ]
        }
    });

    let result =
        normalize_mcp_image_result(&serde_json::json!({}), &value, &loopback_mcp_image_policy())
            .expect("fallback to thumbnail image");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    assert_eq!(
        result["loomMetadata"]["candidates"]["items"]
            .as_array()
            .expect("candidate metadata")
            .len(),
        1
    );
    assert_eq!(
        result["loomMetadata"]["candidates"]["items"][0]["thumbnailUrl"],
        thumbnail_url
    );
}

#[test]
pub(super) fn normalize_mcp_image_search_accepts_octet_stream_thumbnail_without_extension() {
    let thumbnail_fixture =
        HttpImageFixture::start("application/octet-stream", fixture_image_bytes());
    let thumbnail_url = thumbnail_fixture.url("/thumb");
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Fixture image",
                    "url": "https://example.invalid/page",
                    "thumbnail": {
                        "src": thumbnail_url,
                        "width": 1,
                        "height": 1
                    },
                    "properties": {
                        "url": "http://127.0.0.1:9/primary-nope",
                        "width": 1,
                        "height": 1
                    }
                }
            ]
        }
    });

    let result =
        normalize_mcp_image_result(&serde_json::json!({}), &value, &loopback_mcp_image_policy())
            .expect("fallback to octet-stream thumbnail image");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
}

#[test]
pub(super) fn normalize_mcp_image_search_parses_stringified_items_payloads() {
    let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
    let image_url = image_fixture.url("/image.png");
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": format!(
                r#"[{{"title":"Fixture image","url":"https://example.invalid/page","properties":{{"url":"{image_url}","width":1,"height":1}}}}]"#
            ),
            "count": 1
        }
    });

    let result =
        normalize_mcp_image_result(&serde_json::json!({}), &value, &loopback_mcp_image_policy())
            .expect("normalize stringified image-search items");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    assert_eq!(
        result["loomMetadata"]["candidates"]["items"][0]["imageUrl"],
        image_url
    );
}
