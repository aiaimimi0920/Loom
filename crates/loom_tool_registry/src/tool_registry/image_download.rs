//! Bounded image download, data URL parsing, and byte sniffing.

use super::*;

pub(super) fn download_mcp_image_candidate(
    url: &str,
    referer: Option<&str>,
    policy: &crate::network_policy::OutboundPolicy,
    deadline: McpImageDownloadDeadline,
) -> Option<serde_json::Value> {
    // Each attempt reads the deadline again, so the fallback cannot spend a budget the first attempt
    // already used up.
    let reqwest_attempt = deadline.next_attempt_timeout().and_then(|timeout| {
        download_mcp_image_candidate_with_reqwest(url, referer, policy, timeout)
    });
    reqwest_attempt.or_else(|| {
        let timeout = deadline.next_attempt_timeout()?;
        download_mcp_image_candidate_with_platform_fallback(url, referer, policy, timeout)
    })
}

pub(super) fn download_mcp_image_candidate_with_reqwest(
    url: &str,
    referer: Option<&str>,
    policy: &crate::network_policy::OutboundPolicy,
    timeout: Duration,
) -> Option<serde_json::Value> {
    let parsed_url = reqwest::Url::parse(url).ok()?;
    network_policy::validate_outbound_url(&parsed_url, policy).ok()?;
    let client =
        network_policy::secure_client(MCP_IMAGE_FETCH_USER_AGENT, timeout, policy.clone()).ok()?;
    let mut request = client
        .get(parsed_url)
        .header(reqwest::header::ACCEPT, MCP_IMAGE_FETCH_ACCEPT)
        .header(
            reqwest::header::ACCEPT_LANGUAGE,
            MCP_IMAGE_FETCH_ACCEPT_LANGUAGE,
        );
    if let Some(referer) = referer.filter(|value| looks_like_remote_url(value)) {
        request = request.header(reqwest::header::REFERER, referer);
    }
    let response = request.send().ok()?.error_for_status().ok()?;
    let header_mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| is_supported_image_mime_type(value))
        .map(str::to_owned);
    let bytes = network_policy::read_bounded_response(response, MAX_MCP_IMAGE_BYTES).ok()?;
    let mime_type = header_mime_type
        .or_else(|| infer_image_mime_type_from_url(url))
        .or_else(|| infer_image_mime_type_from_bytes(&bytes))?;
    Some(image_content_response(&BASE64.encode(&bytes), &mime_type))
}

#[cfg(windows)]
pub(super) fn download_mcp_image_candidate_with_platform_fallback(
    url: &str,
    referer: Option<&str>,
    policy: &crate::network_policy::OutboundPolicy,
    timeout: Duration,
) -> Option<serde_json::Value> {
    let (mime_type, bytes) =
        download_image_bytes_with_powershell_httpclient(url, referer, policy, timeout)?;
    Some(image_content_response(&BASE64.encode(&bytes), &mime_type))
}

#[cfg(not(windows))]
pub(super) fn download_mcp_image_candidate_with_platform_fallback(
    _url: &str,
    _referer: Option<&str>,
    _policy: &crate::network_policy::OutboundPolicy,
    _timeout: Duration,
) -> Option<serde_json::Value> {
    None
}

#[cfg(windows)]
pub(super) fn download_image_bytes_with_powershell_httpclient(
    url: &str,
    referer: Option<&str>,
    policy: &crate::network_policy::OutboundPolicy,
    timeout: Duration,
) -> Option<(String, Vec<u8>)> {
    let parsed_url = reqwest::Url::parse(url).ok()?;
    network_policy::validate_outbound_url(&parsed_url, policy).ok()?;
    let script = r#"
Add-Type -AssemblyName System.Net.Http
$handler = New-Object System.Net.Http.HttpClientHandler
$handler.AllowAutoRedirect = $false
$client = New-Object System.Net.Http.HttpClient($handler)
$timeoutSeconds = 0
if ($env:LOOM_FETCH_TIMEOUT_SECONDS) {
  $timeoutSeconds = [double]$env:LOOM_FETCH_TIMEOUT_SECONDS
}
if ($timeoutSeconds -le 0) {
  $timeoutSeconds = 30
}
$client.Timeout = [TimeSpan]::FromSeconds($timeoutSeconds)
$client.DefaultRequestHeaders.UserAgent.ParseAdd($env:LOOM_FETCH_USER_AGENT)
$client.DefaultRequestHeaders.Accept.ParseAdd($env:LOOM_FETCH_ACCEPT)
$client.DefaultRequestHeaders.AcceptLanguage.ParseAdd($env:LOOM_FETCH_ACCEPT_LANGUAGE)
if ($env:LOOM_FETCH_REFERER) {
  try {
    $client.DefaultRequestHeaders.Referrer = [Uri]$env:LOOM_FETCH_REFERER
  } catch {
  }
}
try {
  $resp = $client.GetAsync($env:LOOM_FETCH_URL, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
  if (-not $resp.IsSuccessStatusCode) {
    exit 22
  }
  $maxBytes = [int64]$env:LOOM_FETCH_MAX_BYTES
  if ($resp.Content.Headers.ContentLength -and $resp.Content.Headers.ContentLength.Value -gt $maxBytes) {
    exit 23
  }
  $stream = $resp.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
  $memory = New-Object System.IO.MemoryStream
  $buffer = New-Object byte[] 81920
  try {
    while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
      if ($memory.Length + $read -gt $maxBytes) {
        exit 23
      }
      $memory.Write($buffer, 0, $read)
    }
    $bytes = $memory.ToArray()
  } finally {
    $stream.Dispose()
    $memory.Dispose()
  }
  $contentType = ''
  if ($resp.Content.Headers.ContentType) {
    $contentType = $resp.Content.Headers.ContentType.MediaType
  }
  @{ contentType = $contentType; dataBase64 = [Convert]::ToBase64String($bytes) } | ConvertTo-Json -Compress
} finally {
  $client.Dispose()
  $handler.Dispose()
}
"#;

    let mut command = Command::new("powershell.exe");
    command
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .env("LOOM_FETCH_URL", url)
        .env("LOOM_FETCH_MAX_BYTES", MAX_MCP_IMAGE_BYTES.to_string())
        .env(
            "LOOM_FETCH_TIMEOUT_SECONDS",
            format!("{:.3}", timeout.as_secs_f64()),
        )
        .env("LOOM_FETCH_USER_AGENT", MCP_IMAGE_FETCH_USER_AGENT)
        .env("LOOM_FETCH_ACCEPT", MCP_IMAGE_FETCH_ACCEPT)
        .env(
            "LOOM_FETCH_ACCEPT_LANGUAGE",
            MCP_IMAGE_FETCH_ACCEPT_LANGUAGE,
        )
        .env(
            "LOOM_FETCH_REFERER",
            referer
                .filter(|value| looks_like_remote_url(value))
                .unwrap_or(""),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut process = ProcessSpec::from_command(&command);
    // The script's own HttpClient timeout is the same value, so whichever fires first the attempt
    // ends inside the caller's remaining budget rather than at a fixed 30 s.
    process.limits.timeout = timeout;
    process.limits.stdout_bytes = MAX_MCP_IMAGE_BYTES.saturating_mul(2);
    process.limits.stderr_bytes = 1024 * 1024;
    process.limits.memory_bytes = Some(256 * 1024 * 1024);
    process.limits.max_processes = Some(2);
    let output = loom_process::run_with_input(&process, &[]).ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if stdout.is_empty() {
        return None;
    }
    let response = serde_json::from_str::<serde_json::Value>(&stdout).ok()?;
    let bytes = response
        .get("dataBase64")
        .and_then(serde_json::Value::as_str)
        .and_then(|base64| BASE64.decode(base64).ok())?;
    if bytes.len() > MAX_MCP_IMAGE_BYTES {
        return None;
    }
    let mime_type = response
        .get("contentType")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| is_supported_image_mime_type(value))
        .map(str::to_owned)
        .or_else(|| infer_image_mime_type_from_url(url))
        .or_else(|| infer_image_mime_type_from_bytes(&bytes))?;
    Some((mime_type, bytes))
}

pub(super) fn image_response_from_mcp_candidate(
    candidate: &McpImageCandidate,
    policy: &crate::network_policy::OutboundPolicy,
    deadline: McpImageDownloadDeadline,
) -> Option<serde_json::Value> {
    let referer = candidate.source_page_url.as_deref();
    // The forms are tried in the order most likely to pay off: the normalized URL first, since a CDN
    // modifier is the usual reason a URL needed rewriting at all; then the server's own string, which
    // is the right one whenever the rewrite cut into a real path; then the thumbnail, which is a
    // smaller image rather than another address for the same one. Duplicates are skipped so a
    // candidate that repeats itself does not spend the download budget twice on one address.
    let mut attempted: Vec<&str> = Vec::new();
    for url in [
        Some(candidate.image_url.as_str()),
        candidate.alternate_image_url.as_deref(),
        candidate.thumbnail_url.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if attempted.contains(&url) {
            continue;
        }
        attempted.push(url);
        if let Some(response) =
            image_response_from_mcp_candidate_url(url, referer, policy, deadline)
        {
            return Some(response);
        }
    }
    None
}

pub(super) fn image_response_from_mcp_candidate_url(
    url: &str,
    referer: Option<&str>,
    policy: &crate::network_policy::OutboundPolicy,
    deadline: McpImageDownloadDeadline,
) -> Option<serde_json::Value> {
    if url.starts_with("data:image/") {
        return image_response_from_image_data_url(url);
    }
    for candidate_url in std::iter::once(url.to_owned()).chain(
        strip_image_url_modifiers(url)
            .into_iter()
            .filter(|normalized| normalized != url),
    ) {
        if let Some(response) =
            download_mcp_image_candidate(&candidate_url, referer, policy, deadline)
        {
            return Some(response);
        }
    }
    None
}

/// Turn a candidate that arrived as a data URL into an image response, or reject it.
///
/// The download path proves an image is an image by reading its bytes; a data URL used to skip that
/// entirely — the server's string went to the canvas verbatim with the MIME type read out of the URL
/// it came in on. Malformed base64, a payload truncated in transit, and a MIME type that disagrees
/// with the bytes all arrived unchallenged, and the length bound was an estimate of the encoded form
/// rather than a limit on what was decoded.
///
/// So the payload is decoded here, held to the same ceiling a download is held to, identified from
/// its own bytes, and re-encoded. The canvas then receives a MIME type that describes what it was
/// actually given, and a format outside `SUPPORTED_IMAGE_MIME_TYPES` — SVG among them — has no way
/// through, since `infer_image_mime_type_from_bytes` only recognizes raster signatures.
pub(super) fn image_response_from_image_data_url(url: &str) -> Option<serde_json::Value> {
    // Checked before the decode so an absurd string is rejected while there is still only one copy
    // of it: 4 encoded characters per 3 decoded bytes, plus room for the header.
    if url.len() > MAX_MCP_IMAGE_BYTES.saturating_mul(4) / 3 + 4096 {
        return None;
    }
    let (header, payload) = url.split_once(',')?;
    if !header.trim_end().ends_with(";base64") {
        return None;
    }
    image_response_from_base64_image(payload)
}

/// Decode a raw base64 payload, enforce the image byte ceiling, and identify the format from bytes.
pub(super) fn image_response_from_base64_image(payload: &str) -> Option<serde_json::Value> {
    if payload.len() > MAX_MCP_IMAGE_BYTES.saturating_mul(4) / 3 + 4 {
        return None;
    }
    let bytes = BASE64.decode(payload.trim()).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_MCP_IMAGE_BYTES {
        return None;
    }
    let mime_type = infer_image_mime_type_from_bytes(&bytes)?;
    Some(image_content_response(&BASE64.encode(&bytes), &mime_type))
}

pub(super) fn infer_image_mime_type_from_url(url: &str) -> Option<String> {
    let path = url
        .split('?')
        .next()
        .unwrap_or(url)
        .split('#')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    let mime_type = if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".bmp") {
        "image/bmp"
    } else if path.ends_with(".avif") {
        "image/avif"
    } else {
        return None;
    };
    Some(mime_type.to_owned())
}

pub(super) fn infer_image_mime_type_from_bytes(bytes: &[u8]) -> Option<String> {
    let mime_type = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis")
    {
        "image/avif"
    } else {
        return None;
    };
    Some(mime_type.to_owned())
}
