//! MCP image result normalization and candidate state.

use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct McpImageCandidate {
    pub(super) image_url: String,
    /// The server's own string for `image_url`, kept when normalization rewrote it.
    ///
    /// The rewrite drops a CDN modifier off the end of the URL, which is often the only thing making
    /// the URL fetchable. But it cuts at the last image extension in the path, so for a path like
    /// `/logo.png/v2/actual` it cuts away the real file and leaves a URL for a different image. The
    /// string it was derived from is kept here so the download can fall back to it rather than give up
    /// on an address nobody ever sent.
    pub(super) alternate_image_url: Option<String>,
    pub(super) title: Option<String>,
    pub(super) thumbnail_url: Option<String>,
    pub(super) source_page_url: Option<String>,
    pub(super) width: Option<u64>,
    pub(super) height: Option<u64>,
}

pub(super) fn normalize_mcp_result(
    tool: &ToolDefinition,
    arguments: &serde_json::Value,
    value: serde_json::Value,
) -> serde_json::Value {
    if mcp_result_already_contains_image(&value) {
        return value;
    }
    if tool_expects_image_output(tool) {
        let download_policy = mcp_image_download_policy(tool);
        if let Some(image) = normalize_mcp_image_result(arguments, &value, &download_policy) {
            return image;
        }
        if let Some(message) = friendly_mcp_image_result_message(&value) {
            let candidates = collect_mcp_image_candidates(&value);
            if !candidates.is_empty() {
                let selection = selected_mcp_image_candidate_index(arguments, candidates.len());
                let mut response = text_content_response(&message);
                attach_mcp_image_candidate_metadata(
                    &mut response,
                    &candidates,
                    &selection,
                    selection.index,
                );
                return response;
            }
            return text_content_response(&message);
        }
    }
    value
}

pub(super) fn mcp_result_already_contains_image(value: &serde_json::Value) -> bool {
    value
        .get("content")
        .or_else(|| value.get("result").and_then(|result| result.get("content")))
        .and_then(serde_json::Value::as_array)
        .map(|content| {
            content.iter().any(|item| {
                let item_type = item
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                match item_type {
                    "image" => item
                        .get("data")
                        .and_then(serde_json::Value::as_str)
                        .is_some(),
                    "text" => item
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .map(|text| {
                            let trimmed = text.trim();
                            trimmed.starts_with("data:image/") || looks_like_base64_payload(trimmed)
                        })
                        .unwrap_or(false),
                    _ => false,
                }
            })
        })
        .unwrap_or(false)
}

pub(super) fn tool_expects_image_output(tool: &ToolDefinition) -> bool {
    tool.outputs.iter().any(value_declares_image_output)
}

pub(super) fn value_declares_image_output(value: &serde_json::Value) -> bool {
    let output_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if output_type == "image" {
        return true;
    }
    let execution_type = value
        .get("execution_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(execution_type.as_str(), "image_buffer" | "image_path")
}

pub(super) fn normalize_mcp_image_result(
    arguments: &serde_json::Value,
    value: &serde_json::Value,
    policy: &crate::network_policy::OutboundPolicy,
) -> Option<serde_json::Value> {
    let candidates = collect_mcp_image_candidates(value);
    if candidates.is_empty() {
        return None;
    }
    let selection = selected_mcp_image_candidate_index(arguments, candidates.len());
    let (mut normalized, delivered_index) = image_response_from_mcp_candidates(
        &candidates,
        selection.index,
        policy,
        McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET),
    )?;
    attach_mcp_image_candidate_metadata(&mut normalized, &candidates, &selection, delivered_index);
    Some(normalized)
}

pub(super) fn friendly_mcp_image_result_message(value: &serde_json::Value) -> Option<String> {
    if let Some(message) = mcp_image_search_empty_result_message(value) {
        return Some(message);
    }
    let candidates = collect_mcp_image_candidates(value);
    if !candidates.is_empty() {
        return Some("图片搜索已返回候选结果，但图片下载失败，请稍后重试。".to_owned());
    }
    None
}

pub(super) fn mcp_image_search_empty_result_message(value: &serde_json::Value) -> Option<String> {
    if let Some(message) =
        mcp_image_search_empty_result_message_from_payload(value.get("structuredContent"))
    {
        return Some(message);
    }
    if let Some(message) = mcp_image_search_empty_result_message_from_payload(
        value
            .get("result")
            .and_then(|result| result.get("structuredContent")),
    ) {
        return Some(message);
    }
    if let Some(content) = value
        .get("content")
        .or_else(|| value.get("result").and_then(|result| result.get("content")))
        .and_then(serde_json::Value::as_array)
    {
        for item in content {
            let Some(text) = item.get("text").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) else {
                continue;
            };
            if let Some(message) = mcp_image_search_empty_result_message_from_payload(Some(&parsed))
            {
                return Some(message);
            }
        }
    }
    None
}

pub(super) fn mcp_image_search_empty_result_message_from_payload(
    payload: Option<&serde_json::Value>,
) -> Option<String> {
    let payload = payload?;
    let items_len = mcp_image_search_items_len(payload);
    let count = payload
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            payload
                .get("count")
                .and_then(serde_json::Value::as_str)
                .and_then(|raw| raw.parse::<u64>().ok())
        });
    let has_no_items = matches!(items_len, Some(0)) || matches!(count, Some(0));
    if !has_no_items {
        return None;
    }
    let provider_flagged_sensitive = payload
        .get("might_be_offensive")
        .or_else(|| payload.get("mightBeOffensive"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if provider_flagged_sensitive {
        return Some(
            "图片搜索未返回可用结果：搜索服务将该查询判定为可能敏感，请尝试更换关键词。".to_owned(),
        );
    }
    Some("图片搜索未返回可用结果，请尝试更换关键词。".to_owned())
}

pub(super) fn mcp_image_search_items_len(value: &serde_json::Value) -> Option<usize> {
    match value.get("items") {
        Some(serde_json::Value::Array(items)) => Some(items.len()),
        Some(serde_json::Value::String(raw)) => serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|parsed| parsed.as_array().map(Vec::len)),
        _ => None,
    }
}

/// Wall-clock budget for the whole download loop over one MCP tool result's image candidates.
///
/// A candidate does not only fail fast. One candidate expands into the image URL and then the
/// thumbnail, each of those into the URL as given and then the modifier-stripped form, and each of
/// those into a reqwest attempt and then the PowerShell fallback — every attempt bounded only by
/// [`CLOUD_API_TIMEOUT`]. A result whose candidates all point at a host that accepts the connection
/// and then never answers therefore used to hold one tool call for minutes per candidate, and a
/// result carrying the full [`MAX_MCP_IMAGE_CANDIDATES`] for about an hour. The loop now runs
/// against one deadline and every network attempt is bounded by whatever is left of it.
pub(super) const MCP_IMAGE_DOWNLOAD_BUDGET: Duration = Duration::from_secs(90);

/// Ceiling on how many candidates one call tries to download before giving up.
///
/// The candidate list is as long as the MCP server chose to make it. Retrying dozens of them is not
/// how a usable image search behaves: if the first handful cannot be fetched, reporting that back is
/// better than spending the whole budget.
pub(super) const MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS: usize = 6;

/// The least remaining budget worth spending on one more network attempt.
pub(super) const MIN_MCP_IMAGE_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);

/// Remaining wall-clock budget for one MCP image download loop.
#[derive(Clone, Copy, Debug)]
pub(super) struct McpImageDownloadDeadline {
    deadline: Instant,
}

impl McpImageDownloadDeadline {
    pub(super) fn starting_now(budget: Duration) -> Self {
        Self {
            deadline: Instant::now()
                .checked_add(budget)
                .unwrap_or_else(Instant::now),
        }
    }

    /// The timeout for one more network attempt, or `None` when too little of the budget is left for
    /// another request to be worth starting.
    pub(super) fn next_attempt_timeout(&self) -> Option<Duration> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        (remaining >= MIN_MCP_IMAGE_ATTEMPT_TIMEOUT).then(|| remaining.min(CLOUD_API_TIMEOUT))
    }
}
