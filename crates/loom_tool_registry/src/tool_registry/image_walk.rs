//! Bounded traversal of image candidates in MCP results.

use super::*;

pub(super) fn image_response_from_mcp_candidates(
    candidates: &[McpImageCandidate],
    requested_index: usize,
    policy: &crate::network_policy::OutboundPolicy,
    deadline: McpImageDownloadDeadline,
) -> Option<(serde_json::Value, usize)> {
    if candidates.is_empty() {
        return None;
    }
    let ordered = std::iter::once(requested_index).chain(
        candidates
            .iter()
            .enumerate()
            .map(|(index, _)| index)
            .filter(|index| *index != requested_index),
    );
    for candidate_index in ordered.take(MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS) {
        if deadline.next_attempt_timeout().is_none() {
            break;
        }
        let candidate = candidates.get(candidate_index)?;
        if let Some(response) = image_response_from_mcp_candidate(candidate, policy, deadline) {
            return Some((response, candidate_index));
        }
    }
    None
}

/// Nesting limit for the walk over an MCP tool result.
///
/// The walk needs a counter of its own because a string inside the result that begins with `{`
/// or `[` is parsed again as JSON, and each parse starts serde's nesting budget over while the
/// walk is already that many frames into the native stack. A result that is individually
/// shallow at every hop therefore used to be able to drive the walk arbitrarily deep and abort
/// the process on a stack overflow. Tool results come from servers Loom does not control, so
/// the limit is generous enough for real image-search payloads and nothing more.
pub(super) const MAX_MCP_IMAGE_CANDIDATE_DEPTH: usize = 24;

/// Ceiling on how many image candidates one MCP tool result may contribute.
///
/// Without it a flat array of a million URLs is copied into the response metadata and sent to
/// the client, which is a much cheaper attack than nesting.
pub(super) const MAX_MCP_IMAGE_CANDIDATES: usize = 64;

/// Accumulator for the image-candidate walk, holding the results found so far and the dedup set.
#[derive(Default)]
pub(super) struct McpImageCandidateWalk {
    candidates: Vec<McpImageCandidate>,
    seen: std::collections::BTreeSet<String>,
}

impl McpImageCandidateWalk {
    fn is_full(&self) -> bool {
        self.candidates.len() >= MAX_MCP_IMAGE_CANDIDATES
    }

    fn push(&mut self, candidate: McpImageCandidate) {
        if self.seen.insert(candidate.image_url.clone()) {
            self.candidates.push(candidate);
        }
    }
}

pub(super) fn collect_mcp_image_candidates(value: &serde_json::Value) -> Vec<McpImageCandidate> {
    let mut walk = McpImageCandidateWalk::default();
    if let Some(structured_content) = value.get("structuredContent") {
        collect_mcp_image_candidates_from_value(structured_content, 0, &mut walk);
    }
    if let Some(structured_content) = value
        .get("result")
        .and_then(|result| result.get("structuredContent"))
    {
        collect_mcp_image_candidates_from_value(structured_content, 0, &mut walk);
    }
    if walk.candidates.is_empty() {
        if let Some(content) = value
            .get("content")
            .or_else(|| value.get("result").and_then(|result| result.get("content")))
            .and_then(serde_json::Value::as_array)
        {
            for item in content {
                if walk.is_full() {
                    break;
                }
                let Some(text) = item.get("text").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let Some(parsed) = parse_mcp_image_candidate_json(text, 0) else {
                    continue;
                };
                collect_mcp_image_candidates_from_value(&parsed, 0, &mut walk);
            }
        }
    }
    walk.candidates
}

pub(super) fn collect_mcp_image_candidates_from_value(
    value: &serde_json::Value,
    depth: usize,
    walk: &mut McpImageCandidateWalk,
) {
    if depth > MAX_MCP_IMAGE_CANDIDATE_DEPTH || walk.is_full() {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            if let Some(candidate) = image_candidate_from_object(map) {
                walk.push(candidate);
                return;
            }
            for child in map.values() {
                collect_mcp_image_candidates_from_value(child, depth + 1, walk);
                if walk.is_full() {
                    return;
                }
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_mcp_image_candidates_from_value(child, depth + 1, walk);
                if walk.is_full() {
                    return;
                }
            }
        }
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if (looks_like_image_url(trimmed) || trimmed.starts_with("data:image/"))
                && walk.seen.insert(trimmed.to_owned())
            {
                walk.candidates.push(McpImageCandidate {
                    image_url: trimmed.to_owned(),
                    alternate_image_url: None,
                    title: None,
                    thumbnail_url: None,
                    source_page_url: None,
                    width: None,
                    height: None,
                });
                return;
            }
            if matches!(trimmed.as_bytes().first(), Some(b'{') | Some(b'[')) {
                if let Some(parsed) = parse_mcp_image_candidate_json(trimmed, depth) {
                    collect_mcp_image_candidates_from_value(&parsed, depth + 1, walk);
                }
            }
        }
        _ => {}
    }
}

/// Parse a string inside an MCP tool result that itself looks like a JSON document.
///
/// The nesting budget handed to the parser is what is *left* of the walk's budget rather than a
/// fresh one, which is the whole point: the parse happens `depth` frames into the walk, so
/// letting each hop spend the full budget again is what made the recursion unbounded.
pub(super) fn parse_mcp_image_candidate_json(
    text: &str,
    depth: usize,
) -> Option<serde_json::Value> {
    let remaining = MAX_MCP_IMAGE_CANDIDATE_DEPTH.checked_sub(depth)?;
    loom_security::json::parse_within_limits(
        text,
        "MCP tool result",
        loom_security::json::MAX_PROCESS_RESPONSE_BYTES,
        remaining,
    )
    .ok()
}

pub(super) fn image_candidate_from_object(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<McpImageCandidate> {
    let properties = map.get("properties").and_then(serde_json::Value::as_object);
    let CandidateUrl {
        url: image_url,
        original: alternate_image_url,
    } = find_image_url_in_object(map).or_else(|| properties.and_then(find_image_url_in_object))?;
    let title = first_string(map, &["title", "label", "name"]).or_else(|| {
        properties.and_then(|object| first_string(object, &["title", "label", "name"]))
    });
    let thumbnail_url = first_imageish_string(
        map,
        &["thumbnail_url", "thumbnailUrl", "thumbnail", "placeholder"],
    )
    .or_else(|| {
        properties.and_then(|object| {
            first_imageish_string(
                object,
                &["thumbnail_url", "thumbnailUrl", "thumbnail", "placeholder"],
            )
        })
    });
    let width = first_u64(map, &["width"])
        .or_else(|| properties.and_then(|object| first_u64(object, &["width"])));
    let height = first_u64(map, &["height"])
        .or_else(|| properties.and_then(|object| first_u64(object, &["height"])));
    let source_page_url = first_string(map, &["source_page_url", "sourcePageUrl"]).or_else(|| {
        map.get("url")
            .and_then(serde_json::Value::as_str)
            // A `url` that is the image itself is not the page the image sits on. Both forms of the
            // image URL are excluded, because the rewritten one is what `image_url` holds while the
            // original is what this field usually carries.
            .filter(|url| {
                *url != image_url
                    && Some(*url) != alternate_image_url.as_deref()
                    && (url.starts_with("http://") || url.starts_with("https://"))
            })
            .map(str::to_owned)
    });
    Some(McpImageCandidate {
        image_url,
        alternate_image_url,
        title,
        thumbnail_url,
        source_page_url,
        width,
        height,
    })
}

pub(super) fn strip_image_url_modifiers(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let query_or_fragment_index = trimmed.find(['?', '#']).unwrap_or(trimmed.len());
    let (head, tail) = trimmed.split_at(query_or_fragment_index);
    let lower = head.to_ascii_lowercase();
    let mut trimmed_end = None;
    for suffix in IMAGE_URL_EXTENSIONS {
        let mut search_start = 0usize;
        while let Some(relative_index) = lower[search_start..].find(suffix) {
            let index = search_start + relative_index;
            let end = index + suffix.len();
            let next = head[end..].chars().next();
            if matches!(next, None | Some('!') | Some('/')) {
                trimmed_end = Some(end);
            }
            search_start = index + 1;
        }
    }
    let Some(end) = trimmed_end else {
        return None;
    };
    let normalized = format!("{}{}", &head[..end], tail).trim().to_owned();
    if normalized.is_empty() || normalized == trimmed {
        return None;
    }
    Some(normalized)
}
