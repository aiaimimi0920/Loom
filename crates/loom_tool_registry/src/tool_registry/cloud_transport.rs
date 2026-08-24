//! Cloud request transport and bounded response reading.

use super::*;

pub(super) fn execute_cloud_api_tool(
    tool: &ToolDefinition,
    endpoint: &str,
    method: &str,
    content_type: Option<&str>,
    headers: Option<&str>,
    body: Option<&str>,
    arguments: serde_json::Value,
    timeout: Duration,
    cancellation: Option<&AtomicBool>,
) -> ToolRegistryResult<serde_json::Value> {
    let stop_if_cancelled = || -> ToolRegistryResult<()> {
        if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
            return Err(ToolRegistryError::ExecutionCancelled {
                id: tool.id.clone(),
            });
        }
        Ok(())
    };

    // A cancelled run does nothing at all, so nothing is rendered and no request leaves the host.
    stop_if_cancelled()?;
    let endpoint_template = endpoint;
    let endpoint = substitute_cloud_template_with(
        endpoint_template,
        &arguments,
        percent_encode_cloud_template_value,
    );
    validate_rendered_cloud_authority(endpoint_template, &endpoint).map_err(|reason| {
        ToolRegistryError::CloudSecurity {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            reason,
        }
    })?;
    let method = parse_cloud_method(tool, method)?;
    let content_type = content_type
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("application/json")
        .trim()
        .to_owned();
    let content_type_lower = content_type.to_ascii_lowercase();
    let policy = cloud_network_policy(tool);
    let parsed_endpoint =
        reqwest::Url::parse(&endpoint).map_err(|error| ToolRegistryError::CloudSecurity {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            reason: error.to_string(),
        })?;
    crate::network_policy::validate_outbound_url(&parsed_endpoint, &policy).map_err(|reason| {
        ToolRegistryError::CloudSecurity {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            reason,
        }
    })?;
    let client = crate::network_policy::secure_async_client("Loom/0.1 Cloud API", timeout, policy)
        .map_err(|reason| ToolRegistryError::CloudSecurity {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            reason,
        })?;
    let mut request = client.request(method.clone(), &endpoint);
    let mut explicit_content_type = false;
    if let Some(headers) = headers.filter(|value| !value.trim().is_empty()) {
        let rendered_headers = render_cloud_json_template(tool, "headers", headers, &arguments)?;
        let header_map = serde_json::from_value::<HashMap<String, String>>(rendered_headers)
            .map_err(|source| ToolRegistryError::CloudTemplate {
                id: tool.id.clone(),
                field: "headers",
                reason: source.to_string(),
            })?;
        for (name, value) in header_map {
            // A header name or value carrying a control character would either be rejected deep
            // inside the HTTP client or, on a lax client, split the request. Refuse it here where
            // the reason can name the header.
            if header_text_has_control_character(&name) || header_text_has_control_character(&value)
            {
                return Err(ToolRegistryError::CloudTemplate {
                    id: tool.id.clone(),
                    field: "headers",
                    reason: format!("header `{name}` contains a control character"),
                });
            }
            if name.eq_ignore_ascii_case("content-type") {
                explicit_content_type = true;
                if content_type_lower == "multipart/form-data" {
                    continue;
                }
            }
            request = request.header(name, value);
        }
    }

    if matches!(method, Method::POST | Method::PUT | Method::PATCH) {
        if content_type_lower == "multipart/form-data" {
            let form = run_cloud_future(build_cloud_multipart_form(tool, body, &arguments))
                .map_err(|reason| ToolRegistryError::CloudSecurity {
                    id: tool.id.clone(),
                    endpoint: endpoint.clone(),
                    reason,
                })??;
            request = request.multipart(form);
        } else if let Some(body) = body {
            if content_type_lower.contains("json") {
                let json_body = render_cloud_json_template(tool, "body", body, &arguments)?;
                request = request.json(&json_body);
            } else {
                let rendered_body = substitute_cloud_template(body, &arguments);
                request = request.body(rendered_body);
                if !explicit_content_type {
                    request = request.header(reqwest::header::CONTENT_TYPE, content_type.clone());
                }
            }
        } else {
            request = request.json(&arguments);
        }
    } else if body.is_some_and(|value| !value.trim().is_empty()) {
        // Only POST, PUT, and PATCH carry the declared body. A body declared on any other method used
        // to be dropped on the way out, so the request went without the parameters the author wrote and
        // the API answered by complaining about what was missing. The mistake is named here instead.
        return Err(ToolRegistryError::CloudTemplate {
            id: tool.id.clone(),
            field: "body",
            reason: format!(
                "a `body` is declared but the `{}` method does not send one; use POST, PUT, or PATCH, \
                 or move the values into the endpoint's query string",
                method.as_str()
            ),
        });
    }
    stop_if_cancelled()?;
    let response = run_cloud_future(execute_cloud_http_request(request, cancellation))
        .map_err(|reason| ToolRegistryError::CloudSecurity {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            reason,
        })?
        .map_err(|error| match error {
            CloudTransportError::Cancelled => ToolRegistryError::ExecutionCancelled {
                id: tool.id.clone(),
            },
            CloudTransportError::Request(source) => ToolRegistryError::CloudRequest {
                id: tool.id.clone(),
                endpoint: endpoint.clone(),
                source,
            },
            CloudTransportError::ResponseTooLarge => ToolRegistryError::CloudSecurity {
                id: tool.id.clone(),
                endpoint: endpoint.clone(),
                reason: format!("response exceeds {MAX_CLOUD_RESPONSE_BYTES} bytes"),
            },
            CloudTransportError::InvalidUtf8 => ToolRegistryError::CloudSecurity {
                id: tool.id.clone(),
                endpoint: endpoint.clone(),
                reason: "non-image response body is not valid UTF-8".to_owned(),
            },
            CloudTransportError::InvalidImage => ToolRegistryError::CloudSecurity {
                id: tool.id.clone(),
                endpoint: endpoint.clone(),
                reason: "response declared an image but did not contain a supported raster image"
                    .to_owned(),
            },
        })?;
    stop_if_cancelled()?;

    if !response.status.is_success() {
        return Err(ToolRegistryError::CloudHttpStatus {
            id: tool.id.clone(),
            endpoint,
            status: response.status.as_u16(),
            body: bounded_error_text(response.body.as_text()),
        });
    }

    normalize_cloud_response(tool, &endpoint, &response.content_type, response.body)
}

pub(super) fn run_cloud_future<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = T> + Send,
    T: Send,
{
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| format!("create cloud HTTP runtime: {error}"))?;
                Ok(runtime.block_on(future))
            })
            .join()
            .map_err(|_| "cloud HTTP runtime thread panicked".to_owned())?
    })
}

#[derive(Debug)]
pub(super) enum CloudTransportError {
    Cancelled,
    Request(reqwest::Error),
    ResponseTooLarge,
    InvalidUtf8,
    InvalidImage,
}

pub(super) struct CloudWireResponse {
    status: reqwest::StatusCode,
    content_type: String,
    body: CloudResponseBody,
}

pub(super) enum CloudResponseBody {
    ImageDataUrl(String),
    Text(String),
}

impl CloudResponseBody {
    fn as_text(&self) -> &str {
        match self {
            Self::ImageDataUrl(value) | Self::Text(value) => value,
        }
    }
}

pub(super) enum CloudBodyAccumulator {
    Image {
        data_url: String,
        pending: Vec<u8>,
        invalid: bool,
    },
    Text(Vec<u8>),
}

const MAX_IMAGE_SIGNATURE_BYTES: usize = 12;

impl CloudBodyAccumulator {
    pub(super) fn new(image_mime_type: Option<&str>, content_length: Option<u64>) -> Self {
        let raw_capacity = content_length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or_default()
            .min(MAX_CLOUD_RESPONSE_BYTES);
        if image_mime_type.is_some() {
            let encoded_capacity = raw_capacity
                .saturating_add(2)
                .saturating_div(3)
                .saturating_mul(4)
                .saturating_add(32);
            Self::Image {
                data_url: String::with_capacity(encoded_capacity),
                pending: Vec::with_capacity(MAX_IMAGE_SIGNATURE_BYTES),
                invalid: false,
            }
        } else {
            Self::Text(Vec::with_capacity(raw_capacity))
        }
    }

    pub(super) fn push(&mut self, mut chunk: &[u8]) {
        match self {
            Self::Text(bytes) => bytes.extend_from_slice(chunk),
            Self::Image {
                data_url,
                pending,
                invalid,
            } => {
                if *invalid {
                    return;
                }
                if data_url.is_empty() {
                    let taken = (MAX_IMAGE_SIGNATURE_BYTES - pending.len()).min(chunk.len());
                    pending.extend_from_slice(&chunk[..taken]);
                    chunk = &chunk[taken..];
                    if let Some(mime_type) = infer_image_mime_type_from_bytes(pending) {
                        data_url.push_str("data:");
                        data_url.push_str(&mime_type);
                        data_url.push_str(";base64,");
                        let prefix = std::mem::take(pending);
                        append_base64_chunk(data_url, pending, &prefix);
                    } else if pending.len() == MAX_IMAGE_SIGNATURE_BYTES {
                        pending.clear();
                        *invalid = true;
                        return;
                    } else {
                        return;
                    }
                }
                append_base64_chunk(data_url, pending, chunk);
            }
        }
    }

    pub(super) fn finish(mut self) -> Result<CloudResponseBody, CloudTransportError> {
        match &mut self {
            Self::Image {
                data_url,
                pending,
                invalid,
            } => {
                if *invalid || data_url.is_empty() {
                    return Err(CloudTransportError::InvalidImage);
                }
                if !pending.is_empty() {
                    BASE64.encode_string(pending.as_slice(), data_url);
                    pending.clear();
                }
            }
            Self::Text(_) => {}
        }
        match self {
            Self::Image { data_url, .. } => Ok(CloudResponseBody::ImageDataUrl(data_url)),
            Self::Text(bytes) => String::from_utf8(bytes)
                .map(CloudResponseBody::Text)
                .map_err(|_| CloudTransportError::InvalidUtf8),
        }
    }
}

fn append_base64_chunk(data_url: &mut String, pending: &mut Vec<u8>, mut chunk: &[u8]) {
    if !pending.is_empty() {
        let needed = 3 - pending.len();
        let taken = needed.min(chunk.len());
        pending.extend_from_slice(&chunk[..taken]);
        chunk = &chunk[taken..];
        if pending.len() == 3 {
            BASE64.encode_string(pending.as_slice(), data_url);
            pending.clear();
        }
    }
    let aligned = chunk.len() - (chunk.len() % 3);
    if aligned > 0 {
        BASE64.encode_string(&chunk[..aligned], data_url);
    }
    pending.extend_from_slice(&chunk[aligned..]);
}

pub(super) async fn wait_for_cloud_cancellation(cancellation: &AtomicBool) {
    while !cancellation.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub(super) async fn execute_cloud_http_request(
    request: reqwest::RequestBuilder,
    cancellation: Option<&AtomicBool>,
) -> Result<CloudWireResponse, CloudTransportError> {
    let mut response = if let Some(cancellation) = cancellation {
        tokio::select! {
            response = request.send() => response,
            () = wait_for_cloud_cancellation(cancellation) => {
                return Err(CloudTransportError::Cancelled);
            }
        }
    } else {
        request.send().await
    }
    .map_err(CloudTransportError::Request)?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > MAX_CLOUD_RESPONSE_BYTES as u64) {
        return Err(CloudTransportError::ResponseTooLarge);
    }
    let image_mime_type = status
        .is_success()
        .then(|| cloud_image_mime_type(&content_type))
        .flatten();
    let mut accumulator = CloudBodyAccumulator::new(image_mime_type, content_length);
    let mut raw_bytes = 0_usize;
    loop {
        let chunk = if let Some(cancellation) = cancellation {
            tokio::select! {
                chunk = response.chunk() => chunk,
                () = wait_for_cloud_cancellation(cancellation) => {
                    return Err(CloudTransportError::Cancelled);
                }
            }
        } else {
            response.chunk().await
        }
        .map_err(CloudTransportError::Request)?;
        let Some(chunk) = chunk else {
            break;
        };
        raw_bytes = raw_bytes.saturating_add(chunk.len());
        if raw_bytes > MAX_CLOUD_RESPONSE_BYTES {
            return Err(CloudTransportError::ResponseTooLarge);
        }
        accumulator.push(&chunk);
    }
    Ok(CloudWireResponse {
        status,
        content_type,
        body: accumulator.finish()?,
    })
}

pub(super) fn cloud_image_mime_type(content_type: &str) -> Option<&'static str> {
    let declared = content_type.split(';').next().map(str::trim)?;
    SUPPORTED_IMAGE_MIME_TYPES
        .iter()
        .copied()
        .find(|supported| declared.eq_ignore_ascii_case(supported))
}
