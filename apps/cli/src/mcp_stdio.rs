//! Stdio-to-HTTP MCP bridge (`loom mcp-stdio`) so desktop MCP hosts can consume Loom directly.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use crate::{parse_http_url, MAX_CLI_RESPONSE_BYTES};

const MCP_STDIO_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) fn run_mcp_stdio_bridge<R, W>(
    reader: R,
    writer: &mut W,
    daemon_url: &str,
    bearer_token: Option<&str>,
) -> Result<()>
where
    R: Read,
    W: Write,
{
    let buffered = BufReader::new(reader);
    for line_result in buffered.lines() {
        let raw_line = line_result.context("read MCP stdio line")?;
        let trimmed = raw_line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        let (status, response_body) =
            post_mcp_jsonrpc(daemon_url, bearer_token, trimmed.as_bytes())?;
        if status == 202 || response_body.trim().is_empty() {
            continue;
        }
        if !(200..300).contains(&status) {
            bail!("Loom /mcp returned HTTP {status}: {response_body}");
        }
        writeln!(writer, "{}", response_body.trim())?;
        writer.flush()?;
    }
    Ok(())
}

fn post_mcp_jsonrpc(
    base_url: &str,
    bearer_token: Option<&str>,
    body: &[u8],
) -> Result<(u16, String)> {
    let endpoint = parse_http_url(base_url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .with_context(|| format!("connect Loom MCP server at {base_url}"))?;
    stream.set_read_timeout(Some(MCP_STDIO_REQUEST_TIMEOUT))?;
    stream.set_write_timeout(Some(MCP_STDIO_REQUEST_TIMEOUT))?;

    let auth_header = bearer_token
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let host_header = if endpoint.host.contains(':') && !endpoint.host.starts_with('[') {
        format!("[{}]:{}", endpoint.host, endpoint.port)
    } else {
        format!("{}:{}", endpoint.host, endpoint.port)
    };
    let mut request_bytes = format!(
        "POST /mcp HTTP/1.1\r\nHost: {host_header}\r\n{auth_header}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request_bytes.extend_from_slice(body);
    stream.write_all(&request_bytes)?;
    stream.flush()?;

    let mut response_bytes = Vec::new();
    stream
        .take((MAX_CLI_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut response_bytes)?;
    if response_bytes.len() > MAX_CLI_RESPONSE_BYTES {
        bail!("Loom /mcp response exceeded {MAX_CLI_RESPONSE_BYTES} bytes");
    }
    let response = String::from_utf8(response_bytes).context("Loom /mcp response is not UTF-8")?;
    let (head, resp_body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("malformed Loom /mcp HTTP response"))?;
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| anyhow!("missing HTTP status in Loom /mcp response: {head}"))?;
    Ok((status, resp_body.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn mcp_stdio_forwards_jsonrpc_and_skips_202_notifications() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock mcp server");
        let addr = listener.local_addr().expect("addr");
        let server = thread::spawn(move || {
            for idx in 0..2 {
                let (mut sock, _) = listener.accept().expect("accept");
                let mut acc = Vec::new();
                let mut chunk = [0u8; 256];
                while !acc.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = sock.read(&mut chunk).expect("read");
                    if n == 0 {
                        break;
                    }
                    acc.extend_from_slice(&chunk[..n]);
                }
                let text = String::from_utf8_lossy(&acc);
                assert!(text.starts_with("POST /mcp HTTP/1.1\r\n"));
                assert!(
                    text.contains("Authorization: Bearer loom_mcp_test_secret_token_123456\r\n")
                );
                if idx == 0 {
                    let body =
                        r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18"}}"#;
                    write!(
                        sock,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                } else {
                    write!(
                        sock,
                        "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                }
            }
        });

        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n"
        );
        let mut out = Vec::new();
        run_mcp_stdio_bridge(
            Cursor::new(input.as_bytes()),
            &mut out,
            &format!("http://{addr}"),
            Some("loom_mcp_test_secret_token_123456"),
        )
        .expect("bridge succeeds");
        server.join().expect("server thread");

        let rendered = String::from_utf8(out).expect("utf8");
        assert_eq!(
            rendered.trim(),
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18"}}"#
        );
    }
}
