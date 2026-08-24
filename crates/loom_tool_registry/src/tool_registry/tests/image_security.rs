//! Image boundary and active-content rejection coverage.

use super::*;

#[test]
pub(super) fn mcp_image_candidates_stop_at_the_nesting_limit() {
    // A candidate that sits within the budget is still found.
    let shallow = serde_json::json!({
        "structuredContent": {"items": [{"url": "https://example.invalid/a.png"}]}
    });
    assert_eq!(collect_mcp_image_candidates(&shallow).len(), 1);

    // Past the budget the walk stops instead of following the value down.
    let mut deep = serde_json::json!({"url": "https://example.invalid/a.png"});
    for _ in 0..(MAX_MCP_IMAGE_CANDIDATE_DEPTH + 4) {
        deep = serde_json::json!({"nested": deep});
    }
    let value = serde_json::json!({"structuredContent": deep});
    assert!(collect_mcp_image_candidates(&value).is_empty());
}

#[test]
pub(super) fn mcp_image_candidates_bound_chained_stringified_payloads() {
    // Every hop is a shallow document on its own, so only a counter that survives the
    // re-parse keeps this from walking as deep as the attacker cares to nest. Reaching this
    // assertion at all is the point: the previous walk aborted the process here.
    //
    // Each hop costs two levels of budget (the object, then the string re-parsed inside it)
    // and roughly doubles the encoded size, so fourteen hops is both comfortably past the
    // limit of MAX_MCP_IMAGE_CANDIDATE_DEPTH and small enough to build in a test.
    fn chained(hops: usize) -> serde_json::Value {
        let mut text = r#"{"url":"https://example.invalid/a.png"}"#.to_owned();
        for _ in 0..hops {
            text = format!(
                r#"{{"items":{}}}"#,
                serde_json::to_string(&text).expect("encode hop")
            );
        }
        serde_json::json!({"content": [{"type": "text", "text": text}]})
    }

    // A short chain is a real payload shape and still resolves.
    assert_eq!(collect_mcp_image_candidates(&chained(2)).len(), 1);
    assert!(collect_mcp_image_candidates(&chained(14)).is_empty());
}

#[test]
pub(super) fn mcp_image_candidates_are_capped() {
    let items = (0..(MAX_MCP_IMAGE_CANDIDATES * 4))
        .map(|index| serde_json::json!({"url": format!("https://example.invalid/{index}.png")}))
        .collect::<Vec<_>>();
    let value = serde_json::json!({"structuredContent": {"items": items}});
    assert_eq!(
        collect_mcp_image_candidates(&value).len(),
        MAX_MCP_IMAGE_CANDIDATES
    );
}

#[test]
pub(super) fn normalize_mcp_image_search_downloads_from_hosts_requiring_image_accept_header() {
    let image_fixture =
        HeaderAwareHttpImageFixture::start("image/png", fixture_image_bytes(), "accept: image/");
    let image_url = image_fixture.url("/guarded.png");
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Guarded fixture image",
                    "url": "https://example.invalid/page",
                    "properties": {
                        "url": image_url,
                        "width": 1,
                        "height": 1
                    }
                }
            ]
        }
    });

    let result =
        normalize_mcp_image_result(&serde_json::json!({}), &value, &loopback_mcp_image_policy())
            .expect("normalize guarded image-search candidate");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
}

#[test]
pub(super) fn normalize_mcp_image_search_strips_broken_cdn_modifiers_from_candidate_urls() {
    let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
    let image_url = image_fixture.url("/image.png");
    let decorated_image_url = format!("{image_url}!/clip/0x300a0a0");
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Modifier fixture image",
                    "url": "https://example.invalid/page",
                    "properties": {
                        "url": decorated_image_url,
                        "width": 1,
                        "height": 1
                    }
                }
            ],
            "count": 1
        }
    });

    let result =
        normalize_mcp_image_result(&serde_json::json!({}), &value, &loopback_mcp_image_policy())
            .expect("normalize image-search url with broken modifiers");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    assert_eq!(
        result["loomMetadata"]["candidates"]["items"][0]["imageUrl"],
        image_url
    );
}

#[test]
pub(super) fn normalize_mcp_image_search_strips_trailing_path_modifiers_after_image_extension() {
    let image_fixture =
        ExactPathHttpImageFixture::start("image/png", fixture_image_bytes(), "/image.png_300.png");
    let image_url = image_fixture.url("/image.png_300.png");
    let decorated_image_url = format!("{image_url}/dpi/0x300a0!");
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Modifier fixture image",
                    "url": "https://example.invalid/page",
                    "properties": {
                        "url": decorated_image_url,
                        "width": 1,
                        "height": 1
                    }
                }
            ],
            "count": 1
        }
    });

    let result =
        normalize_mcp_image_result(&serde_json::json!({}), &value, &loopback_mcp_image_policy())
            .expect("normalize image-search url with trailing path modifiers");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    assert_eq!(
        result["loomMetadata"]["candidates"]["items"][0]["imageUrl"],
        image_url
    );
}

#[test]
pub(super) fn a_rewritten_candidate_url_keeps_the_string_it_came_from() {
    let candidate_from = |url: &str| {
        let value = serde_json::json!({ "url": url });
        image_candidate_from_object(value.as_object().expect("candidate object"))
            .expect("candidate from url")
    };

    let modifier = candidate_from("https://host/a.jpg!600x400");
    assert_eq!(modifier.image_url, "https://host/a.jpg");
    assert_eq!(
        modifier.alternate_image_url.as_deref(),
        Some("https://host/a.jpg!600x400")
    );

    let nested = candidate_from("https://host/logo.png/v2/actual");
    assert_eq!(nested.image_url, "https://host/logo.png");
    assert_eq!(
        nested.alternate_image_url.as_deref(),
        Some("https://host/logo.png/v2/actual")
    );
    // The string a rewritten URL came from is a download fallback, not the page the image sits on.
    assert!(nested.source_page_url.is_none());
}

#[test]
pub(super) fn normalize_mcp_image_search_falls_back_to_the_unstripped_candidate_url() {
    // The rewrite cuts this path at `logo.png`, which the fixture does not serve; only retrying the
    // URL the server actually sent can reach the image.
    let image_fixture = RetryingExactPathHttpImageFixture::start(
        "image/png",
        fixture_image_bytes(),
        "/logo.png/v2/actual",
    );
    let image_url = image_fixture.url("/logo.png/v2/actual");
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Nested path fixture image",
                    "url": "https://example.invalid/page",
                    "properties": {
                        "url": image_url,
                        "width": 1,
                        "height": 1
                    }
                }
            ],
            "count": 1
        }
    });

    let result =
        normalize_mcp_image_result(&serde_json::json!({}), &value, &loopback_mcp_image_policy())
            .expect("normalize image-search url whose rewrite cuts a real path");

    assert_eq!(result["content"][0]["type"], "image");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
}

#[test]
pub(super) fn normalize_mcp_image_search_returns_friendly_message_for_provider_blocked_queries() {
    let mut tool = ToolDefinition::new(
        "fixture-image-search-provider-blocked",
        "Fixture Image Search Provider Blocked",
        "Return a friendly message when the provider flags the query as sensitive",
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

    let result = normalize_mcp_result(
        &tool,
        &serde_json::json!({ "query": "japanese beauty girl" }),
        serde_json::json!({
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
        }),
    );

    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(
        result["content"][0]["text"],
        "图片搜索未返回可用结果：搜索服务将该查询判定为可能敏感，请尝试更换关键词。"
    );
}

#[test]
pub(super) fn normalize_mcp_image_search_returns_friendly_message_for_empty_results() {
    let mut tool = ToolDefinition::new(
        "fixture-image-search-empty-results",
        "Fixture Image Search Empty Results",
        "Return a friendly message when the provider yields no images",
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

    let result = normalize_mcp_result(
        &tool,
        &serde_json::json!({ "query": "no results please" }),
        serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "{\"type\":\"object\",\"items\":[],\"count\":0}"
                }
            ],
            "structuredContent": {
                "type": "object",
                "items": [],
                "count": 0
            }
        }),
    );

    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(
        result["content"][0]["text"],
        "图片搜索未返回可用结果，请尝试更换关键词。"
    );
}

#[cfg(windows)]
#[test]
pub(super) fn powershell_httpclient_fallback_sends_browserish_accept_header() {
    let fixture =
        HeaderAwareHttpImageFixture::start("image/png", fixture_image_bytes(), "accept: image/");

    let (mime_type, bytes) = download_image_bytes_with_powershell_httpclient(
        &fixture.url("/thumb"),
        None,
        &loopback_mcp_image_policy(),
        CLOUD_API_TIMEOUT,
    )
    .expect("download image bytes via powershell fallback with image accept header");

    assert_eq!(mime_type, "image/png");
    assert_eq!(bytes, fixture_image_bytes());
}

#[cfg(windows)]
#[test]
pub(super) fn powershell_httpclient_fallback_downloads_image_candidate_bytes() {
    let fixture = HttpImageFixture::start("application/octet-stream", fixture_image_bytes());
    let (mime_type, bytes) = download_image_bytes_with_powershell_httpclient(
        &fixture.url("/thumb"),
        None,
        &loopback_mcp_image_policy(),
        CLOUD_API_TIMEOUT,
    )
    .expect("download image bytes via powershell fallback");

    assert_eq!(mime_type, "image/png");
    assert_eq!(bytes, fixture_image_bytes());
}

/// A cloud Art no longer reaches loopback unless it declares that it wants to, so every
/// fixture-backed test has to declare it the way a real local-service Art would. The same
/// declaration now governs an MCP image-search tool's image downloads.
pub(super) fn loopback_cloud_metadata() -> serde_json::Value {
    serde_json::json!({
        "permissionPolicy": { "network": { "allowLocalhost": true } }
    })
}

/// The download policy an MCP image-search tool gets once it declares `allowLocalhost`, for the
/// tests that call the download helpers directly against a loopback fixture.
pub(super) fn loopback_mcp_image_policy() -> crate::network_policy::OutboundPolicy {
    crate::network_policy::OutboundPolicy {
        allow_http_loopback: true,
        ..crate::network_policy::OutboundPolicy::default()
    }
}

#[test]
pub(super) fn an_mcp_image_candidate_is_not_downloaded_from_loopback_by_default() {
    let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
    let image_url = image_fixture.url("/fixture.png");
    let value = serde_json::json!({
        "structuredContent": {
            "type": "object",
            "items": [
                {
                    "title": "Loopback image",
                    "url": "https://example.invalid/page",
                    "properties": {
                        "url": image_url,
                        "width": 1,
                        "height": 1
                    }
                }
            ],
            "count": 1
        }
    });

    // The URL is served and would download fine under the old hardcoded loopback allowance.
    // The candidate host is chosen entirely by the MCP server, so an undeclared tool has to be
    // refused before the request goes out.
    assert!(normalize_mcp_image_result(
        &serde_json::json!({}),
        &value,
        &crate::network_policy::OutboundPolicy::default(),
    )
    .is_none());

    let downloaded =
        normalize_mcp_image_result(&serde_json::json!({}), &value, &loopback_mcp_image_policy())
            .expect("download the loopback candidate once loopback is declared");
    assert_eq!(downloaded["content"][0]["type"], "image");
    assert_eq!(downloaded["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
}

#[test]
pub(super) fn an_mcp_image_download_policy_comes_from_the_tool_declaration() {
    let mut tool = ToolDefinition::new(
        "fixture-image-search-policy",
        "Fixture Image Search Policy",
        "Report the derived image download policy",
        ToolExecution::Mcp {
            server_id: "fixture".to_owned(),
            tool_name: "brave_image_search".to_owned(),
        },
    );

    let undeclared = mcp_image_download_policy(&tool);
    assert!(!undeclared.allow_http_loopback);
    assert!(!undeclared.allow_private_networks);

    tool.metadata = Some(serde_json::json!({
        "permissionPolicy": {
            "network": {
                "allowLocalhost": true,
                "allowPrivateNetworks": true,
                "domains": ["api.search.brave.com"]
            }
        }
    }));
    let declared = mcp_image_download_policy(&tool);
    assert!(declared.allow_http_loopback);
    assert!(declared.allow_private_networks);
    // The declared domains name the search API, not the image hosts the results point at, so
    // they deliberately do not constrain the image download.
    assert!(declared.allowed_domains.is_empty());
}
