//! MCP server configuration and JSON-RPC request contracts for Loom.

pub mod package;

use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use loom_protocol::PackageTrustStatus;
use loom_security::network::{
    apply_runtime_proxy_async, host_is_loopback_literal, validate_outbound_url, OutboundPolicy,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::redirect::Policy as RedirectPolicy;
use reqwest::Client as HttpClient;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

const MCP_REGISTRY_ENDPOINT: &str = "https://registry.modelcontextprotocol.io/v0.1/servers";
pub const MCP_SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2026-07-28", "2025-06-18", "2024-11-05"];
pub const MCP_PREFERRED_PROTOCOL_VERSION: &str = MCP_SUPPORTED_PROTOCOL_VERSIONS[0];

pub const MAX_MCP_SERVER_NAME_BYTES: usize = 256;
pub const MAX_MCP_SERVER_DESCRIPTION_BYTES: usize = 4 * 1024;
pub const MAX_MCP_ARGUMENTS: usize = 128;
pub const MAX_MCP_ARGUMENT_BYTES: usize = 8 * 1024;
pub const MAX_MCP_TOOLS: usize = 256;
pub const MAX_MCP_TOOL_ID_BYTES: usize = 128;
pub const MAX_MCP_CREDENTIALS: usize = 64;
pub const MAX_MCP_CREDENTIAL_LABEL_BYTES: usize = 256;
pub const MAX_MCP_ENVIRONMENT_ENTRIES: usize = 128;
pub const MAX_MCP_ENVIRONMENT_NAME_BYTES: usize = 128;
pub const MAX_MCP_ENVIRONMENT_VALUE_BYTES: usize = 32 * 1024;
pub const MAX_MCP_ENVIRONMENT_TOTAL_BYTES: usize = 24 * 1024;
pub const MAX_MCP_HEADERS: usize = 64;
pub const MAX_MCP_HEADER_NAME_BYTES: usize = 128;
pub const MAX_MCP_HEADER_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_MCP_HEADER_TOTAL_BYTES: usize = 128 * 1024;

/// Version of the MCP crate.
pub const LOOM_MCP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Error)]
pub enum McpError {
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
    #[error("MCP process supervision failed for `{command}`: {reason}")]
    ProcessSupervision { command: String, reason: String },
    #[error("MCP request timed out after {timeout_ms}ms; stderr: {stderr}")]
    Timeout { timeout_ms: u128, stderr: String },
    #[error("MCP response exceeded the {limit} byte message limit")]
    OutputLimit { limit: usize },
    #[error("MCP process exited with code {code:?}; stderr: {stderr}")]
    ProcessExited { code: Option<i32>, stderr: String },
    #[error("MCP server `{server_id}` is disabled")]
    Disabled { server_id: String },
    #[error("invalid MCP configuration: {0}")]
    InvalidConfig(String),
    #[error("MCP server package integrity check failed: {0}")]
    PackageIntegrity(String),
    #[error("MCP transport `{0}` is not supported")]
    UnsupportedTransport(String),
    #[error("MCP HTTP request failed: {0}")]
    Http(String),
    #[error("MCP HTTP endpoint returned status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("MCP request was cancelled")]
    Cancelled,
}

pub type McpResult<T> = Result<T, McpError>;

/// Transport used to connect to a configured MCP server.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransport {
    #[default]
    Stdio,
    StreamableHttp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCredentialRequirement {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerPackageState {
    pub qualified_id: String,
    pub publisher_id: String,
    pub version: String,
    pub digest: String,
    pub package_dir: PathBuf,
    /// SHA-256 of every file extracted at install, keyed by package-relative path with `/`
    /// separators. Checked against the package's `active.json` before a stdio server is spawned.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
    /// What the install-time trust check concluded about this package's signature. Defaults to
    /// `Unsigned`, which is also what a package installed before signatures existed reports.
    #[serde(default)]
    pub trust_status: PackageTrustStatus,
}

impl McpTransport {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable-http",
        }
    }
}

/// User-configured MCP server definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_bindings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_requirements: Vec<McpCredentialRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<McpServerPackageState>,
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
            transport: McpTransport::Stdio,
            url: String::new(),
            headers: BTreeMap::new(),
            credential_env: BTreeMap::new(),
            credential_headers: BTreeMap::new(),
            credential_bindings: BTreeMap::new(),
            credential_requirements: Vec::new(),
            tools: Vec::new(),
            package: None,
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
    pub fn remote(id: impl Into<String>, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            transport: McpTransport::StreamableHttp,
            url: url.into(),
            headers: BTreeMap::new(),
            credential_env: BTreeMap::new(),
            credential_headers: BTreeMap::new(),
            credential_bindings: BTreeMap::new(),
            credential_requirements: Vec::new(),
            tools: Vec::new(),
            package: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn validate(&self) -> McpResult<()> {
        validate_server_metadata_and_limits(self)?;
        match self.transport {
            McpTransport::Stdio if self.command.trim().is_empty() => Err(McpError::InvalidConfig(
                "stdio command is required".to_owned(),
            )),
            McpTransport::StreamableHttp => validate_remote_config(self),
            McpTransport::Stdio => validate_stdio_command(&self.command),
        }
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

fn validate_server_metadata_and_limits(config: &McpServerConfig) -> McpResult<()> {
    validate_config_identity("server id", &config.id)?;
    validate_bounded_config_text("server name", &config.name, MAX_MCP_SERVER_NAME_BYTES, true)?;
    validate_bounded_config_text(
        "server description",
        &config.description,
        MAX_MCP_SERVER_DESCRIPTION_BYTES,
        false,
    )?;
    validate_string_list(
        "entry.args",
        &config.args,
        MAX_MCP_ARGUMENTS,
        MAX_MCP_ARGUMENT_BYTES,
        false,
    )?;
    if config.tools.len() > MAX_MCP_TOOLS {
        return Err(McpError::InvalidConfig(format!(
            "tools contains {} entries; limit is {MAX_MCP_TOOLS}",
            config.tools.len()
        )));
    }
    for (index, tool) in config.tools.iter().enumerate() {
        validate_mcp_tool_identifier(&format!("tools[{index}]"), tool)?;
    }
    if config.credential_requirements.len() > MAX_MCP_CREDENTIALS {
        return Err(McpError::InvalidConfig(format!(
            "credentialRequirements contains {} entries; limit is {MAX_MCP_CREDENTIALS}",
            config.credential_requirements.len()
        )));
    }
    let mut credential_ids = std::collections::BTreeSet::new();
    for (index, requirement) in config.credential_requirements.iter().enumerate() {
        validate_config_identity(
            &format!("credentialRequirements[{index}].id"),
            &requirement.id,
        )?;
        validate_bounded_config_text(
            &format!("credentialRequirements[{index}].label"),
            &requirement.label,
            MAX_MCP_CREDENTIAL_LABEL_BYTES,
            true,
        )?;
        if !credential_ids.insert(requirement.id.as_str()) {
            return Err(McpError::InvalidConfig(format!(
                "duplicate credential requirement id `{}`",
                requirement.id
            )));
        }
    }
    validate_mcp_environment(&config.env)?;
    validate_mcp_headers(&config.headers)?;
    validate_credential_target_names(config)?;
    if let Some(package) = &config.package {
        validate_installed_package_state(config, package)?;
    }
    Ok(())
}

fn validate_bounded_config_text(
    field: &str,
    value: &str,
    max_bytes: usize,
    required: bool,
) -> McpResult<()> {
    if required && value.trim().is_empty() {
        return Err(McpError::InvalidConfig(format!("{field} is required")));
    }
    if value.len() > max_bytes {
        return Err(McpError::InvalidConfig(format!(
            "{field} is {} bytes; limit is {max_bytes}",
            value.len()
        )));
    }
    Ok(())
}

fn validate_string_list(
    field: &str,
    values: &[String],
    max_entries: usize,
    max_value_bytes: usize,
    require_nonempty: bool,
) -> McpResult<()> {
    if values.len() > max_entries {
        return Err(McpError::InvalidConfig(format!(
            "{field} contains {} entries; limit is {max_entries}",
            values.len()
        )));
    }
    for (index, value) in values.iter().enumerate() {
        validate_bounded_config_text(
            &format!("{field}[{index}]"),
            value,
            max_value_bytes,
            require_nonempty,
        )?;
    }
    Ok(())
}

fn validate_config_identity(field: &str, value: &str) -> McpResult<()> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(McpError::InvalidConfig(format!(
            "{field} is not a safe identifier (limit 128 bytes)"
        )))
    }
}

pub fn validate_mcp_tool_identifier(field: &str, value: &str) -> McpResult<()> {
    let valid = !value.is_empty()
        && value.len() <= MAX_MCP_TOOL_ID_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        });
    if valid {
        Ok(())
    } else {
        Err(McpError::InvalidConfig(format!(
            "{field} must be a non-empty MCP tool identifier using at most {MAX_MCP_TOOL_ID_BYTES} bytes"
        )))
    }
}

pub fn validate_mcp_environment(environment: &BTreeMap<String, String>) -> McpResult<()> {
    if environment.len() > MAX_MCP_ENVIRONMENT_ENTRIES {
        return Err(McpError::InvalidConfig(format!(
            "env contains {} entries; limit is {MAX_MCP_ENVIRONMENT_ENTRIES}",
            environment.len()
        )));
    }
    let mut total_bytes = 0usize;
    for (name, value) in environment {
        validate_mcp_environment_name(name)?;
        if value.len() > MAX_MCP_ENVIRONMENT_VALUE_BYTES {
            return Err(McpError::InvalidConfig(format!(
                "env.{name} is {} bytes; limit is {MAX_MCP_ENVIRONMENT_VALUE_BYTES}",
                value.len()
            )));
        }
        total_bytes = total_bytes.saturating_add(name.len() + value.len() + 2);
    }
    if total_bytes > MAX_MCP_ENVIRONMENT_TOTAL_BYTES {
        return Err(McpError::InvalidConfig(format!(
            "env is {total_bytes} aggregate bytes; limit is {MAX_MCP_ENVIRONMENT_TOTAL_BYTES}"
        )));
    }
    Ok(())
}

pub fn validate_mcp_environment_name(name: &str) -> McpResult<()> {
    let mut bytes = name.bytes();
    let valid = name.len() <= MAX_MCP_ENVIRONMENT_NAME_BYTES
        && bytes
            .next()
            .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric());
    if !valid {
        return Err(McpError::InvalidConfig(format!(
            "invalid MCP environment variable name `{name}` (limit {MAX_MCP_ENVIRONMENT_NAME_BYTES} bytes)"
        )));
    }
    let normalized = name.to_ascii_uppercase();
    if matches!(
        normalized.as_str(),
        "PATH"
            | "PATHEXT"
            | "COMSPEC"
            | "SYSTEMROOT"
            | "WINDIR"
            | "TEMP"
            | "TMP"
            | "LD_PRELOAD"
            | "LD_LIBRARY_PATH"
            | "DYLD_INSERT_LIBRARIES"
            | "DYLD_LIBRARY_PATH"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "SHELLOPTS"
            | "PS4"
            | "PROMPT_COMMAND"
    ) {
        return Err(McpError::InvalidConfig(format!(
            "MCP environment variable `{name}` is managed or process-influencing and may not be overridden"
        )));
    }
    Ok(())
}

pub fn is_managed_mcp_header_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "accept"
            | "connection"
            | "content-length"
            | "content-type"
            | "host"
            | "mcp-protocol-version"
            | "mcp-session-id"
            | "origin"
            | "transfer-encoding"
    )
}

pub fn validate_mcp_header_name(name: &str) -> McpResult<()> {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.is_empty() || name.len() > MAX_MCP_HEADER_NAME_BYTES {
        return Err(McpError::InvalidConfig(format!(
            "remote MCP header name `{name}` is empty or exceeds {MAX_MCP_HEADER_NAME_BYTES} bytes"
        )));
    }
    if is_managed_mcp_header_name(&normalized) {
        return Err(McpError::InvalidConfig(format!(
            "remote MCP header `{name}` is managed by Loom"
        )));
    }
    HeaderName::from_bytes(normalized.as_bytes()).map_err(|error| {
        McpError::InvalidConfig(format!("invalid remote MCP header `{name}`: {error}"))
    })?;
    Ok(())
}

pub fn validate_mcp_headers(headers: &BTreeMap<String, String>) -> McpResult<()> {
    if headers.len() > MAX_MCP_HEADERS {
        return Err(McpError::InvalidConfig(format!(
            "headers contains {} entries; limit is {MAX_MCP_HEADERS}",
            headers.len()
        )));
    }
    let mut total_bytes = 0usize;
    for (name, value) in headers {
        validate_mcp_header_name(name)?;
        if value.len() > MAX_MCP_HEADER_VALUE_BYTES {
            return Err(McpError::InvalidConfig(format!(
                "header `{name}` value is {} bytes; limit is {MAX_MCP_HEADER_VALUE_BYTES}",
                value.len()
            )));
        }
        HeaderValue::from_str(value).map_err(|error| {
            McpError::InvalidConfig(format!(
                "invalid value for remote MCP header `{name}`: {error}"
            ))
        })?;
        total_bytes = total_bytes.saturating_add(name.len() + value.len() + 4);
    }
    if total_bytes > MAX_MCP_HEADER_TOTAL_BYTES {
        return Err(McpError::InvalidConfig(format!(
            "headers are {total_bytes} aggregate bytes; limit is {MAX_MCP_HEADER_TOTAL_BYTES}"
        )));
    }
    Ok(())
}

fn validate_credential_target_names(config: &McpServerConfig) -> McpResult<()> {
    if config.credential_env.len() > MAX_MCP_ENVIRONMENT_ENTRIES {
        return Err(McpError::InvalidConfig(format!(
            "credentialEnv contains {} entries; limit is {MAX_MCP_ENVIRONMENT_ENTRIES}",
            config.credential_env.len()
        )));
    }
    for (name, credential_id) in &config.credential_env {
        validate_mcp_environment_name(name)?;
        validate_config_identity("credentialEnv credential id", credential_id)?;
    }
    if config.credential_headers.len() > MAX_MCP_HEADERS {
        return Err(McpError::InvalidConfig(format!(
            "credentialHeaders contains {} entries; limit is {MAX_MCP_HEADERS}",
            config.credential_headers.len()
        )));
    }
    for (name, credential_id) in &config.credential_headers {
        validate_mcp_header_name(name)?;
        validate_config_identity("credentialHeaders credential id", credential_id)?;
    }
    Ok(())
}

fn validate_installed_package_state(
    config: &McpServerConfig,
    package: &McpServerPackageState,
) -> McpResult<()> {
    validate_config_identity("package.publisherId", &package.publisher_id)?;
    let expected_qualified = format!("{}/{}", package.publisher_id, config.id);
    if package.qualified_id != expected_qualified {
        return Err(McpError::InvalidConfig(format!(
            "package.qualifiedId `{}` must equal `{expected_qualified}`",
            package.qualified_id
        )));
    }
    semver::Version::parse(&package.version).map_err(|error| {
        McpError::InvalidConfig(format!("package.version must be SemVer: {error}"))
    })?;
    if package.digest.len() != 64 || !package.digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(McpError::InvalidConfig(
            "package.digest must be a 64-character SHA-256 hex digest".to_owned(),
        ));
    }
    if !package.package_dir.is_absolute() {
        return Err(McpError::InvalidConfig(
            "package.packageDir must be absolute".to_owned(),
        ));
    }
    if package.files.len() > 4096 {
        return Err(McpError::InvalidConfig(format!(
            "package.files contains {} entries; limit is 4096",
            package.files.len()
        )));
    }
    for (path, digest) in &package.files {
        if path.is_empty()
            || Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(McpError::InvalidConfig(format!(
                "package.files path `{path}` is unsafe"
            )));
        }
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(McpError::InvalidConfig(format!(
                "package.files digest for `{path}` must be SHA-256 hex"
            )));
        }
    }
    Ok(())
}

/// Reject a stdio command whose meaning depends on where the daemon happens to have been started.
///
/// A path with a parent but no root — `runtime/server.exe`, `./server`, `..\server.exe` — is completed
/// by the process's current directory, so which binary runs is decided by the daemon's working
/// directory rather than by the configuration. A packaged server never needs one, since the installer
/// records an absolute path under the package directory, and an operator writing the row by hand can
/// say what they mean.
///
/// A bare program name is still allowed: that is a `PATH` lookup, which is how servers are normally
/// launched (`npx`, `node`, `python`), and taking it away would rule out most of the ecosystem.
fn validate_stdio_command(command: &str) -> McpResult<()> {
    let path = Path::new(command.trim());
    let is_bare_name = !path
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty());
    if path.is_absolute() || is_bare_name {
        return Ok(());
    }
    Err(McpError::InvalidConfig(format!(
        "stdio command `{command}` is a relative path, so which file runs depends on the daemon's \
         working directory; use an absolute path, or a bare program name to look up on PATH"
    )))
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

    // A packaged server runs the file the installer extracted and `verify_installed_entry` hashed.
    // Resolving an extensionless command here would search `PATHEXT`, and for a bare name `PATH`
    // too, which can only ever land on a file other than the verified one. Packaged servers get no
    // such search; the verifier rejects the extensionless command before it reaches this point.
    if config.package.is_some() {
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
    let value = std::env::var_os("PATHEXT");
    windows_path_extensions_from(value.as_deref())
}

#[cfg(windows)]
fn windows_path_extensions_from(value: Option<&std::ffi::OsStr>) -> Vec<String> {
    let extensions = value
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_windows_extension)
        .collect::<Vec<_>>();

    if extensions.is_empty() {
        [".com", ".exe", ".bat", ".cmd"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        extensions
    }
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

/// Build the official MCP Registry URL using bounded pagination.
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

    pairs.push("version=latest".to_owned());
    Ok(format!("{MCP_REGISTRY_ENDPOINT}?{}", pairs.join("&")))
}

#[must_use]
pub fn initialize_request(id: u64) -> serde_json::Value {
    initialize_request_for_version(id, MCP_PREFERRED_PROTOCOL_VERSION)
}

#[must_use]
pub fn initialize_request_for_version(id: u64, protocol_version: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": protocol_version,
            "capabilities": {},
            "clientInfo": {
                "name": "Loom",
                "version": LOOM_MCP_VERSION
            }
        }
    })
}

fn validate_initialize_result(result: &JsonValue) -> McpResult<String> {
    let object = result.as_object().ok_or_else(|| {
        McpError::Protocol("MCP initialize result must be a JSON object".to_owned())
    })?;
    let protocol_version = object
        .get("protocolVersion")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .ok_or_else(|| {
            McpError::Protocol(
                "MCP initialize result requires a protocolVersion of at most 64 bytes".to_owned(),
            )
        })?;
    if !MCP_SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version) {
        return Err(McpError::Protocol(format!(
            "MCP server selected unsupported protocolVersion `{protocol_version}`; Loom supports {}",
            MCP_SUPPORTED_PROTOCOL_VERSIONS.join(", ")
        )));
    }
    if !object.get("capabilities").is_some_and(JsonValue::is_object) {
        return Err(McpError::Protocol(
            "MCP initialize result requires an object capabilities field".to_owned(),
        ));
    }
    let server_info = object
        .get("serverInfo")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| {
            McpError::Protocol("MCP initialize result requires serverInfo".to_owned())
        })?;
    for field in ["name", "version"] {
        let value = server_info
            .get(field)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 256)
            .ok_or_else(|| {
                McpError::Protocol(format!(
                    "MCP initialize serverInfo.{field} must be non-empty and at most 256 bytes"
                ))
            })?;
        if value.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(McpError::Protocol(format!(
                "MCP initialize serverInfo.{field} contains control characters"
            )));
        }
    }
    Ok(protocol_version.to_owned())
}

fn is_protocol_compatibility_rejection(error: &McpError) -> bool {
    let text = match error {
        McpError::JsonRpc(value) => value.to_string(),
        McpError::HttpStatus { body, .. } => body.clone(),
        _ => return false,
    }
    .to_ascii_lowercase();
    let names_protocol = text.contains("protocol") || text.contains("version");
    let rejects_version = text.contains("unsupported")
        || text.contains("not supported")
        || text.contains("incompatible");
    names_protocol && rejects_version
}

fn no_common_protocol_error(last_error: &McpError) -> McpError {
    McpError::Protocol(format!(
        "MCP server rejected every supported protocol revision ({}); last rejection: {last_error}",
        MCP_SUPPORTED_PROTOCOL_VERSIONS.join(", ")
    ))
}

/// Transport-neutral MCP client used by daemon and tool registry callers.
pub enum McpClient {
    Stdio(StdioMcpClient),
    StreamableHttp(StreamableHttpMcpClient),
}

impl McpClient {
    pub fn connect(config: &McpServerConfig) -> McpResult<Self> {
        Self::connect_with_timeout(
            config,
            Duration::from_secs(MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed)),
        )
    }

    pub fn connect_with_timeout(
        config: &McpServerConfig,
        request_timeout: Duration,
    ) -> McpResult<Self> {
        if !config.enabled {
            return Err(McpError::Disabled {
                server_id: config.id.clone(),
            });
        }
        config.validate()?;
        match config.transport {
            McpTransport::Stdio => {
                StdioMcpClient::spawn_with_timeout(config, request_timeout).map(Self::Stdio)
            }
            McpTransport::StreamableHttp => {
                StreamableHttpMcpClient::connect_with_timeout(config, request_timeout)
                    .map(Self::StreamableHttp)
            }
        }
    }

    pub fn initialize(&mut self) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.initialize(),
            Self::StreamableHttp(client) => client.initialize(),
        }
    }

    pub fn initialize_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.initialize_cancellable(cancellation),
            Self::StreamableHttp(client) => client.initialize_cancellable(cancellation),
        }
    }

    pub fn list_tools(&mut self) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.list_tools(),
            Self::StreamableHttp(client) => client.list_tools(),
        }
    }

    pub fn list_tools_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.list_tools_cancellable(cancellation),
            Self::StreamableHttp(client) => client.list_tools_cancellable(cancellation),
        }
    }

    pub fn call_tool(&mut self, name: &str, arguments: JsonValue) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.call_tool(name, arguments),
            Self::StreamableHttp(client) => client.call_tool(name, arguments),
        }
    }

    pub fn call_tool_cancellable(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: &AtomicBool,
    ) -> McpResult<JsonValue> {
        match self {
            Self::Stdio(client) => client.call_tool_cancellable(name, arguments, cancellation),
            Self::StreamableHttp(client) => {
                client.call_tool_cancellable(name, arguments, cancellation)
            }
        }
    }

    pub fn close(&mut self) -> McpResult<()> {
        match self {
            Self::Stdio(client) => client.close(),
            Self::StreamableHttp(client) => client.close(),
        }
    }

    pub fn close_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<()> {
        match self {
            Self::Stdio(client) => client.close(),
            Self::StreamableHttp(client) => client.close_cancellable(cancellation),
        }
    }

    pub fn cancel(&mut self) {
        match self {
            Self::Stdio(client) => client.cancel(),
            Self::StreamableHttp(client) => client.cancel(),
        }
    }
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
    process: loom_process::ManagedChild,
    stdin: std::process::ChildStdin,
    stdout: Receiver<StdoutEvent>,
    stderr: Arc<Mutex<BoundedStderr>>,
    request_timeout: Duration,
    next_id: u64,
}

const DEFAULT_MCP_REQUEST_TIMEOUT_SECONDS: u64 = 60;
const DEFAULT_MCP_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
const MCP_MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MCP_MAX_STDERR_BYTES: usize = 1024 * 1024;
static MCP_REQUEST_TIMEOUT_SECONDS: AtomicU64 = AtomicU64::new(DEFAULT_MCP_REQUEST_TIMEOUT_SECONDS);
static MCP_MEMORY_LIMIT_BYTES: AtomicU64 = AtomicU64::new(DEFAULT_MCP_MEMORY_LIMIT_BYTES);

/// Environment variable that lets an operator point a remote MCP server at their own machine.
const MCP_LOCAL_SERVERS_ENV: &str = "LOOM_MCP_ALLOW_LOCAL_SERVERS";
static MCP_ALLOW_LOCAL_SERVERS: AtomicBool = AtomicBool::new(false);

/// Applies process-wide defaults used by newly spawned MCP stdio clients.
pub fn configure_runtime_limits(request_timeout_seconds: u64, memory_limit_bytes: u64) {
    MCP_REQUEST_TIMEOUT_SECONDS.store(request_timeout_seconds.max(1), Ordering::Relaxed);
    MCP_MEMORY_LIMIT_BYTES.store(memory_limit_bytes.max(1), Ordering::Relaxed);
}

/// Allows remote MCP servers to address loopback and private networks.
///
/// This is off by default: a remote server URL can arrive from a package manifest that nobody
/// signed, and the credential headers configured for that server are attached to every request.
/// Pointing such a URL at `127.0.0.1`, at a LAN device, or at a cloud metadata endpoint turns
/// the daemon into a confused deputy, so those destinations require an explicit decision by the
/// operator, either through this call or by setting `LOOM_MCP_ALLOW_LOCAL_SERVERS=1`.
pub fn configure_local_servers(allowed: bool) {
    MCP_ALLOW_LOCAL_SERVERS.store(allowed, Ordering::Relaxed);
}

/// Report whether local and private destinations are currently allowed for remote MCP servers.
#[must_use]
pub fn local_servers_allowed() -> bool {
    MCP_ALLOW_LOCAL_SERVERS.load(Ordering::Relaxed) || environment_allows_local_servers()
}

fn environment_allows_local_servers() -> bool {
    std::env::var(MCP_LOCAL_SERVERS_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// The outbound policy applied to every remote MCP request.
///
/// Remote MCP servers get the same policy the Art and cloud paths use, with redirects refused
/// outright: Streamable HTTP has no use for them, and a redirect is the cheapest way to move a
/// request holding operator credentials from an allowed host to a forbidden one.
fn remote_outbound_policy(allow_local: bool) -> OutboundPolicy {
    OutboundPolicy {
        allow_http_loopback: allow_local,
        allow_private_networks: allow_local,
        allowed_domains: Vec::new(),
        max_redirects: 0,
    }
}

/// Reject a remote MCP URL whose scheme cannot protect what Loom is about to send.
///
/// This runs during configuration validation, so it must not perform a DNS lookup; the address
/// classes are checked in [`StreamableHttpMcpClient::connect_with_timeout`] instead, where a
/// lookup is already unavoidable. Keeping the two apart means saving a server never depends on
/// name resolution while connecting to one still refuses loopback, private and link-local
/// destinations.
fn ensure_remote_scheme_allowed(url: &Url, credentialed: bool, allow_local: bool) -> McpResult<()> {
    let host = url.host_str().unwrap_or_default();
    match url.scheme() {
        "https" => Ok(()),
        "http" if allow_local && host_is_loopback_literal(host) => Ok(()),
        "http" if credentialed => Err(McpError::InvalidConfig(format!(
            "remote MCP URL must use https because credential headers are attached; plain http \
             would send them in cleartext. Plain http is only accepted for a loopback \
             development endpoint, and only with {MCP_LOCAL_SERVERS_ENV}=1"
        ))),
        "http" => Err(McpError::InvalidConfig(format!(
            "remote MCP URL must use https; plain http is only accepted for a loopback \
             development endpoint, and only with {MCP_LOCAL_SERVERS_ENV}=1"
        ))),
        scheme => Err(McpError::InvalidConfig(format!(
            "remote MCP URL scheme `{scheme}` is not supported"
        ))),
    }
}

#[must_use]
pub fn runtime_limits() -> (u64, u64) {
    (
        MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed),
        MCP_MEMORY_LIMIT_BYTES.load(Ordering::Relaxed),
    )
}

enum StdoutEvent {
    Line(Vec<u8>),
    Eof,
    Error(String),
    Oversized,
}

#[derive(Default)]
struct BoundedStderr {
    bytes: Vec<u8>,
    total: u64,
}

impl BoundedStderr {
    fn text(&self) -> String {
        let mut text = String::from_utf8_lossy(&self.bytes).trim().to_owned();
        if self.total > self.bytes.len() as u64 {
            text.push_str(" [truncated]");
        }
        text
    }
}

impl StdioMcpClient {
    pub fn spawn(config: &McpServerConfig) -> McpResult<Self> {
        if !config.enabled {
            return Err(McpError::Disabled {
                server_id: config.id.clone(),
            });
        }
        config.validate()?;
        if config.transport != McpTransport::Stdio {
            return Err(McpError::UnsupportedTransport(
                config.transport.label().to_owned(),
            ));
        }
        Self::spawn_with_timeout(
            config,
            Duration::from_secs(MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed)),
        )
    }

    pub fn spawn_with_timeout(
        config: &McpServerConfig,
        request_timeout: Duration,
    ) -> McpResult<Self> {
        if !config.enabled {
            return Err(McpError::Disabled {
                server_id: config.id.clone(),
            });
        }
        config.validate()?;
        if config.transport != McpTransport::Stdio {
            return Err(McpError::UnsupportedTransport(
                config.transport.label().to_owned(),
            ));
        }
        // A package-backed server runs a file the installer extracted, and it runs it with the
        // user's credentials in its environment, so the digests recorded at install are checked
        // here rather than trusted from `servers.json`.
        crate::package::verify_installed_entry(config)
            .map_err(|error| McpError::PackageIntegrity(error.to_string()))?;
        let spawn_spec = spawn_command_spec(config);
        let mut process_spec = loom_process::ProcessSpec::new(&spawn_spec.program);
        process_spec.args = spawn_spec.args;
        process_spec.env = config.env.clone();
        process_spec.limits.timeout = request_timeout;
        process_spec.limits.stdout_bytes = MCP_MAX_MESSAGE_BYTES;
        process_spec.limits.stderr_bytes = MCP_MAX_STDERR_BYTES;
        process_spec.limits.memory_bytes = Some(
            usize::try_from(MCP_MEMORY_LIMIT_BYTES.load(Ordering::Relaxed)).unwrap_or(usize::MAX),
        );
        process_spec.limits.max_processes = Some(8);
        let (process, pipes) = match loom_process::ManagedChild::spawn(&process_spec) {
            Ok(value) => value,
            Err(loom_process::ProcessError::Spawn(source)) => {
                return Err(McpError::ProcessStart {
                    command: config.command.clone(),
                    source,
                })
            }
            Err(error) => {
                return Err(McpError::ProcessSupervision {
                    command: config.command.clone(),
                    reason: error.to_string(),
                })
            }
        };
        let (stdout_tx, stdout_rx) = mpsc::channel();
        thread::spawn(move || read_stdout_lines(pipes.stdout, stdout_tx));
        let stderr = Arc::new(Mutex::new(BoundedStderr::default()));
        let stderr_capture = Arc::clone(&stderr);
        thread::spawn(move || drain_stderr(pipes.stderr, stderr_capture));

        Ok(Self {
            process,
            stdin: pipes.stdin,
            stdout: stdout_rx,
            stderr,
            request_timeout,
            next_id: 1,
        })
    }

    pub fn initialize(&mut self) -> McpResult<JsonValue> {
        self.initialize_with_cancellation(None)
    }

    pub fn initialize_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        self.initialize_with_cancellation(Some(cancellation))
    }

    fn initialize_with_cancellation(
        &mut self,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        for (version_index, protocol_version) in
            MCP_SUPPORTED_PROTOCOL_VERSIONS.iter().copied().enumerate()
        {
            let id = self.next_request_id();
            self.write_message(&initialize_request_for_version(id, protocol_version))?;
            match self.read_result_with_cancellation(id, cancellation) {
                Ok(result) => {
                    validate_initialize_result(&result)?;
                    self.write_message(&initialized_notification())?;
                    return Ok(result);
                }
                Err(error)
                    if is_protocol_compatibility_rejection(&error)
                        && version_index + 1 < MCP_SUPPORTED_PROTOCOL_VERSIONS.len() => {}
                Err(error) if is_protocol_compatibility_rejection(&error) => {
                    return Err(no_common_protocol_error(&error));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("MCP supported protocol revision table is non-empty")
    }

    pub fn list_tools(&mut self) -> McpResult<JsonValue> {
        self.list_tools_with_cancellation(None)
    }

    pub fn list_tools_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        self.list_tools_with_cancellation(Some(cancellation))
    }

    fn list_tools_with_cancellation(
        &mut self,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.write_message(&tools_list_request(id))?;
        self.read_result_with_cancellation(id, cancellation)
    }

    pub fn call_tool(&mut self, name: &str, arguments: JsonValue) -> McpResult<JsonValue> {
        self.call_tool_with_cancellation(name, arguments, None)
    }

    pub fn call_tool_cancellable(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: &AtomicBool,
    ) -> McpResult<JsonValue> {
        self.call_tool_with_cancellation(name, arguments, Some(cancellation))
    }

    fn call_tool_with_cancellation(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.write_message(&tools_call_request(id, name, arguments))?;
        self.read_result_with_cancellation(id, cancellation)
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

    fn read_result_with_cancellation(
        &mut self,
        expected_id: u64,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let deadline = Instant::now() + self.request_timeout;
        loop {
            if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
                self.process.terminate();
                return Err(McpError::Cancelled);
            }
            let now = Instant::now();
            if now >= deadline {
                self.process.terminate();
                return Err(McpError::Timeout {
                    timeout_ms: self.request_timeout.as_millis(),
                    stderr: self.stderr_text(),
                });
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(25));
            let line = match self.stdout.recv_timeout(wait) {
                Ok(StdoutEvent::Line(line)) => line,
                Ok(StdoutEvent::Oversized) => {
                    self.process.terminate();
                    return Err(McpError::OutputLimit {
                        limit: MCP_MAX_MESSAGE_BYTES,
                    });
                }
                Ok(StdoutEvent::Error(error)) => return Err(McpError::Protocol(error)),
                Ok(StdoutEvent::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let code = self
                        .process
                        .try_wait()
                        .ok()
                        .flatten()
                        .and_then(|status| status.code());
                    return Err(McpError::ProcessExited {
                        code,
                        stderr: self.stderr_text(),
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
            };
            let trimmed = String::from_utf8_lossy(&line).trim().to_owned();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(message) = serde_json::from_str::<JsonValue>(&trimmed) else {
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

    pub fn cancel(&mut self) {
        self.process.terminate();
    }

    pub fn close(&mut self) -> McpResult<()> {
        self.process.terminate();
        Ok(())
    }

    fn stderr_text(&self) -> String {
        self.stderr
            .lock()
            .map(|stderr| stderr.text())
            .unwrap_or_else(|_| "stderr capture unavailable".to_owned())
    }
}

/// Synchronous MCP client for the standard Streamable HTTP transport.
pub struct StreamableHttpMcpClient {
    client: HttpClient,
    url: String,
    headers: HeaderMap,
    session_id: Option<String>,
    protocol_version: String,
    next_id: u64,
}

impl StreamableHttpMcpClient {
    pub fn connect(config: &McpServerConfig) -> McpResult<Self> {
        Self::connect_with_timeout(
            config,
            Duration::from_secs(MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed)),
        )
    }

    pub fn connect_with_timeout(
        config: &McpServerConfig,
        request_timeout: Duration,
    ) -> McpResult<Self> {
        if !config.enabled {
            return Err(McpError::Disabled {
                server_id: config.id.clone(),
            });
        }
        config.validate()?;
        if config.transport != McpTransport::StreamableHttp {
            return Err(McpError::UnsupportedTransport(
                config.transport.label().to_owned(),
            ));
        }

        let request_timeout = request_timeout.max(Duration::from_millis(1));
        let url = Url::parse(config.url.trim())
            .map_err(|error| McpError::InvalidConfig(format!("invalid remote MCP URL: {error}")))?;
        // `config.validate()` above rejected the schemes this policy would also reject, without
        // touching the network. The check here is the one that needs a lookup: it resolves the
        // host and refuses loopback, private, link-local and metadata addresses unless the
        // operator opted in. A hostile DNS answer can still change between this check and the
        // request, which is why redirects are refused as well.
        let policy = remote_outbound_policy(local_servers_allowed());
        validate_outbound_url(&url, &policy).map_err(|error| {
            McpError::InvalidConfig(format!(
                "remote MCP URL `{}` is not allowed: {error}",
                config.url.trim()
            ))
        })?;

        let builder = HttpClient::builder()
            .connect_timeout(request_timeout.min(Duration::from_secs(15)))
            .timeout(request_timeout)
            .redirect(RedirectPolicy::none());
        let client = apply_runtime_proxy_async(builder)
            .and_then(|builder| builder.build().map_err(|error| error.to_string()))
            .map_err(McpError::Http)?;
        let headers = build_remote_headers(&config.headers)?;

        Ok(Self {
            client,
            url: config.url.trim().to_owned(),
            headers,
            session_id: None,
            protocol_version: MCP_PREFERRED_PROTOCOL_VERSION.to_owned(),
            next_id: 1,
        })
    }

    pub fn initialize(&mut self) -> McpResult<JsonValue> {
        self.initialize_with_cancellation(None)
    }

    pub fn initialize_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        self.initialize_with_cancellation(Some(cancellation))
    }

    fn initialize_with_cancellation(
        &mut self,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        for (version_index, protocol_version) in
            MCP_SUPPORTED_PROTOCOL_VERSIONS.iter().copied().enumerate()
        {
            self.protocol_version = protocol_version.to_owned();
            let id = self.next_request_id();
            let request = initialize_request_for_version(id, protocol_version);
            match self.send_message_with_cancellation(&request, Some(id), cancellation) {
                Ok(result) => {
                    self.protocol_version = validate_initialize_result(&result)?;
                    self.send_message_with_cancellation(
                        &initialized_notification(),
                        None,
                        cancellation,
                    )?;
                    return Ok(result);
                }
                Err(error)
                    if is_protocol_compatibility_rejection(&error)
                        && version_index + 1 < MCP_SUPPORTED_PROTOCOL_VERSIONS.len() =>
                {
                    // A rejected initialize request must not create a session. Refuse to carry a
                    // non-conforming response header into the next revision attempt regardless.
                    self.session_id = None;
                }
                Err(error) if is_protocol_compatibility_rejection(&error) => {
                    self.session_id = None;
                    return Err(no_common_protocol_error(&error));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("MCP supported protocol revision table is non-empty")
    }

    pub fn list_tools(&mut self) -> McpResult<JsonValue> {
        self.list_tools_with_cancellation(None)
    }

    pub fn list_tools_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        self.list_tools_with_cancellation(Some(cancellation))
    }

    fn list_tools_with_cancellation(
        &mut self,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.send_message_with_cancellation(&tools_list_request(id), Some(id), cancellation)
    }

    pub fn call_tool(&mut self, name: &str, arguments: JsonValue) -> McpResult<JsonValue> {
        self.call_tool_with_cancellation(name, arguments, None)
    }

    pub fn call_tool_cancellable(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: &AtomicBool,
    ) -> McpResult<JsonValue> {
        self.call_tool_with_cancellation(name, arguments, Some(cancellation))
    }

    fn call_tool_with_cancellation(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.send_message_with_cancellation(
            &tools_call_request(id, name, arguments),
            Some(id),
            cancellation,
        )
    }

    pub fn cancel(&mut self) {
        // Streamable HTTP cancellation is request-scoped; the cancellable methods drop the
        // in-flight async request when their token fires. There is no child process to terminate.
    }

    pub fn close(&mut self) -> McpResult<()> {
        self.close_with_cancellation(None)
    }

    pub fn close_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<()> {
        self.close_with_cancellation(Some(cancellation))
    }

    fn close_with_cancellation(&mut self, cancellation: Option<&AtomicBool>) -> McpResult<()> {
        let Some(session_id) = self.session_id.as_deref() else {
            return Ok(());
        };
        let request = self
            .client
            .delete(&self.url)
            .header(ACCEPT, "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &self.protocol_version)
            .header("MCP-Session-Id", session_id)
            .headers(self.headers.clone());
        let response = run_http_future(execute_http_request(request, cancellation))
            .map_err(|error| McpError::Http(format!("run MCP HTTP close request: {error}")))??;
        let status = response.status;
        if status.is_success()
            || status == reqwest::StatusCode::NOT_FOUND
            || status == reqwest::StatusCode::METHOD_NOT_ALLOWED
        {
            self.session_id = None;
            return Ok(());
        }
        Err(McpError::HttpStatus {
            status: status.as_u16(),
            body: bounded_error_body(&response.body),
        })
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send_message_with_cancellation(
        &mut self,
        message: &JsonValue,
        expected_id: Option<u64>,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let mut request = self
            .client
            .post(&self.url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &self.protocol_version)
            .headers(self.headers.clone())
            .json(message);
        if let Some(session_id) = self.session_id.as_deref() {
            request = request.header("MCP-Session-Id", session_id);
        }

        let response = run_http_future(execute_http_request(request, cancellation))
            .map_err(|error| McpError::Http(format!("run MCP HTTP request: {error}")))??;
        if let Some(session_id) = response.session_id {
            self.session_id = Some(session_id);
        }
        let status = response.status;
        let content_type = response.content_type;
        let body = response.body;
        if !status.is_success() {
            return Err(McpError::HttpStatus {
                status: status.as_u16(),
                body: bounded_error_body(&body),
            });
        }
        if body.is_empty() {
            return expected_id.map_or_else(
                || Ok(JsonValue::Null),
                |id| {
                    Err(McpError::Protocol(format!(
                        "MCP HTTP response id {id} had an empty body"
                    )))
                },
            );
        }

        let messages = if content_type.contains("text/event-stream") {
            parse_sse_messages(&body)?
        } else {
            parse_json_messages(&body)?
        };
        match expected_id {
            Some(id) => result_from_messages(messages, id),
            None => Ok(JsonValue::Null),
        }
    }
}

fn run_http_future<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = T> + Send,
    T: Send,
{
    thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| format!("create runtime: {error}"))?;
                Ok(runtime.block_on(future))
            })
            .join()
            .map_err(|_| "HTTP runtime thread panicked".to_owned())?
    })
}

fn validate_remote_config(config: &McpServerConfig) -> McpResult<()> {
    let url = Url::parse(config.url.trim())
        .map_err(|error| McpError::InvalidConfig(format!("invalid remote MCP URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(McpError::InvalidConfig(
            "remote MCP URL must use http or https".to_owned(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(McpError::InvalidConfig(
            "remote MCP URL must not contain embedded credentials".to_owned(),
        ));
    }
    if url.host_str().is_none() || url.fragment().is_some() {
        return Err(McpError::InvalidConfig(
            "remote MCP URL must contain a host and no fragment".to_owned(),
        ));
    }
    if config.url.contains('{') || config.url.contains('}') {
        return Err(McpError::InvalidConfig(
            "remote MCP URL still contains unresolved template variables".to_owned(),
        ));
    }
    // Both maps mean secrets end up on the wire: `headers` holds the values that are sent, and
    // `credential_headers` names the vault entries the daemon resolves into them before a call.
    let credentialed = !config.headers.is_empty() || !config.credential_headers.is_empty();
    ensure_remote_scheme_allowed(&url, credentialed, local_servers_allowed())?;
    build_remote_headers(&config.headers).map(|_| ())
}

fn build_remote_headers(headers: &BTreeMap<String, String>) -> McpResult<HeaderMap> {
    validate_mcp_headers(headers)?;
    let mut result = HeaderMap::new();
    for (name, value) in headers {
        let normalized = name.trim().to_ascii_lowercase();
        let header_name = HeaderName::from_bytes(normalized.as_bytes())
            .expect("validated remote MCP header name");
        let header_value = HeaderValue::from_str(value).expect("validated remote MCP header value");
        result.insert(header_name, header_value);
    }
    Ok(result)
}

struct HttpWireResponse {
    status: reqwest::StatusCode,
    content_type: String,
    session_id: Option<String>,
    body: Vec<u8>,
}

async fn wait_for_cancellation(cancellation: &AtomicBool) {
    while !cancellation.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn execute_http_request(
    request: reqwest::RequestBuilder,
    cancellation: Option<&AtomicBool>,
) -> McpResult<HttpWireResponse> {
    let mut response = if let Some(cancellation) = cancellation {
        tokio::select! {
            response = request.send() => response,
            () = wait_for_cancellation(cancellation) => return Err(McpError::Cancelled),
        }
    } else {
        request.send().await
    }
    .map_err(|error| McpError::Http(error.to_string()))?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let session_id = response
        .headers()
        .get("MCP-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let body = read_bounded_http_body(&mut response, cancellation).await?;
    Ok(HttpWireResponse {
        status,
        content_type,
        session_id,
        body,
    })
}

async fn read_bounded_http_body(
    response: &mut reqwest::Response,
    cancellation: Option<&AtomicBool>,
) -> McpResult<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let chunk = if let Some(cancellation) = cancellation {
            tokio::select! {
                chunk = response.chunk() => chunk,
                () = wait_for_cancellation(cancellation) => return Err(McpError::Cancelled),
            }
        } else {
            response.chunk().await
        }
        .map_err(|error| McpError::Http(error.to_string()))?;
        let Some(chunk) = chunk else {
            break;
        };
        if body.len().saturating_add(chunk.len()) > MCP_MAX_MESSAGE_BYTES {
            return Err(McpError::OutputLimit {
                limit: MCP_MAX_MESSAGE_BYTES,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn bounded_error_body(body: &[u8]) -> String {
    const ERROR_BODY_LIMIT: usize = 2048;
    let visible = &body[..body.len().min(ERROR_BODY_LIMIT)];
    let mut text = String::from_utf8_lossy(visible).trim().to_owned();
    if body.len() > ERROR_BODY_LIMIT {
        text.push_str(" [truncated]");
    }
    text
}

fn parse_json_messages(body: &[u8]) -> McpResult<Vec<JsonValue>> {
    let value = serde_json::from_slice::<JsonValue>(body)?;
    Ok(match value {
        JsonValue::Array(messages) => messages,
        message => vec![message],
    })
}

fn parse_sse_messages(body: &[u8]) -> McpResult<Vec<JsonValue>> {
    let text = String::from_utf8_lossy(body);
    let mut messages = Vec::new();
    let mut data_lines = Vec::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            data_lines.clear();
            let value = serde_json::from_str::<JsonValue>(&data)?;
            match value {
                JsonValue::Array(values) => messages.extend(values),
                value => messages.push(value),
            }
        } else if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.strip_prefix(' ').unwrap_or(data));
        }
    }
    if messages.is_empty() {
        return Err(McpError::Protocol(
            "MCP SSE response did not contain a JSON data event".to_owned(),
        ));
    }
    Ok(messages)
}

fn result_from_messages(messages: Vec<JsonValue>, expected_id: u64) -> McpResult<JsonValue> {
    let expected = serde_json::json!(expected_id);
    let response = messages
        .into_iter()
        .find(|message| message.get("id") == Some(&expected))
        .ok_or_else(|| {
            McpError::Protocol(format!(
                "MCP HTTP response did not contain id {expected_id}"
            ))
        })?;
    if let Some(error) = response.get("error") {
        return Err(McpError::JsonRpc(error.clone()));
    }
    response.get("result").cloned().ok_or_else(|| {
        McpError::Protocol(format!("MCP HTTP response id {expected_id} missing result"))
    })
}

fn read_stdout_lines(mut stdout: std::process::ChildStdout, sender: mpsc::Sender<StdoutEvent>) {
    let mut buffer = [0u8; 16 * 1024];
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let read = match stdout.read(&mut buffer) {
            Ok(0) => {
                if oversized {
                    let _ = sender.send(StdoutEvent::Oversized);
                } else if !line.is_empty() {
                    let _ = sender.send(StdoutEvent::Line(line));
                }
                let _ = sender.send(StdoutEvent::Eof);
                return;
            }
            Ok(read) => read,
            Err(error) => {
                let _ = sender.send(StdoutEvent::Error(error.to_string()));
                return;
            }
        };
        for byte in &buffer[..read] {
            if *byte == b'\n' {
                let event = if oversized {
                    StdoutEvent::Oversized
                } else {
                    StdoutEvent::Line(std::mem::take(&mut line))
                };
                if sender.send(event).is_err() {
                    return;
                }
                line.clear();
                oversized = false;
            } else if line.len() < MCP_MAX_MESSAGE_BYTES {
                line.push(*byte);
            } else {
                oversized = true;
            }
        }
    }
}

fn drain_stderr(mut stderr: std::process::ChildStderr, capture: Arc<Mutex<BoundedStderr>>) {
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        let Ok(mut capture) = capture.lock() else {
            return;
        };
        capture.total = capture.total.saturating_add(read as u64);
        let remaining = MCP_MAX_STDERR_BYTES.saturating_sub(capture.bytes.len());
        capture
            .bytes
            .extend_from_slice(&buffer[..read.min(remaining)]);
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
    use std::net::{TcpListener, TcpStream};

    static PROCESS_CONFIG_LOCK: Mutex<()> = Mutex::new(());

    struct ProcessConfigTestGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        request_timeout_seconds: u64,
        memory_limit_bytes: u64,
        allow_local_servers: bool,
    }

    impl ProcessConfigTestGuard {
        fn capture() -> Self {
            let lock = PROCESS_CONFIG_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Self {
                _lock: lock,
                request_timeout_seconds: MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed),
                memory_limit_bytes: MCP_MEMORY_LIMIT_BYTES.load(Ordering::Relaxed),
                allow_local_servers: MCP_ALLOW_LOCAL_SERVERS.load(Ordering::Relaxed),
            }
        }
    }

    impl Drop for ProcessConfigTestGuard {
        fn drop(&mut self) {
            MCP_REQUEST_TIMEOUT_SECONDS.store(self.request_timeout_seconds, Ordering::Relaxed);
            MCP_MEMORY_LIMIT_BYTES.store(self.memory_limit_bytes, Ordering::Relaxed);
            MCP_ALLOW_LOCAL_SERVERS.store(self.allow_local_servers, Ordering::Relaxed);
        }
    }

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
            "https://registry.modelcontextprotocol.io/v0.1/servers?limit=100&search=brave%20search&cursor=ai.example%2Fserver%3A1.0.0&version=latest"
        );
    }

    #[test]
    fn registry_url_omits_blank_search_and_cursor() {
        let url = build_registry_url(Some("   "), Some(0), Some(" "))
            .expect("registry url without optional terms");

        assert_eq!(
            url,
            "https://registry.modelcontextprotocol.io/v0.1/servers?limit=1&version=latest"
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
    fn stdio_server_config_requires_explicit_transport() {
        assert!(
            serde_json::from_value::<McpServerConfig>(serde_json::json!({
                "id": "local",
                "name": "Local",
                "command": "npx",
                "args": ["-y", "local-mcp"]
            }))
            .is_err()
        );

        let config: McpServerConfig = serde_json::from_value(serde_json::json!({
            "id": "local",
            "name": "Local",
            "command": "npx",
            "args": ["-y", "local-mcp"],
            "transport": "stdio"
        }))
        .expect("explicit stdio MCP config");

        assert_eq!(config.transport, McpTransport::Stdio);
        assert!(config.url.is_empty());
        assert!(config.headers.is_empty());
        config.validate().expect("valid explicit stdio config");
    }

    #[test]
    fn a_relative_stdio_command_is_refused() {
        // A relative path is completed by the daemon's working directory, so the same configuration
        // starts different files depending on where the daemon was launched from.
        let relative = McpServerConfig::new("local", "Local", "runtime/server.exe");
        assert!(
            matches!(relative.validate(), Err(McpError::InvalidConfig(message))
                if message.contains("relative path")),
            "a relative stdio command must be refused"
        );
        assert!(
            McpServerConfig::new("local", "Local", "./server")
                .validate()
                .is_err(),
            "an explicitly current-directory command must be refused too"
        );

        // A bare name is a `PATH` lookup, which is how servers are normally launched, and an absolute
        // path says exactly what it means. Both stay valid.
        McpServerConfig::new("local", "Local", "npx")
            .validate()
            .expect("a bare program name is a PATH lookup");
        let absolute = if cfg!(windows) {
            r"C:\tools\server.exe"
        } else {
            "/usr/bin/server"
        };
        McpServerConfig::new("local", "Local", absolute)
            .validate()
            .expect("an absolute command is unambiguous");
    }

    #[test]
    fn remote_server_config_rejects_embedded_credentials_and_templates() {
        let embedded =
            McpServerConfig::remote("remote", "Remote", "https://user:secret@example.test/mcp");
        assert!(matches!(
            embedded.validate(),
            Err(McpError::InvalidConfig(_))
        ));

        let templated =
            McpServerConfig::remote("remote", "Remote", "https://{tenant}.example.test/mcp");
        assert!(matches!(
            templated.validate(),
            Err(McpError::InvalidConfig(_))
        ));
    }

    #[test]
    fn server_config_enforces_manifest_sized_fields_and_tool_identifiers() {
        let mut config = McpServerConfig::new("local", "Local", "npx");
        config.args = vec!["argument".to_owned(); MAX_MCP_ARGUMENTS + 1];
        assert!(matches!(
            config.validate(),
            Err(McpError::InvalidConfig(message)) if message.contains("entry.args") && message.contains("limit")
        ));

        let mut config = McpServerConfig::new("local", "Local", "npx");
        config.tools = vec!["invalid tool name".to_owned()];
        assert!(matches!(
            config.validate(),
            Err(McpError::InvalidConfig(message)) if message.contains("tools[0]")
        ));
    }

    #[test]
    fn process_environment_and_remote_headers_share_security_bounds() {
        let mut config = McpServerConfig::new("local", "Local", "npx");
        config.env.insert("PATH".to_owned(), "attacker".to_owned());
        assert!(matches!(
            config.validate(),
            Err(McpError::InvalidConfig(message)) if message.contains("process-influencing")
        ));

        let mut config = McpServerConfig::remote("remote", "Remote", "https://example.test/mcp");
        config
            .headers
            .insert("Host".to_owned(), "attacker".to_owned());
        assert!(matches!(
            config.validate(),
            Err(McpError::InvalidConfig(message)) if message.contains("managed by Loom")
        ));

        let oversized_environment = BTreeMap::from([(
            "SAFE_VALUE".to_owned(),
            "x".repeat(MAX_MCP_ENVIRONMENT_TOTAL_BYTES),
        )]);
        assert!(validate_mcp_environment(&oversized_environment)
            .unwrap_err()
            .to_string()
            .contains("aggregate bytes"));

        let oversized_credential_header = BTreeMap::from([(
            "X-Api-Key".to_owned(),
            "x".repeat(MAX_MCP_HEADER_VALUE_BYTES + 1),
        )]);
        assert!(validate_mcp_headers(&oversized_credential_header)
            .unwrap_err()
            .to_string()
            .contains("value"));
    }

    #[test]
    fn installed_package_state_is_revalidated_with_user_server_config() {
        let mut config = McpServerConfig::new("fixture", "Fixture", "npx");
        config.package = Some(McpServerPackageState {
            qualified_id: "publisher.test/other".to_owned(),
            publisher_id: "publisher.test".to_owned(),
            version: "1.0.0".to_owned(),
            digest: "a".repeat(64),
            package_dir: std::env::temp_dir().join("loom-mcp-fixture-package"),
            files: BTreeMap::new(),
            trust_status: PackageTrustStatus::Unsigned,
        });

        assert!(matches!(
            config.validate(),
            Err(McpError::InvalidConfig(message)) if message.contains("package.qualifiedId")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_extensions_do_not_implicitly_enable_powershell() {
        let extensions = windows_path_extensions_from(Some(std::ffi::OsStr::new(".EXE;.CMD")));
        assert_eq!(extensions, vec![".exe", ".cmd"]);
        assert!(!extensions.iter().any(|extension| extension == ".ps1"));

        let explicit = windows_path_extensions_from(Some(std::ffi::OsStr::new(".EXE;.PS1")));
        assert_eq!(explicit, vec![".exe", ".ps1"]);
    }

    #[test]
    fn remote_config_requires_https_unless_the_operator_allows_a_loopback_endpoint() {
        let public = Url::parse("http://mcp.example.test/mcp").expect("public URL");
        let error = ensure_remote_scheme_allowed(&public, true, false)
            .expect_err("credentialed plain http must be refused");
        assert!(
            error.to_string().contains("cleartext"),
            "unexpected error: {error}"
        );
        assert!(ensure_remote_scheme_allowed(&public, false, false).is_err());
        // Opting in covers the local machine only; a name that merely resolves to loopback is
        // controlled by whoever answers DNS, so it stays refused.
        assert!(ensure_remote_scheme_allowed(&public, false, true).is_err());

        let loopback = Url::parse("http://127.0.0.1:9000/mcp").expect("loopback URL");
        assert!(ensure_remote_scheme_allowed(&loopback, true, false).is_err());
        assert!(ensure_remote_scheme_allowed(&loopback, true, true).is_ok());

        let secure = Url::parse("https://mcp.example.test/mcp").expect("https URL");
        assert!(ensure_remote_scheme_allowed(&secure, true, false).is_ok());
    }

    #[test]
    fn remote_outbound_policy_refuses_local_private_and_metadata_addresses() {
        let policy = remote_outbound_policy(false);
        assert_eq!(
            policy.max_redirects, 0,
            "a redirect can move a credentialed request to a forbidden host"
        );
        for address in [
            "http://127.0.0.1:9200/",
            "https://127.0.0.1:9200/",
            "https://192.168.1.1/admin",
            "http://169.254.169.254/latest/meta-data/",
            "https://[fd00::1]/mcp",
        ] {
            let url = Url::parse(address).expect("test URL");
            assert!(
                validate_outbound_url(&url, &policy).is_err(),
                "{address} must be refused by default"
            );
        }

        let opted_in = remote_outbound_policy(true);
        for address in ["http://127.0.0.1:9200/", "https://192.168.1.1/admin"] {
            let url = Url::parse(address).expect("test URL");
            assert!(
                validate_outbound_url(&url, &opted_in).is_ok(),
                "{address} must be reachable once the operator opts in"
            );
        }
    }

    #[test]
    fn runtime_limits_can_be_updated_for_new_clients() {
        let _guard = ProcessConfigTestGuard::capture();
        configure_runtime_limits(30, 1024 * 1024 * 1024);
        assert_eq!(runtime_limits(), (30, 1024 * 1024 * 1024));
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
        assert_eq!(
            request["params"]["protocolVersion"],
            MCP_PREFERRED_PROTOCOL_VERSION
        );
        assert_eq!(request["params"]["clientInfo"]["name"], "Loom");
    }

    #[test]
    fn initialize_response_conformance_table_is_shared_by_both_transports() {
        for version in MCP_SUPPORTED_PROTOCOL_VERSIONS {
            let result = serde_json::json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "fixture", "version": "1.0.0" }
            });
            assert_eq!(
                validate_initialize_result(&result).unwrap(),
                *version,
                "supported revision {version}"
            );
        }

        let incompatible = serde_json::json!({
            "protocolVersion": "2099-01-01",
            "capabilities": {},
            "serverInfo": { "name": "fixture", "version": "1.0.0" }
        });
        let error = validate_initialize_result(&incompatible).unwrap_err();
        assert!(error.to_string().contains("unsupported protocolVersion"));
        assert!(error
            .to_string()
            .contains(MCP_SUPPORTED_PROTOCOL_VERSIONS[0]));

        for malformed in [
            serde_json::json!({
                "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
                "serverInfo": { "name": "fixture", "version": "1.0.0" }
            }),
            serde_json::json!({
                "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
                "capabilities": {},
                "serverInfo": { "name": "", "version": "1.0.0" }
            }),
        ] {
            assert!(validate_initialize_result(&malformed).is_err());
        }
    }

    #[test]
    fn stdio_client_initializes_and_lists_tools_against_fixture_server() {
        let _guard = ProcessConfigTestGuard::capture();
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
    fn stdio_initialize_falls_back_to_the_next_shared_protocol_revision() {
        let _guard = ProcessConfigTestGuard::capture();
        let config =
            current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "reject-preferred");
        let mut client = StdioMcpClient::spawn(&config).expect("spawn fallback MCP fixture");

        let initialized = client
            .initialize()
            .expect("fall back to legacy MCP revision");

        assert_eq!(
            initialized["protocolVersion"],
            MCP_SUPPORTED_PROTOCOL_VERSIONS[1]
        );
    }

    #[test]
    fn stdio_initialize_reports_bounded_no_common_revision_error() {
        let _guard = ProcessConfigTestGuard::capture();
        let config = current_test_binary_fixture_config()
            .env("LOOM_MCP_FIXTURE_MODE", "reject-all-protocols");
        let mut client = StdioMcpClient::spawn(&config).expect("spawn incompatible MCP fixture");

        let error = client
            .initialize()
            .expect_err("all rejected revisions must fail");

        let message = error.to_string();
        assert!(message.contains("rejected every supported protocol revision"));
        for version in MCP_SUPPORTED_PROTOCOL_VERSIONS {
            assert!(message.contains(version));
        }
        assert!(message.len() < 1024);
    }

    #[test]
    fn stdio_client_calls_fixture_tool_and_returns_structured_content() {
        let _guard = ProcessConfigTestGuard::capture();
        let config = current_test_binary_fixture_config();
        let mut client = StdioMcpClient::spawn(&config).expect("spawn fixture MCP server");

        client.initialize().expect("initialize MCP fixture");
        let result = client
            .call_tool("echo", serde_json::json!({ "text": "hello loom" }))
            .expect("call echo tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "hello loom");
    }

    #[test]
    fn streamable_http_client_initializes_lists_and_calls_tools() {
        let _guard = ProcessConfigTestGuard::capture();
        // The fixture listens on loopback over plain http and the config carries a bearer token,
        // which is exactly the combination the outbound policy refuses by default; a developer
        // running a local MCP server opts in the same way.
        configure_local_servers(true);
        let fixture = StreamableHttpFixture::start();
        let config = McpServerConfig::remote("remote", "Remote MCP", fixture.url())
            .header("Authorization", "Bearer fixture-token");
        let mut client = McpClient::connect(&config).expect("connect HTTP MCP fixture");

        let initialized = client.initialize().expect("initialize HTTP MCP fixture");
        let tools = client.list_tools().expect("list HTTP MCP tools");
        let result = client
            .call_tool("echo", serde_json::json!({ "text": "hello remote" }))
            .expect("call HTTP MCP tool");

        assert_eq!(initialized["serverInfo"]["name"], "loom-http-fixture");
        assert_eq!(tools["tools"][0]["name"], "echo");
        assert_eq!(result["content"][0]["text"], "hello remote");
        fixture.finish();
    }

    #[test]
    fn streamable_http_initialize_uses_the_same_protocol_fallback_table() {
        let _guard = ProcessConfigTestGuard::capture();
        configure_local_servers(true);
        let fixture = ProtocolFallbackHttpFixture::start();
        let config = McpServerConfig::remote("remote-fallback", "Remote Fallback", fixture.url());
        let mut client =
            StreamableHttpMcpClient::connect(&config).expect("connect fallback HTTP fixture");

        let initialized = client
            .initialize()
            .expect("fall back to legacy HTTP MCP revision");
        assert_eq!(
            initialized["protocolVersion"],
            MCP_SUPPORTED_PROTOCOL_VERSIONS[1]
        );
        assert_eq!(client.protocol_version, MCP_SUPPORTED_PROTOCOL_VERSIONS[1]);
        client.close().expect("close fallback HTTP session");
        fixture.finish();
    }

    #[test]
    fn streamable_http_close_terminates_or_accepts_an_unsupported_session_close() {
        for mode in [SessionCloseMode::Success, SessionCloseMode::Unsupported] {
            let _guard = ProcessConfigTestGuard::capture();
            configure_local_servers(true);
            let fixture = SessionCloseHttpFixture::start(mode);
            let config = McpServerConfig::remote("remote-close", "Remote Close", fixture.url());
            let mut client =
                StreamableHttpMcpClient::connect(&config).expect("connect close fixture");

            client.initialize().expect("initialize close fixture");
            assert!(client.session_id.is_some());
            client.close().expect("close or accept unsupported close");
            assert!(client.session_id.is_none());
            fixture.finish();
        }
    }

    #[test]
    fn streamable_http_session_close_is_cancellable() {
        let _guard = ProcessConfigTestGuard::capture();
        configure_local_servers(true);
        let fixture = SessionCloseHttpFixture::start(SessionCloseMode::Delayed);
        let config = McpServerConfig::remote("remote-close", "Remote Close", fixture.url());
        let mut client = StreamableHttpMcpClient::connect(&config).expect("connect close fixture");
        client.initialize().expect("initialize close fixture");
        let cancellation = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&cancellation);
        let trigger_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            trigger.store(true, Ordering::Release);
        });

        let started = Instant::now();
        let error = client
            .close_cancellable(cancellation.as_ref())
            .expect_err("delayed close must be cancellable");
        let elapsed = started.elapsed();

        trigger_thread
            .join()
            .expect("join close cancellation trigger");
        assert!(matches!(error, McpError::Cancelled));
        assert!(
            elapsed < Duration::from_secs(1),
            "close cancel took {elapsed:?}"
        );
        fixture.finish();
    }

    #[test]
    fn live_streamable_http_server_from_official_registry() {
        let _guard = ProcessConfigTestGuard::capture();
        let Some(url) = std::env::var("LOOM_MCP_LIVE_TEST_URL")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        else {
            return;
        };
        let config = McpServerConfig::remote("live", "Live MCP", url);
        let mut client = McpClient::connect(&config).expect("connect live HTTP MCP");
        let initialized = client.initialize().expect("initialize live HTTP MCP");
        let tools = client.list_tools().expect("list live HTTP MCP tools");
        assert!(initialized.get("serverInfo").is_some());
        assert!(tools.get("tools").and_then(JsonValue::as_array).is_some());
    }

    #[test]
    fn stdio_client_times_out_and_terminates_hung_server() {
        let _guard = ProcessConfigTestGuard::capture();
        let config = current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "hang");
        let mut client = StdioMcpClient::spawn_with_timeout(&config, Duration::from_millis(150))
            .expect("spawn hung fixture");
        let error = client.initialize().expect_err("hung fixture must time out");
        assert!(matches!(error, McpError::Timeout { .. }));
    }

    #[test]
    fn stdio_client_cancels_a_hung_request_before_its_timeout() {
        let _guard = ProcessConfigTestGuard::capture();
        let config = current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "hang");
        let mut client = StdioMcpClient::spawn_with_timeout(&config, Duration::from_secs(5))
            .expect("spawn hung fixture");
        let cancellation = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&cancellation);
        let trigger_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            trigger.store(true, Ordering::Release);
        });

        let started = Instant::now();
        let error = client
            .initialize_cancellable(cancellation.as_ref())
            .expect_err("hung fixture must be cancellable");
        let elapsed = started.elapsed();

        trigger_thread.join().expect("join cancellation trigger");
        assert!(matches!(error, McpError::Cancelled));
        assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
    }

    #[test]
    fn streamable_http_cancels_while_waiting_for_response_headers() {
        assert_delayed_http_request_is_cancellable(DelayedHttpMode::Headers);
    }

    #[test]
    fn streamable_http_cancels_while_waiting_for_response_body() {
        assert_delayed_http_request_is_cancellable(DelayedHttpMode::Body);
    }

    fn assert_delayed_http_request_is_cancellable(mode: DelayedHttpMode) {
        let _guard = ProcessConfigTestGuard::capture();
        configure_local_servers(true);
        let fixture = DelayedHttpFixture::start(mode);
        let config = McpServerConfig::remote("delayed", "Delayed MCP", fixture.url());
        let mut client =
            StreamableHttpMcpClient::connect_with_timeout(&config, Duration::from_secs(5))
                .expect("connect delayed fixture");
        let cancellation = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&cancellation);
        let trigger_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            trigger.store(true, Ordering::Release);
        });

        let started = Instant::now();
        let error = client
            .initialize_cancellable(cancellation.as_ref())
            .expect_err("delayed request must be cancellable");
        let elapsed = started.elapsed();

        trigger_thread.join().expect("join cancellation trigger");
        assert!(matches!(error, McpError::Cancelled));
        assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
        fixture.finish();
    }

    #[test]
    fn stdio_client_drains_bounded_stderr_without_deadlocking() {
        let _guard = ProcessConfigTestGuard::capture();
        let config =
            current_test_binary_fixture_config().env("LOOM_MCP_FIXTURE_MODE", "stderr-flood");
        let mut client = StdioMcpClient::spawn_with_timeout(&config, Duration::from_secs(5))
            .expect("spawn stderr fixture");
        let init = client
            .initialize()
            .expect("stderr flood must not block stdout");
        assert_eq!(init["serverInfo"]["name"], "loom-fixture");
    }

    #[cfg(windows)]
    #[test]
    fn stdio_client_spawns_extensionless_windows_cmd_fixture() {
        let _guard = ProcessConfigTestGuard::capture();
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
        let fixture_mode = std::env::var("LOOM_MCP_FIXTURE_MODE").ok();
        match fixture_mode.as_deref() {
            Some("hang") => {
                std::thread::sleep(Duration::from_secs(30));
                return;
            }
            Some("stderr-flood") => {
                let mut stderr = std::io::stderr().lock();
                for _ in 0..256 {
                    stderr
                        .write_all(&[b'e'; 8192])
                        .expect("write stderr fixture chunk");
                }
                stderr.flush().expect("flush stderr fixture");
            }
            _ => {}
        }
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        for line in stdin.lock().lines() {
            let line = line.expect("fixture stdin line");
            let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let method = request["method"].as_str().unwrap_or_default();
            match method {
                "initialize" => {
                    let requested_version = request["params"]["protocolVersion"]
                        .as_str()
                        .unwrap_or_default();
                    let reject = fixture_mode.as_deref() == Some("reject-all-protocols")
                        || (fixture_mode.as_deref() == Some("reject-preferred")
                            && requested_version == MCP_PREFERRED_PROTOCOL_VERSION);
                    let response = if reject {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request["id"].clone(),
                            "error": {
                                "code": -32602,
                                "message": format!("unsupported protocol version {requested_version}")
                            }
                        })
                    } else {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request["id"].clone(),
                            "result": {
                                "protocolVersion": if fixture_mode.as_deref() == Some("reject-preferred") {
                                    requested_version
                                } else {
                                    "2024-11-05"
                                },
                                "capabilities": { "tools": {} },
                                "serverInfo": {
                                    "name": "loom-fixture",
                                    "version": "0.1.0"
                                }
                            }
                        })
                    };
                    write_fixture_response(&mut stdout, response);
                }
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

    #[derive(Clone, Copy)]
    enum DelayedHttpMode {
        Headers,
        Body,
    }

    struct DelayedHttpFixture {
        url: String,
        worker: thread::JoinHandle<()>,
    }

    struct ProtocolFallbackHttpFixture {
        url: String,
        worker: thread::JoinHandle<()>,
    }

    impl ProtocolFallbackHttpFixture {
        fn start() -> Self {
            let listener =
                TcpListener::bind(("127.0.0.1", 0)).expect("bind protocol fallback fixture");
            let address = listener
                .local_addr()
                .expect("protocol fallback fixture address");
            let worker = thread::spawn(move || {
                for request_index in 0..4 {
                    let (mut stream, _) =
                        listener.accept().expect("accept protocol fallback request");
                    let request = read_http_fixture_request(&mut stream);
                    match request_index {
                        0 | 1 => {
                            let message: JsonValue = request
                                .split_once("\r\n\r\n")
                                .and_then(|(_, body)| serde_json::from_str(body).ok())
                                .expect("protocol fallback initialize JSON");
                            let requested = message["params"]["protocolVersion"]
                                .as_str()
                                .expect("requested protocol version");
                            assert_eq!(requested, MCP_SUPPORTED_PROTOCOL_VERSIONS[request_index]);
                            let lower_request = request.to_ascii_lowercase();
                            assert!(lower_request.contains(&format!(
                                "mcp-protocol-version: {}",
                                MCP_SUPPORTED_PROTOCOL_VERSIONS[request_index]
                            )));
                            if request_index == 0 {
                                write_http_fixture_response(
                                    &mut stream,
                                    "200 OK",
                                    "application/json",
                                    None,
                                    &serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": message["id"],
                                        "error": {
                                            "code": -32602,
                                            "message": "unsupported protocol version"
                                        }
                                    })
                                    .to_string(),
                                );
                            } else {
                                write_http_fixture_response(
                                    &mut stream,
                                    "200 OK",
                                    "application/json",
                                    Some("fallback-session"),
                                    &serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": message["id"],
                                        "result": {
                                            "protocolVersion": MCP_SUPPORTED_PROTOCOL_VERSIONS[1],
                                            "capabilities": {},
                                            "serverInfo": {
                                                "name": "fallback-fixture",
                                                "version": "0.1.0"
                                            }
                                        }
                                    })
                                    .to_string(),
                                );
                            }
                        }
                        2 => {
                            assert!(request
                                .to_ascii_lowercase()
                                .contains("mcp-session-id: fallback-session"));
                            write_http_fixture_response(
                                &mut stream,
                                "202 Accepted",
                                "application/json",
                                None,
                                "",
                            );
                        }
                        _ => {
                            assert!(request.starts_with("DELETE "));
                            assert!(request
                                .to_ascii_lowercase()
                                .contains("mcp-session-id: fallback-session"));
                            write_http_fixture_response(
                                &mut stream,
                                "204 No Content",
                                "application/json",
                                None,
                                "",
                            );
                        }
                    }
                }
            });
            Self {
                url: format!("http://{address}/mcp"),
                worker,
            }
        }

        fn url(&self) -> String {
            self.url.clone()
        }

        fn finish(self) {
            self.worker
                .join()
                .expect("protocol fallback fixture worker");
        }
    }

    impl DelayedHttpFixture {
        fn start(mode: DelayedHttpMode) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind delayed HTTP fixture");
            let address = listener.local_addr().expect("delayed HTTP fixture address");
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept delayed HTTP request");
                let request = read_http_fixture_request(&mut stream);
                let message: JsonValue = request
                    .split_once("\r\n\r\n")
                    .and_then(|(_, body)| serde_json::from_str(body).ok())
                    .expect("delayed HTTP fixture JSON");
                let body = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": message["id"],
                    "result": {
                        "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
                        "capabilities": {},
                        "serverInfo": { "name": "delayed", "version": "0.1.0" }
                    }
                })
                .to_string();
                match mode {
                    DelayedHttpMode::Headers => {
                        thread::sleep(Duration::from_millis(500));
                        let _ = write_http_fixture_response_unchecked(&mut stream, &body);
                    }
                    DelayedHttpMode::Body => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let split = body.len() / 2;
                        let _ = stream.write_all(&body.as_bytes()[..split]);
                        let _ = stream.flush();
                        thread::sleep(Duration::from_millis(500));
                        let _ = stream.write_all(&body.as_bytes()[split..]);
                        let _ = stream.flush();
                    }
                }
            });
            Self {
                url: format!("http://{address}/mcp"),
                worker,
            }
        }

        fn url(&self) -> String {
            self.url.clone()
        }

        fn finish(self) {
            self.worker.join().expect("delayed HTTP fixture worker");
        }
    }

    fn write_http_fixture_response_unchecked(
        stream: &mut TcpStream,
        body: &str,
    ) -> std::io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )?;
        stream.flush()
    }

    #[derive(Clone, Copy)]
    enum SessionCloseMode {
        Success,
        Unsupported,
        Delayed,
    }

    struct SessionCloseHttpFixture {
        url: String,
        worker: thread::JoinHandle<()>,
    }

    impl SessionCloseHttpFixture {
        fn start(mode: SessionCloseMode) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind close HTTP fixture");
            let address = listener.local_addr().expect("close HTTP fixture address");
            let worker = thread::spawn(move || {
                for request_index in 0..3 {
                    let (mut stream, _) = listener.accept().expect("accept close HTTP request");
                    let request = read_http_fixture_request(&mut stream);
                    match request_index {
                        0 => {
                            assert!(request.starts_with("POST "));
                            let message: JsonValue = request
                                .split_once("\r\n\r\n")
                                .and_then(|(_, body)| serde_json::from_str(body).ok())
                                .expect("close fixture initialize JSON");
                            write_http_fixture_response(
                                &mut stream,
                                "200 OK",
                                "application/json",
                                Some("close-session"),
                                &serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": message["id"],
                                    "result": {
                                        "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
                                        "capabilities": {},
                                        "serverInfo": { "name": "close-fixture", "version": "0.1.0" }
                                    }
                                })
                                .to_string(),
                            );
                        }
                        1 => write_http_fixture_response(
                            &mut stream,
                            "202 Accepted",
                            "application/json",
                            None,
                            "",
                        ),
                        _ => {
                            assert!(request.starts_with("DELETE "));
                            assert!(request
                                .to_ascii_lowercase()
                                .contains("mcp-session-id: close-session"));
                            match mode {
                                SessionCloseMode::Success => write_http_fixture_response(
                                    &mut stream,
                                    "204 No Content",
                                    "application/json",
                                    None,
                                    "",
                                ),
                                SessionCloseMode::Unsupported => write_http_fixture_response(
                                    &mut stream,
                                    "405 Method Not Allowed",
                                    "text/plain",
                                    None,
                                    "unsupported",
                                ),
                                SessionCloseMode::Delayed => {
                                    thread::sleep(Duration::from_millis(500));
                                    let _ = write_http_fixture_response_unchecked(&mut stream, "");
                                }
                            }
                        }
                    }
                }
            });
            Self {
                url: format!("http://{address}/mcp"),
                worker,
            }
        }

        fn url(&self) -> String {
            self.url.clone()
        }

        fn finish(self) {
            self.worker.join().expect("close HTTP fixture worker");
        }
    }

    struct StreamableHttpFixture {
        url: String,
        worker: thread::JoinHandle<()>,
    }

    impl StreamableHttpFixture {
        fn start() -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP MCP fixture");
            let address = listener.local_addr().expect("HTTP MCP fixture address");
            let worker = thread::spawn(move || {
                for request_index in 0..4 {
                    let (mut stream, _) = listener.accept().expect("accept HTTP MCP request");
                    let request = read_http_fixture_request(&mut stream);
                    assert!(request
                        .to_ascii_lowercase()
                        .contains("accept: application/json, text/event-stream"));
                    assert!(request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer fixture-token"));
                    if request_index > 0 {
                        assert!(request
                            .to_ascii_lowercase()
                            .contains("mcp-session-id: fixture-session"));
                    }
                    let body = request
                        .split_once("\r\n\r\n")
                        .map(|(_, body)| body)
                        .unwrap_or("{}");
                    let message: JsonValue =
                        serde_json::from_str(body).expect("HTTP MCP fixture JSON");
                    match message["method"].as_str().unwrap_or_default() {
                        "initialize" => write_http_fixture_response(
                            &mut stream,
                            "200 OK",
                            "application/json",
                            Some("fixture-session"),
                            &serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": message["id"],
                                "result": {
                                    "protocolVersion": MCP_PREFERRED_PROTOCOL_VERSION,
                                    "capabilities": { "tools": {} },
                                    "serverInfo": { "name": "loom-http-fixture", "version": "0.1.0" }
                                }
                            })
                            .to_string(),
                        ),
                        "notifications/initialized" => write_http_fixture_response(
                            &mut stream,
                            "202 Accepted",
                            "application/json",
                            None,
                            "",
                        ),
                        "tools/list" => write_http_fixture_response(
                            &mut stream,
                            "200 OK",
                            "text/event-stream",
                            None,
                            &format!(
                                "event: message\ndata: {}\n\n",
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": message["id"],
                                    "result": { "tools": [{ "name": "echo", "inputSchema": { "type": "object" } }] }
                                })
                            ),
                        ),
                        "tools/call" => write_http_fixture_response(
                            &mut stream,
                            "200 OK",
                            "application/json",
                            None,
                            &serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": message["id"],
                                "result": {
                                    "content": [{ "type": "text", "text": message["params"]["arguments"]["text"] }]
                                }
                            })
                            .to_string(),
                        ),
                        method => panic!("unexpected HTTP MCP method {method}"),
                    }
                }
            });
            Self {
                url: format!("http://{address}/mcp"),
                worker,
            }
        }

        fn url(&self) -> String {
            self.url.clone()
        }

        fn finish(self) {
            self.worker.join().expect("HTTP MCP fixture worker");
        }
    }

    fn read_http_fixture_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = stream.read(&mut buffer).expect("read HTTP MCP request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            let text = String::from_utf8_lossy(&request);
            let Some(header_end) = text.find("\r\n\r\n") else {
                continue;
            };
            let content_length = text[..header_end]
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.trim()
                        .eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        String::from_utf8(request).expect("HTTP MCP request UTF-8")
    }

    fn write_http_fixture_response(
        stream: &mut TcpStream,
        status: &str,
        content_type: &str,
        session_id: Option<&str>,
        body: &str,
    ) {
        let session_header = session_id
            .map(|value| format!("MCP-Session-Id: {value}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{session_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("write HTTP MCP response");
        stream.flush().expect("flush HTTP MCP response");
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
