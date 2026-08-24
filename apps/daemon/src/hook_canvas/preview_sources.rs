// Convert a Hook image reference into a local filesystem path. Tauri asset URLs
// and `file://` URLs are decoded; plain paths pass through unchanged. Returns
// `None` for remote (http/https non-asset) references Loom must not fetch.
fn normalize_preview_source(source: &str) -> Option<String> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = asset_url_path(trimmed) {
        return non_empty_after_decode(&rest);
    }
    if let Some(rest) = trimmed.strip_prefix("file://") {
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        return non_empty_after_decode(rest);
    }
    // Any other URL scheme (http/https to a real host) is a remote resource that
    // the local preview endpoint must not read from disk.
    if looks_like_remote_url(trimmed) {
        return None;
    }
    Some(trimmed.to_owned())
}

// Extract the path portion of a Tauri asset URL such as
// `http://asset.localhost/C%3A%5C...png` or `asset://localhost/...`.
fn asset_url_path(source: &str) -> Option<String> {
    let without_scheme = source
        .strip_prefix("http://")
        .or_else(|| source.strip_prefix("https://"))
        .or_else(|| source.strip_prefix("asset://"))?;
    let (host, rest) = without_scheme.split_once('/')?;
    if !host.eq_ignore_ascii_case("asset.localhost") && !host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    Some(rest.to_owned())
}

fn looks_like_remote_url(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn non_empty_after_decode(encoded: &str) -> Option<String> {
    let decoded = percent_decode(encoded);
    let trimmed = decoded.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                if let (Some(high), Some(low)) = (high, low) {
                    decoded.push((high * 16 + low) as u8);
                    index += 3;
                    continue;
                }
                decoded.push(bytes[index]);
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn first_non_empty_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| non_empty_string(value.get(key)))
}

fn value_as_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
            .filter(|number| number.is_finite())
    })
}

fn normalized_coordinate(value: Option<f64>) -> (f64, bool) {
    match value {
        Some(value) if value.is_finite() => (value, false),
        _ => (0.0, true),
    }
}

fn normalized_size(value: Option<f64>) -> (f64, bool) {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => (value.max(MIN_NODE_SIZE), false),
        _ => (DEFAULT_NODE_SIZE, true),
    }
}

fn classify_node(node_type: Option<&str>, art_id: Option<&str>) -> HookCanvasNodeKind {
    if matches!(node_type, Some("art" | "artNode"))
        && art_id.is_some_and(|value| loom_workflow_store::validate_art_id(value).is_ok())
    {
        HookCanvasNodeKind::Art
    } else if node_type == Some("sticker") {
        HookCanvasNodeKind::Screenshot
    } else {
        HookCanvasNodeKind::Unknown
    }
}

fn normalized_status(value: Option<&str>, kind: &HookCanvasNodeKind) -> &'static str {
    match value.map(|value| value.to_ascii_lowercase()).as_deref() {
        Some("ready") => "ready",
        Some("processing") => "processing",
        Some("error") => "error",
        Some("unknown") => "unknown",
        _ if matches!(kind, HookCanvasNodeKind::Unknown) => "unknown",
        _ => "ready",
    }
}

// Directories the daemon is allowed to serve preview images from. Hook stores
// canvas images in two places: the session's own `images/` directory and the
// shared `clipboard_cache` under `%LOCALAPPDATA%\Hook`
// (current live captures referenced by absolute path or Tauri asset URL). Both
// are canonicalized so the preview endpoint can enforce a strict prefix check
// and never read outside them.
fn canonical_preview_roots(session_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut push = |path: PathBuf| {
        if let Ok(canonical) = fs::canonicalize(&path) {
            if canonical.is_dir() && !roots.contains(&canonical) {
                roots.push(canonical);
            }
        }
    };
    push(session_dir.join("images"));
    for root in hook_clipboard_cache_roots() {
        push(root);
    }
    roots
}

// Candidate `clipboard_cache` locations. Hook writes live capture images to
// `%LOCALAPPDATA%\Hook\clipboard_cache`; an explicit override supports isolated
// smokes and non-default installs.
fn hook_clipboard_cache_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dir) = std::env::var_os("LOOM_HOOK_IMAGE_ROOT") {
        let dir = PathBuf::from(dir);
        if !dir.as_os_str().is_empty() {
            roots.push(dir);
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Hook").join("clipboard_cache"));
    }
    roots
}

fn resolve_first_preview_source(
    session_dir: &Path,
    preview_roots: &[PathBuf],
    sources: &[String],
) -> Option<HookCanvasPreviewSource> {
    sources
        .iter()
        .find_map(|source| resolve_preview_source(session_dir, preview_roots, source))
}

fn resolve_preview_source(
    session_dir: &Path,
    preview_roots: &[PathBuf],
    source: &str,
) -> Option<HookCanvasPreviewSource> {
    let trimmed = source.trim();
    if preview_data_url_is_within_limit(trimmed) {
        return Some(HookCanvasPreviewSource::DataUrl(trimmed.to_owned()));
    }
    if preview_roots.is_empty() {
        return None;
    }
    let source_path = Path::new(trimmed);
    let candidate = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        session_dir.join(source_path)
    };
    let candidate = fs::canonicalize(candidate).ok()?;
    (candidate.is_file() && preview_roots.iter().any(|root| candidate.starts_with(root)))
        .then_some(HookCanvasPreviewSource::File(candidate))
}

fn is_supported_image_data_url(source: &str) -> bool {
    image_data_url_payload(source).is_some()
}

fn image_data_url_payload(source: &str) -> Option<&str> {
    let (header, payload) = source.split_once(',')?;
    if header.len() > MAX_PREVIEW_DATA_URL_HEADER_BYTES {
        return None;
    }
    let prefix = header.get(.."data:image/".len())?;
    let encoding = header.get(header.len().checked_sub(";base64".len())?..)?;
    (prefix.eq_ignore_ascii_case("data:image/") && encoding.eq_ignore_ascii_case(";base64"))
        .then_some(payload)
}

// Base64 expands three decoded bytes to four encoded bytes. Checking the
// encoded payload first prevents the decoder in the preview route from
// allocating beyond the same 20 MiB contract used for file previews.
fn preview_data_url_is_within_limit(source: &str) -> bool {
    preview_data_url_is_within_limit_for(source, MAX_PREVIEW_BYTES as usize)
}

fn preview_data_url_is_within_limit_for(source: &str, max_decoded_bytes: usize) -> bool {
    let Some(payload) = image_data_url_payload(source) else {
        return false;
    };
    base64_decoded_len_upper_bound(payload)
        .is_some_and(|decoded_bytes| decoded_bytes <= max_decoded_bytes)
}

fn base64_decoded_len_upper_bound(payload: &str) -> Option<usize> {
    let complete_groups = payload.len() / 4;
    let remainder_bytes = match payload.len() % 4 {
        0 => 0,
        2 => 1,
        3 => 2,
        _ => return None,
    };
    let padding_bytes = if payload.len() % 4 == 0 {
        if payload.ends_with("==") {
            2
        } else if payload.ends_with('=') {
            1
        } else {
            0
        }
    } else {
        0
    };
    complete_groups
        .checked_mul(3)?
        .checked_add(remainder_bytes)?
        .checked_sub(padding_bytes)
}

fn preview_source_is_within_limit(source: &HookCanvasPreviewSource) -> bool {
    match source {
        HookCanvasPreviewSource::File(_) => true,
        HookCanvasPreviewSource::DataUrl(data_url) => preview_data_url_is_within_limit(data_url),
    }
}

fn revision_for(bytes: &[u8], preview_versions: &[String]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    // Fold each node's preview content version into the revision so an in-place
    // image update (same session.json, same node id, same file path) still
    // produces a new revision and forces the desktop to replace its snapshot.
    for version in preview_versions {
        version.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

// A cheap content version for a preview image derived from its size and last
// modification time. This changes when Hook overwrites the file in place, which
// lets the preview URL bust WebView/browser caching without reading the whole
// image on every canvas read.
fn preview_source_version(source: &HookCanvasPreviewSource) -> String {
    match source {
        HookCanvasPreviewSource::File(path) => preview_file_content_version(path),
        HookCanvasPreviewSource::DataUrl(data_url) => preview_data_url_content_version(data_url),
    }
}

fn preview_file_content_version(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    if let Ok(metadata) = fs::metadata(path) {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                duration.as_nanos().hash(&mut hasher);
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

fn preview_data_url_content_version(data_url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    data_url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn modified_at_millis(path: &Path) -> Option<String> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().to_string())
}
