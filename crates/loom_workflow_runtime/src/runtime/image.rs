//! YAML conversion, child argument insertion, image normalization, and content builders.

use super::*;
use std::fs::{File, OpenOptions};
use std::io::Read;

pub(super) const MAX_IMAGE_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_IMAGE_REFERENCE_BYTES: usize = 48 * 1024 * 1024;

pub(super) fn yaml_value_to_json(value: &serde_yaml::Value) -> JsonValue {
    serde_json::to_value(value).unwrap_or(JsonValue::Null)
}

pub(super) fn insert_child_argument(
    child_args: &mut JsonMap<String, JsonValue>,
    target: &str,
    value: JsonValue,
) {
    let target = target.strip_prefix("params.").unwrap_or(target);
    child_args.insert(target.to_owned(), value);
}

pub(super) fn insert_child_input(child_args: &mut JsonMap<String, JsonValue>, input: &str) {
    child_args
        .entry("input_base64".to_owned())
        .or_insert_with(|| JsonValue::String(input.to_owned()));
    child_args
        .entry("input".to_owned())
        .or_insert_with(|| JsonValue::String(input.to_owned()));
    child_args
        .entry("image".to_owned())
        .or_insert_with(|| JsonValue::String(input.to_owned()));
}

pub(super) fn extract_root_input(arguments: &JsonValue) -> Option<String> {
    let object = arguments.as_object()?;
    ["input_base64", "image", "data", "output_base64"]
        .iter()
        .find_map(|key| {
            object
                .get(*key)
                .and_then(JsonValue::as_str)
                .and_then(normalize_image_reference)
        })
        .or_else(|| {
            object
                .get("input")
                .and_then(|value| {
                    value.as_str().map(str::to_owned).or_else(|| {
                        value
                            .get("data")
                            .and_then(JsonValue::as_str)
                            .map(str::to_owned)
                    })
                })
                .and_then(|value| normalize_image_reference(&value))
        })
}

pub(super) fn json_value_as_image(value: &JsonValue) -> Option<String> {
    value
        .as_str()
        .and_then(normalize_image_reference)
        .or_else(|| {
            value
                .get("data")
                .and_then(JsonValue::as_str)
                .and_then(normalize_image_reference)
        })
        .or_else(|| {
            extract_image_output(value)
                .as_deref()
                .and_then(normalize_image_reference)
        })
}

pub(super) fn normalize_image_reference(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() <= MAX_IMAGE_REFERENCE_BYTES && is_image_like(value) {
        return Some(value.to_owned());
    }

    let file = open_image_file(Path::new(value))?;
    let mut bytes = Vec::with_capacity(file.metadata().ok()?.len() as usize);
    file.take(MAX_IMAGE_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_IMAGE_FILE_BYTES {
        return None;
    }
    loom_image_io::web_renderable_image_bytes_as_data_url(&bytes).map(|image| image.data_url)
}

/// Stored workflow metadata may carry an inline preview, but never an ambient filesystem capability.
pub(super) fn normalize_embedded_image_reference(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() <= MAX_IMAGE_REFERENCE_BYTES && is_image_like(value)).then(|| value.to_owned())
}

pub(super) fn metadata_is_windows_reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

fn open_image_file(path: &Path) -> Option<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow(&mut options);
    let file = options.open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_IMAGE_FILE_BYTES
        || metadata_is_windows_reparse(&metadata)
    {
        return None;
    }
    Some(file)
}

#[cfg(unix)]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW);
}

#[cfg(windows)]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(not(any(unix, windows)))]
fn configure_no_follow(_options: &mut OpenOptions) {}

pub(super) fn is_sticker_art(uses: &str) -> bool {
    uses == "__sticker__"
}

pub(super) fn is_image_like(value: &str) -> bool {
    value.starts_with("data:image/") || loom_image_io::looks_like_base64_image_payload(value)
}

pub(super) fn image_content_response(data: &str, mime_type: &str) -> JsonValue {
    let data = if data.starts_with("data:image/") && data.contains(";base64,") {
        data.to_owned()
    } else {
        format!("data:{mime_type};base64,{data}")
    };
    json!({
        "content": [
            {
                "type": "image",
                "data": data,
                "mimeType": mime_type
            }
        ]
    })
}

pub(super) fn text_content_response(text: &str) -> JsonValue {
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ]
    })
}
