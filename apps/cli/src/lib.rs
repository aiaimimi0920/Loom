//! Lightweight HTTP command-line client for a local or remote `loom-daemon` hub.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

mod mcp_stdio;

const DAEMON_AUTH_TOKEN_FILE: &str = "daemon-token";
const DEFAULT_CLI_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const MAX_CLI_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

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
        ["mcp-stdio"] => {
            let effective_token = options.mcp_token.as_deref();
            mcp_stdio::run_mcp_stdio_bridge(
                std::io::stdin().lock(),
                writer,
                &options.daemon_url,
                effective_token,
            )?;
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
        "  mcp-stdio              Bridge stdio MCP JSON-RPC to Loom's outbound /mcp server\n",
        "  ai status              Read global AI Gateway relay status\n",
        "\n",
        "Options:\n",
        "  --daemon-url <URL>     Daemon URL [default: LOOM_DAEMON_URL or http://127.0.0.1:8765]\n",
        "  --daemon-token <TOKEN> Bearer token [loopback only default: LOOM_DAEMON_TOKEN or token file]\n",
        "  --mcp-token <TOKEN>    Scoped MCP share token [default: LOOM_MCP_SHARE_TOKEN]\n",
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
    mcp_token: Option<String>,
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
        let mut explicit_daemon_token = None;
        let mut mcp_token = std::env::var("LOOM_MCP_SHARE_TOKEN")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| normalize_daemon_auth_token(&v, "LOOM_MCP_SHARE_TOKEN"))
            .transpose()?;
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
                    explicit_daemon_token = Some(raw.clone());
                }
                "--mcp-token" => {
                    index += 1;
                    let raw = args
                        .get(index)
                        .ok_or_else(|| anyhow!("--mcp-token requires a value"))?;
                    mcp_token = if raw.trim().is_empty() {
                        None
                    } else {
                        Some(normalize_daemon_auth_token(raw, "--mcp-token")?)
                    };
                }
                "--json" => {
                    raw_json = true;
                }
                value => command.push(value.to_owned()),
            }
            index += 1;
        }

        let daemon_token = select_daemon_auth_token(
            &daemon_url,
            explicit_daemon_token.as_deref(),
            default_daemon_auth_token,
        )?;
        Ok(Self {
            command,
            daemon_url,
            daemon_token,
            mcp_token,
            raw_json,
        })
    }
}

fn select_daemon_auth_token(
    base_url: &str,
    explicit: Option<&str>,
    load_local: impl FnOnce() -> Result<Option<String>>,
) -> Result<Option<String>> {
    let endpoint = parse_http_url(base_url)?;
    if let Some(raw) = explicit {
        return if raw.trim().is_empty() {
            Ok(None)
        } else {
            normalize_daemon_auth_token(raw, "--daemon-token").map(Some)
        };
    }
    if endpoint.host.eq_ignore_ascii_case("localhost")
        || endpoint
            .host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
    {
        load_local()
    } else {
        Ok(None)
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
mod cli_tests;
