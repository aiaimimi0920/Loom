//! Normalized tool response helpers.

pub(super) fn data_url_mime_type(data_url: &str) -> Option<&str> {
    let data_url = data_url.strip_prefix("data:")?;
    let mime_type = data_url.split(';').next()?.trim();
    (!mime_type.is_empty()).then_some(mime_type)
}

pub(super) fn image_content_response(data: &str, mime_type: &str) -> serde_json::Value {
    let data = if data.starts_with("data:image/") && data.contains(";base64,") {
        data.to_owned()
    } else {
        format!("data:{mime_type};base64,{data}")
    };
    serde_json::json!({
        "content": [
            {
                "type": "image",
                "data": data,
                "mimeType": mime_type
            }
        ]
    })
}

pub(super) fn text_content_response(text: &str) -> serde_json::Value {
    serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ]
    })
}

/// Whether a string is plausibly a base64 image payload that no other signal identified.
///
/// The rule itself lives in `loom_image_io` so that the workflow runtime, which has to answer the
/// same question about the same values, cannot drift from it.
pub(super) fn looks_like_base64_payload(value: &str) -> bool {
    loom_image_io::looks_like_base64_image_payload(value)
}
