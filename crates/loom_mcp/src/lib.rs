//! MCP server configuration and JSON-RPC request contracts for Loom.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

const MCP_REGISTRY_ENDPOINT: &str = "https://registry.modelcontextprotocol.io/v0/servers";

/// Version of the MCP crate.
pub const LOOM_MCP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Error)]
pub enum McpError {
    #[error("MCP registry cursor/search contained unsupported input")]
    InvalidRegistryQuery,
    #[error("failed to start MCP process `{command}`: {source}")]
    ProcessStart {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("MCP process did not expose {pipe}")]
    MissingPipe { pipe: &'static str },
    #[error("MCP stdio error: {0}")]
    Io(#[from] std::io::Error),
    #[error("MCP JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("MCP server returned JSON-RPC error: {0}")]
    JsonRpc(JsonValue),
    #[error("MCP protocol error: {0}")]
    Protocol(String),
}

pub type McpResult<T> = Result<T, McpError>;

/// User-configured MCP server definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SpawnCommandSpec {
    program: String,
    args: Vec<String>,
}

impl SpawnCommandSpec {
    fn direct(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

impl McpServerConfig {
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            command: command.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            enabled: true,
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

fn spawn_command_spec(config: &McpServerConfig) -> SpawnCommandSpec {
    #[cfg(windows)]
    if let Some(spec) = resolve_windows_spawn_command(config) {
        return spec;
    }

    SpawnCommandSpec::direct(config.command.clone(), config.args.clone())
}

#[cfg(windows)]
fn resolve_windows_spawn_command(config: &McpServerConfig) -> Option<SpawnCommandSpec> {
    let command = Path::new(&config.command);

    if is_windows_powershell_script(command) {
        return Some(windows_powershell_spawn_spec(command, &config.args));
    }

    if command.extension().is_some() {
        return None;
    }

    let resolved = resolve_windows_command_path(command)?;
    if is_windows_powershell_script(&resolved) {
        return Some(windows_powershell_spawn_spec(&resolved, &config.args));
    }

    Some(SpawnCommandSpec::direct(
        resolved.display().to_string(),
        config.args.clone(),
    ))
}

#[cfg(windows)]
fn resolve_windows_command_path(command: &Path) -> Option<PathBuf> {
    let extensions = windows_path_extensions();

    if is_windows_path_qualified(command) {
        return resolve_windows_command_in_paths(command, &[], &extensions);
    }

    let search_paths = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    resolve_windows_command_in_paths(command, &search_paths, &extensions)
}

#[cfg(windows)]
fn resolve_windows_command_in_paths(
    command: &Path,
    search_paths: &[PathBuf],
    extensions: &[String],
) -> Option<PathBuf> {
    if command.extension().is_some() {
        return None;
    }

    if is_windows_path_qualified(command) {
        return resolve_windows_command_candidates(command, extensions);
    }

    search_paths.iter().find_map(|search_path| {
        resolve_windows_command_candidates(&search_path.join(command), extensions)
    })
}

#[cfg(windows)]
fn resolve_windows_command_candidates(command: &Path, extensions: &[String]) -> Option<PathBuf> {
    extensions
        .iter()
        .map(|extension| append_windows_extension(command, extension))
        .find(|candidate| candidate.is_file())
}

#[cfg(windows)]
fn windows_path_extensions() -> Vec<String> {
    let mut extensions = std::env::var_os("PATHEXT")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_windows_extension)
        .collect::<Vec<_>>();

    if extensions.is_empty() {
        extensions = [".com", ".exe", ".bat", ".cmd"]
            .into_iter()
            .map(str::to_owned)
            .collect();
    }

    if !extensions
        .iter()
        .any(|value| value.eq_ignore_ascii_case(".ps1"))
    {
        extensions.push(".ps1".to_owned());
    }

    extensions
}

#[cfg(windows)]
fn normalize_windows_extension(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('.') {
        trimmed.to_ascii_lowercase()
    } else {
        format!(".{}", trimmed.to_ascii_lowercase())
    }
}

#[cfg(windows)]
fn append_windows_extension(command: &Path, extension: &str) -> PathBuf {
    let mut candidate = command.as_os_str().to_os_string();
    candidate.push(extension);
    PathBuf::from(candidate)
}

#[cfg(windows)]
fn is_windows_path_qualified(command: &Path) -> bool {
    command.is_absolute()
        || command
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty())
}

#[cfg(windows)]
fn is_windows_powershell_script(command: &Path) -> bool {
    command
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("ps1"))
}

#[cfg(windows)]
fn windows_powershell_spawn_spec(command: &Path, args: &[String]) -> SpawnCommandSpec {
    let mut command_args = vec![
        "-NoProfile".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-File".to_owned(),
        command.display().to_string(),
    ];
    command_args.extend(args.iter().cloned());
    SpawnCommandSpec::direct("powershell.exe", command_args)
}

/// Build the official MCP Registry URL using the same limit bounds as ArtLoom.
pub fn build_registry_url(
    search: Option<&str>,
    limit: Option<u32>,
    cursor: Option<&str>,
) -> McpResult<String> {
    let safe_limit = limit.unwrap_or(60).clamp(1, 100);
    let mut pairs = vec![format!("limit={safe_limit}")];

    if let Some(search_text) = search.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push(format!("search={}", percent_encode(search_text)));
    }

    if let Some(cursor_text) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
        pairs.push(format!("cursor={}", percent_encode(cursor_text)));
    }

    Ok(format!("{MCP_REGISTRY_ENDPOINT}?{}", pairs.join("&")))
}

#[must_use]
pub fn initialize_request(id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "Loom",
                "version": LOOM_MCP_VERSION
            }
        }
    })
}

#[must_use]
pub fn initialized_notification() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
}

#[must_use]
pub fn tools_list_request(id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/list",
        "params": {}
    })
}

#[must_use]
pub fn tools_call_request(id: u64, name: &str, arguments: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments
        }
    })
}

/// Synchronous stdio MCP JSON-RPC client.
pub struct StdioMcpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl StdioMcpClient {
    pub fn spawn(config: &McpServerConfig) -> McpResult<Self> {
        let spawn_spec = spawn_command_spec(config);
        let mut command = Command::new(&spawn_spec.program);
        command
            .args(&spawn_spec.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|source| McpError::ProcessStart {
            command: config.command.clone(),
            source,
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or(McpError::MissingPipe { pipe: "stdin" })?;
        let stdout = child
            .stdout
            .take()
            .ok_or(McpError::MissingPipe { pipe: "stdout" })?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    pub fn initialize(&mut self) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.write_message(&initialize_request(id))?;
        let result = self.read_result(id)?;
        self.write_message(&initialized_notification())?;
        Ok(result)
    }

    pub fn list_tools(&mut self) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.write_message(&tools_list_request(id))?;
        self.read_result(id)
    }

    pub fn call_tool(&mut self, name: &str, arguments: JsonValue) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.write_message(&tools_call_request(id, name, arguments))?;
        self.read_result(id)
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn write_message(&mut self, message: &JsonValue) -> McpResult<()> {
        serde_json::to_writer(&mut self.stdin, message)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn read_result(&mut self, expected_id: u64) -> McpResult<JsonValue> {
        loop {
            let mut line = String::new();
            let bytes = self.stdout.read_line(&mut line)?;
            if bytes == 0 {
                return Err(McpError::Protocol(format!(
                    "MCP process closed stdout before response id {expected_id}"
                )));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(message) = serde_json::from_str::<JsonValue>(trimmed) else {
                continue;
            };

            if message.get("id") != Some(&serde_json::json!(expected_id)) {
                continue;
            }

            if let Some(error) = message.get("error") {
                return Err(McpError::JsonRpc(error.clone()));
            }

            return message.get("result").cloned().ok_or_else(|| {
                McpError::Protocol(format!("MCP response id {expected_id} missing result"))
            });
        }
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, Write};

    #[test]
    fn registry_url_encodes_search_limit_and_cursor() {
        let url = build_registry_url(
            Some("brave search"),
            Some(250),
            Some("ai.example/server:1.0.0"),
        )
        .expect("registry url");

        assert_eq!(
            url,
            "https://registry.modelcontextprotocol.io/v0/servers?limit=100&search=brave%20search&cursor=ai.example%2Fserver%3A1.0.0"
        );
    }

    #[test]
    fn registry_url_omits_blank_search_and_cursor() {
        let url = build_registry_url(Some("   "), Some(0), Some(" "))
            .expect("registry url without optional terms");

        assert_eq!(
            url,
            "https://registry.modelcontextprotocol.io/v0/servers?limit=1"
        );
    }

    #[test]
    fn server_config_defaults_enabled() {
        let config = McpServerConfig::new("brave", "Brave Search", "npx")
            .arg("-y")
            .arg("@brave/brave-search-mcp-server")
            .env("BRAVE_API_KEY", "test-key");

        assert_eq!(config.id, "brave");
        assert_eq!(config.name, "Brave Search");
        assert_eq!(config.command, "npx");
        assert_eq!(config.args, vec!["-y", "@brave/brave-search-mcp-server"]);
        assert_eq!(
            config.env.get("BRAVE_API_KEY").map(String::as_str),
            Some("test-key")
        );
        assert!(config.enabled);
    }

    #[test]
    fn tools_list_request_uses_mcp_json_rpc_shape() {
        let request = tools_list_request(2);

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "tools/list");
        assert_eq!(request["params"], serde_json::json!({}));
    }

    #[test]
    fn tools_call_request_embeds_tool_name_and_arguments() {
        let request = tools_call_request(
            3,
            "brave_web_search",
            serde_json::json!({
                "query": "loom mcp",
                "count": 3
            }),
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 3);
        assert_eq!(request["method"], "tools/call");
        assert_eq!(request["params"]["name"], "brave_web_search");
        assert_eq!(request["params"]["arguments"]["query"], "loom mcp");
        assert_eq!(request["params"]["arguments"]["count"], 3);
    }

    #[test]
    fn initialize_request_identifies_loom_client() {
        let request = initialize_request(1);

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 1);
        assert_eq!(request["method"], "initialize");
        assert_eq!(request["params"]["protocolVersion"], "2024-11-05");
        assert_eq!(request["params"]["clientInfo"]["name"], "Loom");
    }

    #[test]
    fn stdio_client_initializes_and_lists_tools_against_fixture_server() {
        let config = current_test_binary_fixture_config();
        let mut client = StdioMcpClient::spawn(&config).expect("spawn fixture MCP server");

        let init = client.initialize().expect("initialize MCP fixture");
        let tools = client.list_tools().expect("list fixture MCP tools");

        assert_eq!(init["serverInfo"]["name"], "loom-fixture");
        assert_eq!(init["serverInfo"]["version"], "0.1.0");
        assert_eq!(tools["tools"][0]["name"], "echo");
        assert_eq!(tools["tools"][0]["description"], "Echo arguments");
    }

    #[test]
    fn stdio_client_calls_fixture_tool_and_returns_structured_content() {
        let config = current_test_binary_fixture_config();
        let mut client = StdioMcpClient::spawn(&config).expect("spawn fixture MCP server");

        client.initialize().expect("initialize MCP fixture");
        let result = client
            .call_tool("echo", serde_json::json!({ "text": "hello loom" }))
            .expect("call echo tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "hello loom");
    }

    #[cfg(windows)]
    #[test]
    fn stdio_client_spawns_extensionless_windows_cmd_fixture() {
        let fixture = windows_cmd_fixture_config();
        let mut client =
            StdioMcpClient::spawn(&fixture).expect("spawn extensionless cmd MCP fixture");

        let init = client.initialize().expect("initialize MCP fixture");
        let tools = client.list_tools().expect("list fixture MCP tools");

        assert_eq!(init["serverInfo"]["name"], "loom-fixture");
        assert_eq!(tools["tools"][0]["name"], "echo");
    }

    #[cfg(windows)]
    #[test]
    fn resolve_windows_command_in_paths_prefers_cmd_wrapper_for_bare_command() {
        let temp_root = unique_test_temp_dir("resolve-path");
        std::fs::create_dir_all(&temp_root).expect("create path resolution temp dir");

        let command_base = temp_root.join("npx");
        std::fs::write(command_base.with_extension("ps1"), "Write-Host ignored")
            .expect("write ps1 candidate");
        std::fs::write(command_base.with_extension("cmd"), "@echo off\r\n")
            .expect("write cmd candidate");

        let resolved = resolve_windows_command_in_paths(
            Path::new("npx"),
            &[temp_root],
            &[".cmd".to_owned(), ".ps1".to_owned()],
        )
        .expect("resolve command candidate");

        assert_eq!(resolved, command_base.with_extension("cmd"));
    }

    #[cfg(windows)]
    #[test]
    fn resolve_windows_spawn_command_wraps_powershell_scripts() {
        let config = McpServerConfig::new("fixture-ps1", "Fixture PS1", r"C:\loom\fixture.ps1")
            .arg("--flag");

        let spawn_spec =
            resolve_windows_spawn_command(&config).expect("resolve powershell spawn wrapper");

        assert_eq!(spawn_spec.program, "powershell.exe");
        assert_eq!(
            spawn_spec.args,
            vec![
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                r"C:\loom\fixture.ps1",
                "--flag",
            ]
        );
    }

    #[test]
    fn mcp_fixture_server() {
        if std::env::var("LOOM_MCP_FIXTURE_SERVER").ok().as_deref() != Some("1") {
            return;
        }

        run_mcp_fixture_server();
        std::process::exit(0);
    }

    fn current_test_binary_fixture_config() -> McpServerConfig {
        let exe = std::env::current_exe().expect("current test executable");
        McpServerConfig::new("fixture", "Fixture MCP", exe.display().to_string())
            .arg("tests::mcp_fixture_server")
            .arg("--exact")
            .arg("--nocapture")
            .env("LOOM_MCP_FIXTURE_SERVER", "1")
    }

    fn run_mcp_fixture_server() {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        for line in stdin.lock().lines() {
            let line = line.expect("fixture stdin line");
            let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let method = request["method"].as_str().unwrap_or_default();
            match method {
                "initialize" => write_fixture_response(
                    &mut stdout,
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"].clone(),
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": { "tools": {} },
                            "serverInfo": {
                                "name": "loom-fixture",
                                "version": "0.1.0"
                            }
                        }
                    }),
                ),
                "notifications/initialized" => {}
                "tools/list" => write_fixture_response(
                    &mut stdout,
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"].clone(),
                        "result": {
                            "tools": [
                                {
                                    "name": "echo",
                                    "description": "Echo arguments",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "text": { "type": "string" }
                                        }
                                    }
                                }
                            ]
                        }
                    }),
                ),
                "tools/call" => {
                    let text = request["params"]["arguments"]["text"]
                        .as_str()
                        .unwrap_or_default();
                    write_fixture_response(
                        &mut stdout,
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request["id"].clone(),
                            "result": {
                                "content": [
                                    {
                                        "type": "text",
                                        "text": text
                                    }
                                ]
                            }
                        }),
                    );
                }
                _ => write_fixture_response(
                    &mut stdout,
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"].clone(),
                        "error": {
                            "code": -32601,
                            "message": format!("unknown method {method}")
                        }
                    }),
                ),
            }
        }
    }

    fn write_fixture_response(stdout: &mut impl Write, response: serde_json::Value) {
        writeln!(
            stdout,
            "\n{}",
            serde_json::to_string(&response).expect("serialize fixture response")
        )
        .expect("write fixture response");
        stdout.flush().expect("flush fixture response");
    }

    #[cfg(windows)]
    fn windows_cmd_fixture_config() -> McpServerConfig {
        let temp_root = unique_test_temp_dir("fixture");
        std::fs::create_dir_all(&temp_root).expect("create MCP fixture temp dir");

        let command_base = temp_root.join("loom-mcp-fixture");
        let script_path = command_base.with_extension("cmd");
        let current_exe = std::env::current_exe().expect("current test executable");
        let script = format!(
            "@echo off\r\nset LOOM_MCP_FIXTURE_SERVER=1\r\n\"{}\" tests::mcp_fixture_server --exact --nocapture\r\n",
            current_exe.display()
        );
        std::fs::write(&script_path, script).expect("write MCP fixture cmd wrapper");

        McpServerConfig::new(
            "fixture-cmd",
            "Fixture MCP CMD",
            command_base.display().to_string(),
        )
    }

    #[cfg(windows)]
    fn unique_test_temp_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "loom-mcp-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("fixture timestamp")
                .as_nanos()
        ))
    }
}
