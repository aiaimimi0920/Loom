//! Bounded command paths, loopback URL parsing, binary HTTP, and base64 output.

use super::*;

pub(super) fn percent_encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

// Fetch a binary daemon response (e.g. a preview image) over the native HTTP
// client. Unlike `http_request_json`, this reads raw bytes so image payloads are
// not corrupted by UTF-8 decoding, and it returns the Content-Type so the caller
// can build a correct `data:` URL.
pub(super) fn http_get_binary(base_url: &str, path: &str) -> Result<(String, Vec<u8>), String> {
    let (host, port) = parse_loopback_http_url(base_url)?;
    let path = normalize_daemon_path(path.to_owned())?;
    let authorization = daemon_auth_token()?
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let mut stream = TcpStream::connect_timeout(
        &loopback_socket_addr(&host, port)?,
        LOOM_DAEMON_CONNECT_TIMEOUT,
    )
    .map_err(|error| format!("无法连接 Loom 本地服务 {base_url}：{error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("无法设置 Loom 本地服务读取超时：{error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("无法设置 Loom 本地服务写入超时：{error}"))?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: image/*\r\n{authorization}Connection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("无法写入 Loom 本地服务请求 {path}：{error}"))?;

    let mut raw = read_bounded_http_response(&mut stream, &path, MAX_DAEMON_BINARY_RESPONSE_BYTES)?;
    let parsed = parse_http_response(&raw, &path, MAX_DAEMON_BINARY_RESPONSE_BYTES)?;
    if !(200..=299).contains(&parsed.status_code) {
        return Err(format!("{path} returned {}", parsed.status_line));
    }
    let content_type = detect_raster_content_type(&raw[parsed.body_offset..])
        .ok_or_else(|| format!("Loom 本地服务没有返回受支持的栅格图像：{path}"))?
        .to_owned();
    let body = raw.split_off(parsed.body_offset);
    Ok((content_type, body))
}

pub(super) fn detect_raster_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
    } else if is_avif(bytes) {
        Some("image/avif")
    } else {
        None
    }
}

fn is_avif(bytes: &[u8]) -> bool {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let box_len = u32::from_be_bytes(bytes[..4].try_into().expect("four-byte ftyp size")) as usize;
    if !(16..=bytes.len()).contains(&box_len) {
        return false;
    }
    [&bytes[8..12]]
        .into_iter()
        .chain(bytes[16..box_len].chunks_exact(4))
        .any(|brand| brand == b"avif" || brand == b"avis")
}

// Minimal standard base64 encoder so the desktop wrapper can return `data:` URLs
// without adding a dependency.
pub(super) fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub(super) fn normalize_daemon_path(path: String) -> Result<String, String> {
    let path = path.trim();
    if !path.starts_with('/') || path.contains('\r') || path.contains('\n') || path.contains("..") {
        return Err("Loom 本地服务 API 路径必须是绝对本地路径。".to_string());
    }
    Ok(path.to_string())
}

pub(super) fn parse_loopback_http_url(base_url: &str) -> Result<(String, u16), String> {
    let without_scheme = base_url
        .strip_prefix("http://")
        .ok_or_else(|| "Loom 本地服务地址必须使用 http:// 回环地址。".to_string())?;
    let authority = without_scheme.strip_suffix('/').unwrap_or(without_scheme);
    if authority.is_empty()
        || authority.contains('/')
        || authority.contains('?')
        || authority.contains('#')
        || authority.contains('@')
    {
        return Err("Loom 本地服务地址只能包含回环主机和端口。".to_owned());
    }
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| "Loom 本地服务地址必须包含端口。".to_string())?;
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return Err("Loom 桌面端只连接回环地址上的本地服务。".to_string());
    }
    let port = port
        .parse::<u16>()
        .map_err(|error| format!("Loom 本地服务端口无效：{error}"))?;
    if port == 0 {
        return Err("Loom 本地服务端口不能为 0。".to_owned());
    }
    Ok((host.trim_matches(&['[', ']'][..]).to_string(), port))
}

pub(super) fn loopback_socket_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    let ip = match host {
        "127.0.0.1" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "::1" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => return Err("Loom 桌面端只连接回环地址上的本地服务。".to_string()),
    };
    Ok(SocketAddr::new(ip, port))
}
