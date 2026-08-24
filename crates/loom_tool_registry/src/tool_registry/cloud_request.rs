//! Cloud request policy, multipart assembly, and response normalization.

use super::*;

/// Resolve the deadline for a cloud API call.
///
/// A caller that states a deadline gets it: `execute_tool_with_timeout` is how the daemon passes
/// the run budget down, and clamping it to the 30 s default meant the budget could only ever be
/// shortened, never honoured. A package may declare `metadata.cloudApi.timeoutMs` for the API it
/// wraps, which applies when the caller states nothing. Both are bounded by
/// [`CLOUD_API_MAX_TIMEOUT`] so a bad number cannot pin a worker thread indefinitely, and by one
/// millisecond at the bottom because `reqwest` treats a zero timeout as "no timeout at all".
pub(super) fn cloud_api_timeout(tool: &ToolDefinition, requested: Option<Duration>) -> Duration {
    let declared = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("cloudApi"))
        .and_then(|cloud| cloud.get("timeoutMs"))
        .and_then(serde_json::Value::as_u64)
        .map(Duration::from_millis);
    requested
        .or(declared)
        .unwrap_or(CLOUD_API_TIMEOUT)
        .clamp(Duration::from_millis(1), CLOUD_API_MAX_TIMEOUT)
}

pub(super) fn cloud_network_policy(tool: &ToolDefinition) -> crate::network_policy::OutboundPolicy {
    let network = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("permissionPolicy"))
        .and_then(|policy| policy.get("network"));
    crate::network_policy::OutboundPolicy {
        // Loopback is off unless the package asks for it, matching `OutboundPolicy::default`. A
        // cloud Art that declares no network policy at all used to be allowed to call
        // `http://localhost:*` and `http://127.0.0.1:*` in cleartext, which reaches the Loom
        // daemon's own HTTP surface, Hook, and any local model server — while carrying the Art's
        // credential headers. An Art that genuinely talks to a local service knows it does, so it
        // can say so.
        allow_http_loopback: network
            .and_then(|network| network.get("allowLocalhost"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        allow_private_networks: network
            .and_then(|network| network.get("allowPrivateNetworks"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        allowed_domains: network
            .and_then(|network| network.get("domains"))
            .and_then(serde_json::Value::as_array)
            .map(|domains| {
                domains
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        ..crate::network_policy::OutboundPolicy::default()
    }
}

/// Outbound policy for downloading an image candidate that an MCP server chose.
///
/// The candidate URL comes out of the search result, so its host is whatever CDN the upstream
/// service happens to serve images from and a domain allowlist cannot be applied here — the
/// domains an image-search tool declares name its API host, not the image hosts. What can be
/// applied is the local-network boundary. Both download paths used to hardcode loopback on, which
/// handed any MCP server a request primitive into the Loom daemon's own HTTP surface, Hook, or a
/// local model server, just by returning `http://127.0.0.1:<port>/...` as an image URL. Loopback
/// and private networks are now off unless the tool declares them, which is the same lever a cloud
/// Art uses.
pub(super) fn mcp_image_download_policy(
    tool: &ToolDefinition,
) -> crate::network_policy::OutboundPolicy {
    let declared = cloud_network_policy(tool);
    crate::network_policy::OutboundPolicy {
        allow_http_loopback: declared.allow_http_loopback,
        allow_private_networks: declared.allow_private_networks,
        ..crate::network_policy::OutboundPolicy::default()
    }
}

pub(super) async fn build_cloud_multipart_form(
    tool: &ToolDefinition,
    body: Option<&str>,
    arguments: &serde_json::Value,
) -> ToolRegistryResult<multipart::Form> {
    let Some(body) = body.filter(|value| !value.trim().is_empty()) else {
        return Ok(multipart::Form::new());
    };
    let form_config = serde_json::from_str::<HashMap<String, String>>(body).map_err(|source| {
        ToolRegistryError::CloudTemplate {
            id: tool.id.clone(),
            field: "body",
            reason: source.to_string(),
        }
    })?;
    let mut form = multipart::Form::new();
    for (key, value) in form_config {
        let rendered_value = substitute_cloud_template(&value, arguments);
        // `__DISABLED__` is the author's own way of saying "leave this field out", so it is honoured
        // in silence.
        if rendered_value == "__DISABLED__" {
            continue;
        }
        // A placeholder the template declared and no argument filled used to remove the field from the
        // request, so the API answered with a confusing complaint about a parameter Loom never sent.
        // The check reads the template's own placeholders rather than looking for `{{` in the result,
        // because an argument value is allowed to contain braces and used to be dropped for it.
        if let Some(placeholder) = unresolved_cloud_template_placeholder(&value, &rendered_value) {
            return Err(ToolRegistryError::CloudTemplate {
                id: tool.id.clone(),
                field: "body",
                reason: format!(
                    "multipart field `{key}` still contains the unresolved placeholder \
                     `{placeholder}`"
                ),
            });
        }
        if rendered_value.is_empty() {
            continue;
        }

        if is_cloud_multipart_file_field(&value) {
            if rendered_value.starts_with("data:") {
                let mime =
                    data_url_mime_type(&rendered_value).unwrap_or("application/octet-stream");
                let extension = match mime {
                    "image/jpeg" => "jpg",
                    "image/webp" => "webp",
                    _ => "png",
                };
                let bytes =
                    loom_image_io::decode_data_url_bytes(&rendered_value).map_err(|error| {
                        ToolRegistryError::CloudTemplate {
                            id: tool.id.clone(),
                            field: "body",
                            reason: error.to_string(),
                        }
                    })?;
                let part = multipart::Part::bytes(bytes)
                    .file_name(format!("loom-cloud-input.{extension}"))
                    .mime_str(mime)
                    .map_err(|error| ToolRegistryError::CloudTemplate {
                        id: tool.id.clone(),
                        field: "body",
                        reason: error.to_string(),
                    })?;
                form = form.part(key, part);
            } else if is_remote_url_value(&rendered_value) {
                // Some hosted APIs take the image as a URL in the same field an author binds a
                // path to. A remote URL is not a local file, so it travels as a plain text field.
                form = form.text(key, rendered_value);
            } else {
                let path = cloud_multipart_upload_path(tool, &key, &rendered_value)?;
                let part = multipart::Part::file(path)
                    .await
                    .map_err(ToolRegistryError::Io)?;
                form = form.part(key, part);
            }
        } else {
            form = form.text(key, rendered_value);
        }
    }
    Ok(form)
}

/// Decide whether a multipart field carries a file.
///
/// Only the author's own template decides this. The heuristic used to also treat any field *named*
/// `file`, `image`, `image_file`, or `*_file` as a file field, so a caller could pass an arbitrary
/// absolute path as the value of an ordinary text field and the host would read that file off disk
/// and upload it to the third-party endpoint. An author who wants a file upload writes the path
/// binding — `{{inputs.x.path}}` — which is exactly what the Desktop cloud editor's multipart help
/// text tells them to write.
pub(super) fn is_cloud_multipart_file_field(template_value: &str) -> bool {
    template_value.contains(".path}}") || template_value.contains("inputs.image}}")
}

pub(super) fn is_remote_url_value(rendered_value: &str) -> bool {
    let lowered = rendered_value.to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

/// Resolve the local file a declared multipart file field wants to upload.
///
/// The rendered value comes from the execution arguments, so the previous `Path::exists` check
/// meant "read whatever path the caller names and upload it": a caller could aim a hosted Art at
/// an SSH key or a credential store and exfiltrate it through the Art's own endpoint. The path now
/// has to canonicalize to a real file inside a root Loom itself owns, the way the framework arm
/// confines every path it accepts.
pub(super) fn cloud_multipart_upload_path(
    tool: &ToolDefinition,
    field: &str,
    rendered_value: &str,
) -> ToolRegistryResult<PathBuf> {
    let template_error = |reason: String| ToolRegistryError::CloudTemplate {
        id: tool.id.clone(),
        field: "body",
        reason,
    };
    let canonical = fs::canonicalize(rendered_value).map_err(|error| {
        template_error(format!(
            "multipart field `{field}` cannot resolve upload path `{rendered_value}`: {error}"
        ))
    })?;
    if !canonical.is_file() {
        return Err(template_error(format!(
            "multipart field `{field}` upload path `{}` is not a file",
            canonical.display()
        )));
    }
    let inside_allowed_root = cloud_multipart_upload_roots(tool)
        .iter()
        .any(|root| cloud_upload_root_allows(root, &canonical));
    if !inside_allowed_root {
        return Err(template_error(format!(
            "multipart field `{field}` upload path `{}` resolves outside the Art package, control plane, and staged input roots",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// Roots a cloud Art may upload a local file from: its own package directory, the control plane
/// root that holds Art state, cache, and outputs, and the host temp directory the daemon stages
/// call inputs in.
pub(super) fn cloud_multipart_upload_roots(tool: &ToolDefinition) -> Vec<PathBuf> {
    let mut roots = vec![std::env::temp_dir()];
    if let Some(package_dir) = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("dir"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
    {
        roots.push(PathBuf::from(package_dir));
    }
    if let Some(control_plane_root) = crate::art_settings::control_plane_root_for_tool(tool) {
        roots.push(control_plane_root);
    }
    roots
}

/// The host temp directory is shared with every other program on the machine, so being inside it
/// is not by itself a reason to upload a file. Only Loom's own staging entries — every temp path
/// this workspace creates is prefixed `loom-` — count as allowed inside it. Any other allowed root
/// vouches for its whole subtree, including a control plane root that happens to live under temp.
pub(super) fn cloud_upload_root_allows(root: &Path, canonical: &Path) -> bool {
    let Ok(canonical_root) = fs::canonicalize(root) else {
        return false;
    };
    if !canonical.starts_with(&canonical_root) {
        return false;
    }
    if fs::canonicalize(std::env::temp_dir()).is_ok_and(|temp_root| temp_root == canonical_root) {
        return canonical
            .strip_prefix(&canonical_root)
            .ok()
            .and_then(|relative| relative.components().next())
            .and_then(|component| component.as_os_str().to_str())
            .is_some_and(|first| first.starts_with("loom-"));
    }
    true
}

pub(super) fn parse_cloud_method(
    tool: &ToolDefinition,
    method: &str,
) -> ToolRegistryResult<Method> {
    match method.to_ascii_uppercase().as_str() {
        "GET" => Ok(Method::GET),
        "POST" => Ok(Method::POST),
        "PUT" => Ok(Method::PUT),
        "PATCH" => Ok(Method::PATCH),
        "DELETE" => Ok(Method::DELETE),
        _ => Err(ToolRegistryError::CloudInvalidMethod {
            id: tool.id.clone(),
            method: method.to_owned(),
        }),
    }
}

pub(super) fn normalize_cloud_response(
    tool: &ToolDefinition,
    endpoint: &str,
    content_type: &str,
    body: CloudResponseBody,
) -> ToolRegistryResult<serde_json::Value> {
    let body = match body {
        CloudResponseBody::ImageDataUrl(data_url) => {
            let mime_type = cloud_image_mime_type(content_type).unwrap_or("image/png");
            return Ok(image_content_response(&data_url, mime_type));
        }
        CloudResponseBody::Text(body) => body.trim().to_owned(),
    };
    if body.is_empty() {
        return Ok(text_content_response(""));
    }
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(value) => Ok(normalize_cloud_json_value(value)),
        Err(source) if content_type.to_ascii_lowercase().contains("json") => {
            Err(ToolRegistryError::CloudJson {
                id: tool.id.clone(),
                endpoint: endpoint.to_owned(),
                source,
                body: bounded_error_text(&body),
            })
        }
        Err(_) => Ok(text_content_response(&body)),
    }
}

pub(super) fn normalize_cloud_json_value(value: serde_json::Value) -> serde_json::Value {
    if value
        .get("content")
        .and_then(serde_json::Value::as_array)
        .is_some()
    {
        return value;
    }
    if let Some(output) = value.get("output") {
        if let Some(image) = cloud_json_image_response(output) {
            return image;
        }
    }
    if let Some(image) = cloud_json_image_response(&value) {
        return image;
    }
    if let Some(text) = value.get("text").and_then(serde_json::Value::as_str) {
        return text_content_response(text);
    }
    text_content_response(&value.to_string())
}

/// Read a `data` string out of a cloud JSON object as an image, when the response gives a reason to
/// believe it is one.
///
/// A data URL is decoded and identified from its bytes. Raw base64 needs either a supported declared
/// raster MIME type or the existing base64-image heuristic, and is then subjected to the same byte
/// validation. Labels are never trusted on their own: malformed data, an SVG, and a payload whose
/// bytes are not a supported raster format all fall through to text handling.
pub(super) fn cloud_json_image_response(value: &serde_json::Value) -> Option<serde_json::Value> {
    let data = value.get("data").and_then(serde_json::Value::as_str)?;
    if data.starts_with("data:") {
        return image_response_from_image_data_url(data);
    }
    let declared = value
        .get("mimeType")
        .or_else(|| value.get("mime_type"))
        .and_then(serde_json::Value::as_str);
    if declared.is_some_and(|mime_type| !is_supported_image_mime_type(mime_type)) {
        return None;
    }
    if declared.is_none() && !looks_like_base64_payload(data) {
        return None;
    }
    image_response_from_base64_image(data)
}
