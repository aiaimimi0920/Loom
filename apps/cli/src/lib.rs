//! Lightweight HTTP command-line client for a local or remote `loom-daemon` hub.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

const DAEMON_AUTH_TOKEN_FILE: &str = "daemon-token";
const DEFAULT_CLI_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CLI_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub fn run_cli_with_writer<I, S, W>(args: I, writer: &mut W) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    W: Write,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect();
    if has_flag(&args, &["--help", "-h"]) {
        writer.write_all(cli_help_text().as_bytes())?;
        return Ok(());
    }
    if has_flag(&args, &["--version", "-V"]) {
        writeln!(writer, "loom {}", loom_core::LOOM_VERSION)?;
        return Ok(());
    }

    let options = CliOptions::parse(&args)?;
    let command: Vec<&str> = options.command.iter().map(String::as_str).collect();
    match command.as_slice() {
        ["status"] => {
            let response = http_get(
                &options.daemon_url,
                "/status",
                options.daemon_token.as_deref(),
            )?;
            writeln!(writer, "{response}")?;
        }
        ["health"] => {
            let response = http_get(&options.daemon_url, "/health", None)?;
            writeln!(writer, "{response}")?;
        }
        ["frameworks", "list"] => {
            let body = http_get(
                &options.daemon_url,
                "/v1/frameworks",
                options.daemon_token.as_deref(),
            )?;
            if options.raw_json {
                writeln!(writer, "{body}")?;
            } else {
                let parsed: Value =
                    serde_json::from_str(&body).context("parse /v1/frameworks JSON")?;
                if let Some(items) = parsed.get("frameworks").and_then(Value::as_array) {
                    for item in items {
                        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                        let version = item.get("version").and_then(Value::as_str).unwrap_or("-");
                        let ready = item.get("ready").and_then(Value::as_bool).unwrap_or(false);
                        writeln!(writer, "{id}\t{version}\tready={ready}")?;
                    }
                }
            }
        }
        ["arts", "list"] => {
            let body = http_get(
                &options.daemon_url,
                "/v1/tools",
                options.daemon_token.as_deref(),
            )?;
            if options.raw_json {
                writeln!(writer, "{body}")?;
            } else {
                let parsed: Value = serde_json::from_str(&body).context("parse /v1/tools JSON")?;
                if let Some(items) = parsed.get("tools").and_then(Value::as_array) {
                    for item in items {
                        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                        let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
                        let enabled = item
                            .get("enabled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        writeln!(writer, "{id}\t{name}\tenabled={enabled}")?;
                    }
                }
            }
        }
        ["workflows", "list"] => {
            let body = http_get(
                &options.daemon_url,
                "/v1/workflows",
                options.daemon_token.as_deref(),
            )?;
            if options.raw_json {
                writeln!(writer, "{body}")?;
            } else {
                let parsed: Value =
                    serde_json::from_str(&body).context("parse /v1/workflows JSON")?;
                if let Some(items) = parsed.get("workflows").and_then(Value::as_array) {
                    for item in items {
                        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                        writeln!(writer, "{id}")?;
                    }
                }
            }
        }
        ["mcp", "list"] => {
            let body = http_get(
                &options.daemon_url,
                "/v1/mcp/servers",
                options.daemon_token.as_deref(),
            )?;
            if options.raw_json {
                writeln!(writer, "{body}")?;
            } else {
                let parsed: Value =
                    serde_json::from_str(&body).context("parse /v1/mcp/servers JSON")?;
                if let Some(items) = parsed.get("servers").and_then(Value::as_array) {
                    for item in items {
                        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                        let enabled = item
                            .get("enabled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        writeln!(writer, "{id}\tenabled={enabled}")?;
                    }
                }
            }
        }
        ["ai", "status"] => {
            let response = http_get(
                &options.daemon_url,
                "/v1/ai/status",
                options.daemon_token.as_deref(),
            )?;
            writeln!(writer, "{response}")?;
        }
        [] => bail!("missing command"),
        command => bail!("unsupported command `{}`", command.join(" ")),
    }

    Ok(())
}

fn has_flag(args: &[String], flags: &[&str]) -> bool {
    args.iter().skip(1).any(|arg| flags.contains(&arg.as_str()))
}

fn cli_help_text() -> &'static str {
    concat!(
        "Usage: loom <COMMAND> [OPTIONS]\n",
        "\n",
        "Commands:\n",
        "  status                 Read loom-daemon runtime status\n",
        "  health                 Read public loom-daemon health probe\n",
        "  frameworks list        List installed Art execution frameworks\n",
        "  arts list              List registered Arts\n",
        "  workflows list         List stored Otter/Art workflows\n",
        "  mcp list               List configured MCP servers\n",
        "  ai status              Read global AI Gateway relay status\n",
        "\n",
        "Options:\n",
        "  --daemon-url <URL>     Daemon URL [default: LOOM_DAEMON_URL or http://127.0.0.1:8765]\n",
        "  --daemon-token <TOKEN> Bearer token [default: LOOM_DAEMON_TOKEN or control-plane token file]\n",
        "  --json                 Print raw JSON response for list commands\n",
        "  -h, --help             Print help\n",
        "  -V, --version          Print version\n",
    )
}

#[derive(Debug)]
struct CliOptions {
    command: Vec<String>,
    daemon_url: String,
    daemon_token: Option<String>,
    raw_json: bool,
}

fn normalize_daemon_auth_token(value: &str, source: &str) -> Result<String> {
    let token = value.trim();
    if token.is_empty() {
        bail!("Loom daemon auth token from {source} is empty");
    }
    if token.len() > 4096 || token.bytes().any(|byte| byte <= b' ' || byte == 0x7f) {
        bail!("Loom daemon auth token from {source} has an invalid format");
    }
    Ok(token.to_owned())
}

fn default_control_plane_root() -> PathBuf {
    std::env::var_os("LOOM_CONTROL_PLANE_ROOT")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.join("Loom").join("control-plane"))
        })
        .unwrap_or_else(|| PathBuf::from(".runtime").join("loom").join("control-plane"))
}

fn default_daemon_auth_token() -> Result<Option<String>> {
    if let Some(token) = std::env::var_os("LOOM_DAEMON_TOKEN").filter(|token| !token.is_empty()) {
        return normalize_daemon_auth_token(&token.to_string_lossy(), "LOOM_DAEMON_TOKEN")
            .map(Some);
    }
    let path = default_control_plane_root().join(DAEMON_AUTH_TOKEN_FILE);
    match fs::read_to_string(&path) {
        Ok(token) => normalize_daemon_auth_token(
            &token,
            &format!("persisted token file `{}`", path.display()),
        )
        .map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error)
            .with_context(|| format!("read Loom daemon auth token file `{}`", path.display())),
    }
}

impl CliOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut command = Vec::new();
        let mut daemon_url = std::env::var("LOOM_DAEMON_URL")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "http://127.0.0.1:8765".to_owned());
        let mut daemon_token = default_daemon_auth_token()?;
        let mut raw_json = false;
        let mut index = 1;

        while index < args.len() {
            match args[index].as_str() {
                "--daemon-url" => {
                    index += 1;
                    daemon_url = args
                        .get(index)
                        .ok_or_else(|| anyhow!("--daemon-url requires a value"))?
                        .clone();
                }
                "--daemon-token" => {
                    index += 1;
                    let raw = args
                        .get(index)
                        .ok_or_else(|| anyhow!("--daemon-token requires a value"))?;
                    daemon_token = if raw.trim().is_empty() {
                        None
                    } else {
                        Some(normalize_daemon_auth_token(raw, "--daemon-token")?)
                    };
                }
                "--json" => {
                    raw_json = true;
                }
                value => command.push(value.to_owned()),
            }
            index += 1;
        }

        Ok(Self {
            command,
            daemon_url,
            daemon_token,
            raw_json,
        })
    }
}

fn http_get(base_url: &str, path: &str, bearer_token: Option<&str>) -> Result<String> {
    let endpoint = parse_http_url(base_url)?;
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .with_context(|| format!("connect daemon at {base_url}"))?;
    stream.set_read_timeout(Some(DEFAULT_CLI_TIMEOUT))?;
    stream.set_write_timeout(Some(DEFAULT_CLI_TIMEOUT))?;
    let auth_header = bearer_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let host_header = if endpoint.host.contains(':') && !endpoint.host.starts_with('[') {
        format!("[{}]:{}", endpoint.host, endpoint.port)
    } else {
        format!("{}:{}", endpoint.host, endpoint.port)
    };
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host_header}\r\n{auth_header}Connection: close\r\n\r\n",
    )?;
    let mut response_bytes = Vec::new();
    stream
        .take((MAX_CLI_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut response_bytes)?;
    if response_bytes.len() > MAX_CLI_RESPONSE_BYTES {
        bail!("daemon response exceeded {MAX_CLI_RESPONSE_BYTES} bytes");
    }
    let response = String::from_utf8(response_bytes).context("daemon response is not UTF-8")?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("malformed daemon response"))?;
    if !head.starts_with("HTTP/1.1 200") {
        bail!("daemon returned non-200 response: {head}");
    }
    Ok(body.to_owned())
}

struct HttpEndpoint {
    host: String,
    port: u16,
}

fn parse_http_url(base_url: &str) -> Result<HttpEndpoint> {
    let authority = base_url
        .trim()
        .strip_prefix("http://")
        .ok_or_else(|| anyhow!("daemon URL must use http://"))?
        .split('/')
        .next()
        .ok_or_else(|| anyhow!("daemon URL is missing authority"))?;
    if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, rest) = bracketed
            .split_once(']')
            .ok_or_else(|| anyhow!("invalid IPv6 daemon authority"))?;
        let port = match rest.strip_prefix(':') {
            Some(port) => port.parse::<u16>()?,
            None if rest.is_empty() => 80,
            None => bail!("invalid IPv6 daemon port"),
        };
        return Ok(HttpEndpoint {
            host: host.to_owned(),
            port,
        });
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host.to_owned(), port.parse::<u16>()?),
        None => (authority.to_owned(), 80),
    };
    if host.trim().is_empty() {
        bail!("daemon URL host is empty");
    }
    Ok(HttpEndpoint { host, port })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn help_and_version_do_not_require_a_daemon() {
        let mut help = Vec::new();
        run_cli_with_writer(["loom", "--help"], &mut help).expect("help command");
        let help = String::from_utf8(help).expect("help utf8");
        assert!(help.contains("Usage: loom"));
        assert!(help.contains("frameworks list"));
        assert!(help.contains("arts list"));
        assert!(help.contains("workflows list"));
        assert!(help.contains("ai status"));

        let mut version = Vec::new();
        run_cli_with_writer(["loom", "--version"], &mut version).expect("version command");
        let version = String::from_utf8(version).expect("version utf8");
        assert_eq!(version.trim(), format!("loom {}", loom_core::LOOM_VERSION));
    }

    #[test]
    fn status_command_reads_daemon_status_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept status request");
            let request = read_http_request(&mut socket);
            assert!(
                request.starts_with("GET /status HTTP/1.1"),
                "unexpected request: {request:?}"
            );
            let body = r#"{"status":"ready","modules":[{"name":"core","initialized":true}]}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let mut output = Vec::new();
        run_cli_with_writer(
            [
                "loom",
                "status",
                "--daemon-url",
                &format!("http://{address}"),
            ],
            &mut output,
        )
        .expect("status command");

        server.join().expect("mock server");
        let output = String::from_utf8(output).expect("utf8 output");
        assert!(output.contains("ready"));
        assert!(output.contains("core"));
    }

    #[test]
    fn status_command_sends_bearer_token_when_configured() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept status request");
            let request = read_http_request(&mut socket);
            assert!(
                request.contains("Authorization: Bearer test-token\r\n"),
                "missing authorization header: {request:?}"
            );
            let body = r#"{"status":"ready","modules":[]}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let mut output = Vec::new();
        run_cli_with_writer(
            [
                "loom",
                "status",
                "--daemon-url",
                &format!("http://{address}"),
                "--daemon-token",
                "test-token",
            ],
            &mut output,
        )
        .expect("status command");

        server.join().expect("mock server");
        let output = String::from_utf8(output).expect("utf8 output");
        assert!(output.contains("ready"));
    }

    #[test]
    fn list_commands_query_daemon_control_plane_endpoints() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            for (expected_path, body) in [
                (
                    "GET /v1/frameworks HTTP/1.1",
                    r#"{"frameworks":[{"id":"process","version":"1.0.0","ready":true}]}"#,
                ),
                (
                    "GET /v1/workflows HTTP/1.1",
                    r#"{"workflows":[{"id":"image-blend-workflow"}]}"#,
                ),
            ] {
                let (mut socket, _) = listener.accept().expect("accept list request");
                let request = read_http_request(&mut socket);
                assert!(request.starts_with(expected_path), "got {request:?}");
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .expect("write list response");
            }
        });

        let url = format!("http://{address}");
        let mut frameworks_out = Vec::new();
        run_cli_with_writer(
            ["loom", "frameworks", "list", "--daemon-url", &url],
            &mut frameworks_out,
        )
        .expect("frameworks list");
        assert!(String::from_utf8(frameworks_out)
            .expect("utf8")
            .contains("process\t1.0.0\tready=true"));

        let mut workflows_out = Vec::new();
        run_cli_with_writer(
            ["loom", "workflows", "list", "--daemon-url", &url],
            &mut workflows_out,
        )
        .expect("workflows list");
        assert_eq!(
            String::from_utf8(workflows_out).expect("utf8").trim(),
            "image-blend-workflow"
        );

        server.join().expect("mock server");
    }

    fn read_http_request(socket: &mut impl Read) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 64];
        loop {
            let bytes = socket.read(&mut buffer).expect("read request");
            if bytes == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..bytes]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(request).expect("utf8 request")
    }
}
