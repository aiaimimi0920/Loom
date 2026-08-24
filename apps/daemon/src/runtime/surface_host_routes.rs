// Surface host capabilities, package paths, route decoding, and MCP URL encoding.
fn default_declarative_surface_host_capabilities() -> SurfaceHostCapabilities {
    SurfaceHostCapabilities {
        api_version: loom_protocol::SURFACE_API_VERSION.to_owned(),
        runtimes: vec![SurfaceRuntimeKind::Declarative],
        nodes: loom_protocol::DECLARATIVE_SURFACE_NODE_TYPES
            .iter()
            .map(|node| (*node).to_owned())
            .collect(),
        transports: Vec::new(),
        capabilities: Vec::new(),
        input: loom_protocol::SurfaceInputCapabilities {
            pointer: true,
            hover: true,
            touch: true,
            keyboard: true,
        },
    }
}

fn surface_host_supports_capability(host: &SurfaceHostCapabilities, capability: &str) -> bool {
    host.capabilities.iter().any(|value| value == capability)
        || host.transports.iter().any(|value| value == capability)
        || matches!(
            capability,
            "input.pointer" if host.input.pointer
        )
        || matches!(
            capability,
            "input.hover" if host.input.hover
        )
        || matches!(
            capability,
            "input.touch" if host.input.touch
        )
        || matches!(
            capability,
            "input.keyboard" if host.input.keyboard
        )
}

fn resolve_surface_package_entry(
    control_plane_root: &Path,
    art_dir: &Path,
    entry: &str,
) -> std::result::Result<PathBuf, SurfaceStoreError> {
    let arts_root = fs::canonicalize(control_plane_root.join("arts")).map_err(|error| {
        SurfaceStoreError::Invalid(format!("Art package root is unavailable: {error}"))
    })?;
    let art_dir = fs::canonicalize(art_dir).map_err(|error| {
        SurfaceStoreError::Invalid(format!("Art package directory is unavailable: {error}"))
    })?;
    if !art_dir.starts_with(&arts_root) {
        return Err(SurfaceStoreError::Invalid(
            "Surface package directory escapes the Art package root".to_owned(),
        ));
    }
    let scene_path = fs::canonicalize(art_dir.join(entry)).map_err(|error| {
        SurfaceStoreError::Invalid(format!("Surface entry is unavailable: {error}"))
    })?;
    if !scene_path.starts_with(&art_dir) || !scene_path.is_file() {
        return Err(SurfaceStoreError::Invalid(
            "Surface entry escapes its immutable package".to_owned(),
        ));
    }
    Ok(scene_path)
}

fn invalid_surface_payload(error: serde_json::Error) -> Result<(u16, String)> {
    structured_error(
        400,
        json!({
            "code": "invalid_surface_payload",
            "message": error.to_string(),
        }),
    )
}

fn surface_store_error(error: SurfaceStoreError) -> Result<(u16, String)> {
    structured_error(
        error.status_code(),
        json!({
            "code": error.code(),
            "message": error.to_string(),
        }),
    )
}

fn decoded_package_path_id(path: &str, prefix: &str) -> Option<String> {
    let encoded = path.strip_prefix(prefix)?;
    decode_package_route_id(encoded)
}

fn decoded_package_path_id_with_suffix(path: &str, prefix: &str, suffix: &str) -> Option<String> {
    let encoded = path.strip_prefix(prefix)?.strip_suffix(suffix)?;
    decode_package_route_id(encoded)
}

fn decode_package_route_id(encoded: &str) -> Option<String> {
    if encoded.is_empty() || encoded.contains('/') || encoded.contains('?') {
        return None;
    }
    let decoded = percent_decode(encoded);
    if let Some((publisher, id)) = decoded.split_once('/') {
        if publisher.contains('/')
            || !loom_protocol::is_safe_publisher_id(publisher)
            || !loom_protocol::is_safe_package_id(id)
        {
            return None;
        }
    } else if decoded.is_empty()
        || decoded.contains(['/', '\\', ':'])
        || decoded == "."
        || decoded == ".."
        || decoded.contains("..")
    {
        return None;
    }
    Some(decoded)
}

fn tool_execute_path_id(path: &str) -> Option<String> {
    decoded_package_path_id_with_suffix(path, "/v1/tools/", "/execute")
}

fn query_value(path: &str, name: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (percent_decode(key) == name).then(|| percent_decode(value))
    })
}

fn build_mcp_registry_url(
    endpoint: &str,
    search: Option<&str>,
    limit: Option<u32>,
    cursor: Option<&str>,
    updated_since: Option<&str>,
    version: Option<&str>,
    include_deleted: bool,
) -> String {
    let safe_limit = limit.unwrap_or(60).clamp(1, 100);
    let mut pairs = vec![format!("limit={safe_limit}")];
    if let Some(search_text) = search.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push(format!("search={}", percent_encode(search_text)));
    }
    if let Some(cursor_text) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push(format!("cursor={}", percent_encode(cursor_text)));
    }
    if let Some(updated_since) = updated_since
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        pairs.push(format!("updated_since={}", percent_encode(updated_since)));
    }
    let version = version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest");
    pairs.push(format!("version={}", percent_encode(version)));
    if include_deleted {
        pairs.push("include_deleted=true".to_owned());
    }
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    format!(
        "{}{separator}{}",
        endpoint.trim_end_matches('&'),
        pairs.join("&")
    )
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(if bytes[index] == b'+' {
            b' '
        } else {
            bytes[index]
        });
        index += 1;
    }
    String::from_utf8_lossy(&decoded).to_string()
}

#[derive(Debug, Deserialize)]
struct PutManagedConfigRequest {
    expected_revision: u64,
    config: Value,
}
