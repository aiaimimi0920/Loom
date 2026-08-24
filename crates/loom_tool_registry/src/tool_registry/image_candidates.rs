//! Image candidate URL selection and metadata inference.

use super::*;

/// A candidate URL in the form it will be requested in, together with the string it came from.
///
/// Normalization sometimes rewrites the server's string. When it does, both forms are worth keeping:
/// the rewrite is what usually works, and the original is what is right when the rewrite guessed wrong.
pub(super) struct CandidateUrl {
    pub(super) url: String,
    /// Present only when `url` is a rewrite, so a caller can tell the two cases apart.
    pub(super) original: Option<String>,
}

impl CandidateUrl {
    fn verbatim(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            original: None,
        }
    }
}

pub(super) fn normalize_image_candidate_url(
    value: &str,
    allow_remote_without_extension: bool,
) -> Option<CandidateUrl> {
    let trimmed = value.trim();
    if trimmed.starts_with("data:image/") || looks_like_image_url(trimmed) {
        return Some(CandidateUrl::verbatim(trimmed));
    }
    if let Some(stripped) = strip_image_url_modifiers(trimmed) {
        if looks_like_image_url(&stripped)
            || (allow_remote_without_extension && looks_like_remote_url(&stripped))
        {
            return Some(CandidateUrl {
                url: stripped,
                original: Some(trimmed.to_owned()),
            });
        }
    }
    if allow_remote_without_extension && looks_like_remote_url(trimmed) {
        return Some(CandidateUrl::verbatim(trimmed));
    }
    None
}

pub(super) fn find_image_url_in_object(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<CandidateUrl> {
    for key in [
        "image_url",
        "imageUrl",
        "thumbnail_url",
        "thumbnailUrl",
        "src",
        "data",
    ] {
        if let Some(url) = map
            .get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(|value| normalize_image_candidate_url(value, true))
        {
            return Some(url);
        }
    }
    let url = map.get("url").and_then(serde_json::Value::as_str)?;
    if let Some(normalized) =
        normalize_image_candidate_url(url, object_looks_like_image_result(map))
    {
        return Some(normalized);
    }
    None
}

pub(super) fn first_imageish_string(
    map: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        let Some(value) = map.get(*key) else {
            continue;
        };
        let key_implies_image = matches!(
            *key,
            "thumbnail_url" | "thumbnailUrl" | "thumbnail" | "placeholder"
        );
        match value {
            serde_json::Value::String(text) => {
                if let Some(url) = normalize_image_candidate_url(text, key_implies_image) {
                    return Some(url.url);
                }
            }
            serde_json::Value::Object(object) => {
                if let Some(url) = first_string(
                    object,
                    &[
                        "src",
                        "url",
                        "image_url",
                        "imageUrl",
                        "thumbnail_url",
                        "thumbnailUrl",
                    ],
                )
                .and_then(|candidate| normalize_image_candidate_url(&candidate, key_implies_image))
                {
                    return Some(url.url);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn first_string(
    map: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}

pub(super) fn first_u64(
    map: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<u64> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(serde_json::Value::as_u64))
}

/// Which candidate the arguments asked for, and which one this crate can actually start from.
pub(super) struct McpImageCandidateSelection {
    /// The index the arguments named, kept exactly as asked so an out-of-range request stays reportable.
    pub(super) requested: Option<usize>,
    /// The index to start from: the requested one when it exists, the last candidate otherwise.
    pub(super) index: usize,
}

pub(super) fn selected_mcp_image_candidate_index(
    arguments: &serde_json::Value,
    candidate_count: usize,
) -> McpImageCandidateSelection {
    let requested = arguments
        .as_object()
        .and_then(|object| {
            [
                "result_index",
                "resultIndex",
                "selected_index",
                "selectedIndex",
                "image_index",
            ]
            .iter()
            .find_map(|key| object.get(*key))
        })
        .and_then(value_as_usize);
    if candidate_count == 0 {
        return McpImageCandidateSelection {
            requested,
            index: 0,
        };
    }
    McpImageCandidateSelection {
        requested,
        index: requested
            .unwrap_or(0)
            .min(candidate_count.saturating_sub(1)),
    }
}

/// Say why the candidate that was delivered is not the one the arguments named.
///
/// Two things move the choice, and both used to happen in silence. An index past the end of the list is
/// clamped to the last candidate, so asking for the eighth of three quietly returned the third. A
/// candidate that cannot be downloaded falls through to another one, so the canvas was told a different
/// image had been selected than the one it asked for. Neither is an error worth failing the call over —
/// an image still arrived — but neither should be invisible either.
pub(super) fn mcp_image_selection_note(
    requested: usize,
    clamped: usize,
    delivered: usize,
    candidate_count: usize,
) -> Option<String> {
    let mut notes = Vec::new();
    if requested >= candidate_count {
        notes.push(format!(
            "requested index {requested} is past the last of {candidate_count} candidates, \
             so candidate {clamped} was used instead"
        ));
    }
    if delivered != clamped {
        notes.push(format!(
            "candidate {clamped} could not be downloaded, so candidate {delivered} was used instead"
        ));
    }
    if notes.is_empty() {
        None
    } else {
        Some(notes.join("; "))
    }
}

pub(super) fn value_as_usize(value: &serde_json::Value) -> Option<usize> {
    match value {
        serde_json::Value::Number(number) => number.as_u64().map(|value| value as usize),
        serde_json::Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

pub(super) fn attach_mcp_image_candidate_metadata(
    image_result: &mut serde_json::Value,
    candidates: &[McpImageCandidate],
    selection: &McpImageCandidateSelection,
    delivered_index: usize,
) {
    let Some(result_object) = image_result.as_object_mut() else {
        return;
    };
    // `selectedIndex` stays the candidate the canvas is actually showing — the daemon reads it to know
    // which item is on screen. When that is not the one the arguments named, both the request and the
    // reason are recorded alongside it instead of leaving the difference unexplained.
    let note = selection.requested.and_then(|requested| {
        mcp_image_selection_note(
            requested,
            selection.index,
            delivered_index,
            candidates.len(),
        )
    });
    let mut candidate_metadata = serde_json::json!({
        "kind": "image.candidates",
        "selectedIndex": delivered_index,
        "items": candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| serde_json::json!({
                "index": index,
                "title": candidate.title,
                "imageUrl": candidate.image_url,
                "thumbnailUrl": candidate.thumbnail_url,
                "sourcePageUrl": candidate.source_page_url,
                "width": candidate.width,
                "height": candidate.height
            }))
            .collect::<Vec<_>>()
    });
    if let (Some(note), Some(requested)) = (note, selection.requested) {
        let object = candidate_metadata
            .as_object_mut()
            .expect("candidate metadata is built as an object");
        object.insert("requestedIndex".to_owned(), requested.into());
        object.insert("selectionNote".to_owned(), note.into());
    }
    result_object.insert(
        "loomMetadata".to_owned(),
        serde_json::json!({ "candidates": candidate_metadata }),
    );
}

pub(super) fn object_looks_like_image_result(
    map: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    map.contains_key("width")
        || map.contains_key("height")
        || map.contains_key("thumbnail_url")
        || map.contains_key("thumbnailUrl")
        || map
            .get("mimeType")
            .or_else(|| map.get("mime_type"))
            .and_then(serde_json::Value::as_str)
            .map(|mime| mime.starts_with("image/"))
            .unwrap_or(false)
}

pub(super) fn looks_like_image_url(value: &str) -> bool {
    if value.starts_with("data:image/") {
        return true;
    }
    if !looks_like_remote_url(value) {
        return false;
    }
    let path = value
        .split('?')
        .next()
        .unwrap_or(value)
        .split('#')
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase();
    IMAGE_URL_EXTENSIONS
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

/// Whether a declared MIME type is one this crate is willing to deliver as an image.
pub(super) fn is_supported_image_mime_type(mime_type: &str) -> bool {
    let mime_type = mime_type.trim().to_ascii_lowercase();
    SUPPORTED_IMAGE_MIME_TYPES
        .iter()
        .any(|supported| *supported == mime_type)
}

pub(super) fn looks_like_remote_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}
