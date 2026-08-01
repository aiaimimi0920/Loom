//! User-managed tool and Art registry contracts for Loom.

pub mod credentials;
pub mod dependency;
pub mod framework;
pub mod framework_process;
pub mod install;
pub mod network_policy;
mod secure_zip;

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use loom_process::{ProcessError, ProcessSpec};
use loom_protocol::{is_safe_publisher_id, PublisherIdentity};
use reqwest::blocking::multipart;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const TOOLS_FILE: &str = "tools.json";
const SCRIPT_EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
const CLOUD_API_TIMEOUT: Duration = Duration::from_secs(30);
const MCP_IMAGE_FETCH_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
const MCP_IMAGE_FETCH_ACCEPT: &str =
    "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";
const MCP_IMAGE_FETCH_ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";
const MAX_MCP_IMAGE_BYTES: usize = 32 * 1024 * 1024;
static REGISTRY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum ToolRegistryError {
    #[error("invalid tool definition `{id}`: {reason}")]
    InvalidToolDefinition { id: String, reason: String },
    #[error("tool `{id}` is disabled")]
    ExecutionRejected { id: String },
    #[error("tool id `{id}` is ambiguous; use a publisher-qualified id")]
    AmbiguousToolId { id: String },
    #[error("tool `{id}` execution type `{execution_type}` is not supported by this runtime")]
    UnsupportedExecution {
        id: String,
        execution_type: &'static str,
    },
    #[error("MCP server `{server_id}` for tool `{tool_id}` was not found or is disabled")]
    MissingMcpServer { tool_id: String, server_id: String },
    #[error("MCP execution failed: {0}")]
    Mcp(#[from] loom_mcp::McpError),
    #[error("CLI tool `{id}` failed: {reason}")]
    CliWrapperFailed { id: String, reason: String },
    #[error("script `{path}` for tool `{id}` was not found")]
    ScriptNotFound { id: String, path: String },
    #[error("script `{path}` for tool `{id}` failed to spawn: {source}")]
    ScriptSpawn {
        id: String,
        path: String,
        source: std::io::Error,
    },
    #[error("script `{path}` for tool `{id}` timed out after {timeout_ms}ms")]
    ScriptTimedOut {
        id: String,
        path: String,
        timeout_ms: u128,
    },
    #[error("script `{path}` for tool `{id}` exited with code {code:?}: {stderr}")]
    ScriptFailed {
        id: String,
        path: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("script `{path}` for tool `{id}` returned no stdout")]
    ScriptEmptyStdout { id: String, path: String },
    #[error("script `{path}` for tool `{id}` returned invalid JSON: {source}; stdout: {stdout}")]
    ScriptJson {
        id: String,
        path: String,
        source: serde_json::Error,
        stdout: String,
    },
    #[error("Python Art `{art_id}` for tool `{id}` was not found")]
    PythonArtNotFound { id: String, art_id: String },
    #[error("Python Art launcher for tool `{id}` was not found")]
    PythonArtLauncherNotFound { id: String },
    #[error("Python Art `{art_id}` for tool `{id}` failed to spawn: {source}")]
    PythonArtSpawn {
        id: String,
        art_id: String,
        source: std::io::Error,
    },
    #[error("Python Art `{art_id}` for tool `{id}` exited with code {code:?}: {stderr}")]
    PythonArtFailed {
        id: String,
        art_id: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("Python Art `{art_id}` for tool `{id}` returned no stdout")]
    PythonArtEmptyStdout { id: String, art_id: String },
    #[error(
        "Python Art `{art_id}` for tool `{id}` returned invalid JSON: {source}; stdout: {stdout}"
    )]
    PythonArtJson {
        id: String,
        art_id: String,
        source: serde_json::Error,
        stdout: String,
    },
    #[error("Python Art `{art_id}` for tool `{id}` returned status {status}: {message}")]
    PythonArtStatus {
        id: String,
        art_id: String,
        status: i64,
        message: String,
    },
    #[error("cloud API method `{method}` for tool `{id}` is not supported")]
    CloudInvalidMethod { id: String, method: String },
    #[error("cloud API request to `{endpoint}` for tool `{id}` failed: {source}")]
    CloudRequest {
        id: String,
        endpoint: String,
        source: reqwest::Error,
    },
    #[error("cloud API endpoint `{endpoint}` for tool `{id}` violates network policy: {reason}")]
    CloudSecurity {
        id: String,
        endpoint: String,
        reason: String,
    },
    #[error("cloud API request to `{endpoint}` for tool `{id}` returned HTTP {status}: {body}")]
    CloudHttpStatus {
        id: String,
        endpoint: String,
        status: u16,
        body: String,
    },
    #[error("cloud API response from `{endpoint}` for tool `{id}` returned invalid JSON: {source}; body: {body}")]
    CloudJson {
        id: String,
        endpoint: String,
        source: serde_json::Error,
        body: String,
    },
    #[error("cloud API `{field}` template for tool `{id}` is invalid: {reason}")]
    CloudTemplate {
        id: String,
        field: &'static str,
        reason: String,
    },
    #[error("framework package `{framework}` for tool `{id}` was not found: {path}")]
    FrameworkPackageNotFound {
        id: String,
        framework: String,
        path: String,
    },
    #[error("framework Art directory for tool `{id}` was not found: {path}")]
    FrameworkArtDirectoryNotFound { id: String, path: String },
    #[error("framework `{framework}` for tool `{id}` failed to spawn: {reason}")]
    FrameworkProcessSpawn {
        id: String,
        framework: String,
        reason: String,
    },
    #[error("framework `{framework}` for tool `{id}` timed out after {timeout_ms}ms")]
    FrameworkProcessTimeout {
        id: String,
        framework: String,
        timeout_ms: u128,
    },
    #[error("framework `{framework}` for tool `{id}` process I/O failed: {reason}")]
    FrameworkProcessIo {
        id: String,
        framework: String,
        reason: String,
    },
    #[error("framework `{framework}` for tool `{id}` returned invalid protocol data: {reason}")]
    FrameworkProcessProtocol {
        id: String,
        framework: String,
        reason: String,
    },
    #[error("framework `{framework}` for tool `{id}` failed [{code}]: {message}{detail}")]
    FrameworkProcessFailed {
        id: String,
        framework: String,
        code: String,
        message: String,
        detail: String,
    },
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type ToolRegistryResult<T> = Result<T, ToolRegistryError>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub execution: ToolExecution,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl ToolDefinition {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        execution: ToolExecution,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            enabled: true,
            execution,
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: Vec::new(),
            metadata: None,
        }
    }

    pub fn validate(&self) -> ToolRegistryResult<()> {
        require_non_empty(&self.id, &self.id, "id")?;
        require_no_path_separator(&self.id, &self.id)?;
        require_non_empty(&self.id, &self.name, "name")?;
        if let Some(publisher) = self.publisher_identity() {
            if !is_safe_publisher_id(&publisher.id) {
                return Err(ToolRegistryError::InvalidToolDefinition {
                    id: self.id.clone(),
                    reason: "publisher id must be a safe package namespace".to_owned(),
                });
            }
        }
        self.execution.validate(&self.id)
    }

    #[must_use]
    pub fn publisher_identity(&self) -> Option<PublisherIdentity> {
        self.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("packageSecurity"))
            .and_then(|security| security.get("publisher"))
            .and_then(|publisher| serde_json::from_value(publisher.clone()).ok())
    }

    #[must_use]
    pub fn qualified_id(&self) -> String {
        self.publisher_identity()
            .map(|publisher| format!("{}/{}", publisher.id, self.id))
            .unwrap_or_else(|| self.id.clone())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ToolExecution {
    #[serde(rename_all = "camelCase")]
    CliWrapper { command: String, args: Vec<String> },
    #[serde(rename_all = "camelCase")]
    CloudApi {
        #[serde(alias = "url")]
        endpoint: String,
        method: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        headers: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Script { path: String },
    #[serde(rename_all = "camelCase")]
    PythonArt {
        art_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        art_path: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Mcp {
        server_id: String,
        tool_name: String,
    },
    #[serde(rename_all = "camelCase")]
    Workflow {
        workflow_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workflow_bindings: Option<WorkflowExecutionBindings>,
    },
    #[serde(rename_all = "camelCase")]
    FrameworkArt { framework: String },
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionBindings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<WorkflowInputBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_output: Option<WorkflowOutputBinding>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInputBinding {
    pub workflow_param: String,
    pub node_id: String,
    pub target: String,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutputBinding {
    pub node_id: String,
    pub output: String,
    pub kind: String,
}

impl ToolExecution {
    fn validate(&self, tool_id: &str) -> ToolRegistryResult<()> {
        match self {
            Self::CliWrapper { command, .. } => require_non_empty(tool_id, command, "command"),
            Self::CloudApi {
                endpoint, method, ..
            } => {
                require_non_empty(tool_id, endpoint, "endpoint")?;
                require_non_empty(tool_id, method, "method")
            }
            Self::Script { path } => require_non_empty(tool_id, path, "path"),
            Self::PythonArt { art_id, .. } => require_non_empty(tool_id, art_id, "art_id"),
            Self::Mcp {
                server_id,
                tool_name,
            } => {
                require_non_empty(tool_id, server_id, "server_id")?;
                require_non_empty(tool_id, tool_name, "tool_name")
            }
            Self::Workflow { workflow_id, .. } => {
                require_non_empty(tool_id, workflow_id, "workflow_id")
            }
            Self::FrameworkArt { framework } => {
                require_non_empty(tool_id, framework, "framework")?;
                if !framework::is_valid_framework_reference(framework) {
                    return Err(ToolRegistryError::InvalidToolDefinition {
                        id: tool_id.to_owned(),
                        reason: "framework must be a safe package id".to_owned(),
                    });
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolRegistry {
    root: PathBuf,
}

impl ToolRegistry {
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn save_tool(&self, tool: ToolDefinition) -> ToolRegistryResult<ToolDefinition> {
        tool.validate()?;
        self.ensure_root()?;

        let mut tools = self.read_tools()?;
        let qualified_id = tool.qualified_id();
        if let Some(existing) = tools
            .iter_mut()
            .find(|existing| existing.qualified_id() == qualified_id)
        {
            *existing = tool.clone();
        } else {
            tools.push(tool.clone());
        }
        sort_tools(&mut tools);
        self.write_tools(&tools)?;
        Ok(tool)
    }

    pub fn list_tools(&self) -> ToolRegistryResult<Vec<ToolDefinition>> {
        self.ensure_root()?;
        let mut tools = self.read_tools()?;
        sort_tools(&mut tools);
        Ok(tools)
    }

    pub fn get_tool(&self, id: &str) -> ToolRegistryResult<Option<ToolDefinition>> {
        let tools = self.list_tools()?;
        if let Some(tool) = tools.iter().find(|tool| tool.qualified_id() == id) {
            return Ok(Some(tool.clone()));
        }
        let mut matches = tools.into_iter().filter(|tool| tool.id == id);
        let first = matches.next();
        if first.is_some() && matches.next().is_some() {
            return Err(ToolRegistryError::AmbiguousToolId { id: id.to_owned() });
        }
        Ok(first)
    }

    pub fn delete_tool(&self, id: &str) -> ToolRegistryResult<bool> {
        self.ensure_root()?;
        let mut tools = self.read_tools()?;
        let exact = tools
            .iter()
            .position(|tool| tool.qualified_id() == id)
            .or_else(|| {
                let matches = tools
                    .iter()
                    .enumerate()
                    .filter(|(_, tool)| tool.id == id)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                if matches.len() == 1 {
                    Some(matches[0])
                } else {
                    None
                }
            });
        if exact.is_none() && tools.iter().filter(|tool| tool.id == id).count() > 1 {
            return Err(ToolRegistryError::AmbiguousToolId { id: id.to_owned() });
        }
        let before = tools.len();
        if let Some(index) = exact {
            tools.remove(index);
        }
        let deleted = tools.len() != before;
        if deleted {
            self.write_tools(&tools)?;
        }
        Ok(deleted)
    }

    fn ensure_root(&self) -> ToolRegistryResult<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    fn tools_path(&self) -> PathBuf {
        self.root.join(TOOLS_FILE)
    }

    fn read_tools(&self) -> ToolRegistryResult<Vec<ToolDefinition>> {
        let path = self.tools_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path)?;
        match serde_json::from_str(&content) {
            Ok(tools) => Ok(tools),
            Err(error) => {
                let Some(tools) = recover_tools_with_trailing_delimiters(&content) else {
                    return Err(ToolRegistryError::Json(error));
                };
                self.write_corruption_backup(&content)?;
                self.write_tools(&tools)?;
                Ok(tools)
            }
        }
    }

    fn write_tools(&self, tools: &[ToolDefinition]) -> ToolRegistryResult<()> {
        let content = serde_json::to_string_pretty(tools)?;
        let (temporary_path, mut temporary_file) = self.create_transient_file("tmp")?;
        if let Err(error) = temporary_file
            .write_all(content.as_bytes())
            .and_then(|()| temporary_file.sync_all())
        {
            let _ = fs::remove_file(&temporary_path);
            return Err(ToolRegistryError::Io(error));
        }
        drop(temporary_file);

        if let Err(error) = replace_registry_file(&temporary_path, &self.tools_path()) {
            let _ = fs::remove_file(&temporary_path);
            return Err(ToolRegistryError::Io(error));
        }
        Ok(())
    }

    fn write_corruption_backup(&self, content: &str) -> ToolRegistryResult<PathBuf> {
        let (backup_path, mut backup_file) = self.create_transient_file("corrupt")?;
        if let Err(error) = backup_file
            .write_all(content.as_bytes())
            .and_then(|()| backup_file.sync_all())
        {
            let _ = fs::remove_file(&backup_path);
            return Err(ToolRegistryError::Io(error));
        }
        Ok(backup_path)
    }

    fn create_transient_file(&self, marker: &str) -> ToolRegistryResult<(PathBuf, File)> {
        for _ in 0..100 {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let sequence = REGISTRY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = self.root.join(format!(
                "{TOOLS_FILE}.{marker}-{}-{timestamp}-{sequence}",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(ToolRegistryError::Io(error)),
            }
        }

        Err(ToolRegistryError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate a unique tool registry temporary file",
        )))
    }
}

fn recover_tools_with_trailing_delimiters(content: &str) -> Option<Vec<ToolDefinition>> {
    let mut stream = serde_json::Deserializer::from_str(content).into_iter::<Vec<ToolDefinition>>();
    let tools = stream.next()?.ok()?;
    let trailing = content.get(stream.byte_offset()..)?;
    if trailing.trim().is_empty()
        || !trailing
            .chars()
            .all(|character| character.is_whitespace() || matches!(character, '}' | ']'))
    {
        return None;
    }
    Some(tools)
}

#[cfg(not(windows))]
fn replace_registry_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_registry_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    fn extended_length_path(path: &Path) -> std::io::Result<Vec<u16>> {
        let absolute = match fs::canonicalize(path) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = path.parent().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "registry file path has no parent",
                    )
                })?;
                let file_name = path.file_name().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "registry file path has no file name",
                    )
                })?;
                fs::canonicalize(parent)?.join(file_name)
            }
            Err(error) => return Err(error),
        };
        let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
        let mut extended =
            if wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
                || wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16])
            {
                wide
            } else if wide.starts_with(&[b'\\' as u16, b'\\' as u16]) {
                let mut path = r"\\?\UNC\".encode_utf16().collect::<Vec<_>>();
                path.extend_from_slice(&wide[2..]);
                path
            } else {
                let mut path = r"\\?\".encode_utf16().collect::<Vec<_>>();
                path.extend_from_slice(&wide);
                path
            };
        extended.push(0);
        Ok(extended)
    }

    let source = extended_length_path(source)?;
    let destination = extended_length_path(destination)?;
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn sort_tools(tools: &mut [ToolDefinition]) {
    tools.sort_by_key(ToolDefinition::qualified_id);
}

pub fn execute_tool(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    tool.validate()?;
    if !tool.enabled {
        return Err(ToolRegistryError::ExecutionRejected {
            id: tool.id.clone(),
        });
    }

    match &tool.execution {
        ToolExecution::Mcp {
            server_id,
            tool_name,
        } => {
            let server = mcp_servers
                .iter()
                .find(|server| server.id == *server_id && server.enabled)
                .ok_or_else(|| ToolRegistryError::MissingMcpServer {
                    tool_id: tool.id.clone(),
                    server_id: server_id.clone(),
                })?;

            let mut client = loom_mcp::StdioMcpClient::spawn(server)?;
            client.initialize()?;
            let tool_list = client.list_tools().ok();
            let normalized_arguments = normalize_mcp_call_arguments(
                &arguments,
                tool_list
                    .as_ref()
                    .and_then(|tools| find_mcp_tool_input_schema(tools, tool_name)),
            );
            client
                .call_tool(tool_name, normalized_arguments.clone())
                .map(|value| normalize_mcp_result(tool, &normalized_arguments, value))
                .map_err(ToolRegistryError::from)
        }
        ToolExecution::CloudApi {
            endpoint,
            method,
            content_type,
            headers,
            body,
        } => execute_cloud_api_tool(
            tool,
            endpoint,
            method,
            content_type.as_deref(),
            headers.as_deref(),
            body.as_deref(),
            arguments,
        ),
        ToolExecution::Script { path } => execute_script_tool(tool, path, arguments),
        ToolExecution::CliWrapper { command, args } => {
            execute_cli_wrapper_tool(tool, command, args, arguments)
        }
        ToolExecution::PythonArt { art_id, art_path } => {
            execute_python_art_tool(tool, art_id, art_path.as_deref(), arguments)
        }
        ToolExecution::FrameworkArt { framework } => {
            framework_process::execute_framework_art(tool, framework, arguments)
        }
        _ => Err(ToolRegistryError::UnsupportedExecution {
            id: tool.id.clone(),
            execution_type: execution_type_name(&tool.execution),
        }),
    }
}

fn find_mcp_tool_input_schema<'a>(
    listed_tools: &'a serde_json::Value,
    tool_name: &str,
) -> Option<&'a serde_json::Value> {
    listed_tools
        .get("tools")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find(|tool| tool.get("name").and_then(serde_json::Value::as_str) == Some(tool_name))
        .and_then(|tool| tool.get("inputSchema").or_else(|| tool.get("input_schema")))
}

fn normalize_mcp_call_arguments(
    arguments: &serde_json::Value,
    input_schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    let Some(argument_object) = arguments.as_object() else {
        return arguments.clone();
    };
    let property_schemas = input_schema
        .and_then(|schema| schema.get("properties"))
        .and_then(serde_json::Value::as_object);
    let mut normalized = serde_json::Map::with_capacity(argument_object.len());
    for (key, value) in argument_object {
        let schema = property_schemas.and_then(|properties| properties.get(key));
        normalized.insert(
            key.clone(),
            normalize_mcp_argument_value(key, value, schema),
        );
    }
    serde_json::Value::Object(normalized)
}

fn normalize_mcp_argument_value(
    name: &str,
    value: &serde_json::Value,
    schema: Option<&serde_json::Value>,
) -> serde_json::Value {
    if let Some(normalized) = normalize_mcp_search_lang_alias(name, value, schema) {
        return normalized;
    }
    if let Some(schema) = schema {
        if schema_type_matches(schema, "integer") {
            if let Some(parsed) = value.as_i64() {
                return serde_json::Value::from(parsed);
            }
            if let Some(raw) = value.as_str().map(str::trim) {
                if let Ok(parsed) = raw.parse::<i64>() {
                    return serde_json::Value::from(parsed);
                }
            }
        }
        if schema_type_matches(schema, "number") {
            if let Some(parsed) = value.as_f64() {
                return serde_json::Value::from(parsed);
            }
            if let Some(raw) = value.as_str().map(str::trim) {
                if let Ok(parsed) = raw.parse::<f64>() {
                    return serde_json::Value::from(parsed);
                }
            }
        }
        if schema_type_matches(schema, "boolean") {
            if let Some(parsed) = value.as_bool() {
                return serde_json::Value::from(parsed);
            }
            if let Some(raw) = value.as_str().map(str::trim) {
                if let Some(parsed) = parse_bool_string(raw) {
                    return serde_json::Value::from(parsed);
                }
            }
        }
        if let (Some(raw), Some(enum_values)) = (
            value.as_str(),
            schema.get("enum").and_then(serde_json::Value::as_array),
        ) {
            if let Some(canonical) = enum_values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .find(|candidate| candidate.eq_ignore_ascii_case(raw))
            {
                return serde_json::Value::String(canonical.to_owned());
            }
        }
    }
    value.clone()
}

fn normalize_mcp_search_lang_alias(
    name: &str,
    value: &serde_json::Value,
    schema: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    if !name.eq_ignore_ascii_case("search_lang") {
        return None;
    }
    let raw = value.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(canonical) = schema_enum_canonical_value(schema, raw) {
        if canonical == raw {
            return None;
        }
        return Some(serde_json::Value::String(canonical));
    }

    let lowered = raw.to_ascii_lowercase();
    let mapped = match lowered.as_str() {
        "zh" | "zh-cn" => "zh-hans".to_owned(),
        "zh-tw" | "zh-hant" => "zh-hant".to_owned(),
        _ => lowered,
    };

    if let Some(canonical) = schema_enum_canonical_value(schema, &mapped) {
        if canonical == raw {
            return None;
        }
        return Some(serde_json::Value::String(canonical));
    }

    (mapped != raw).then_some(serde_json::Value::String(mapped))
}

fn schema_type_matches(schema: &serde_json::Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(serde_json::Value::String(actual)) => actual == expected,
        Some(serde_json::Value::Array(actual)) => actual
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|candidate| candidate == expected),
        _ => false,
    }
}

fn schema_enum_canonical_value(
    schema: Option<&serde_json::Value>,
    expected: &str,
) -> Option<String> {
    schema
        .and_then(|schema| schema.get("enum"))
        .and_then(serde_json::Value::as_array)
        .and_then(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .find(|candidate| candidate.eq_ignore_ascii_case(expected))
                .map(str::to_owned)
        })
}

fn parse_bool_string(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn execute_python_art_tool(
    tool: &ToolDefinition,
    art_id: &str,
    art_path: Option<&str>,
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    let launcher_path = resolve_python_launcher_path().ok_or_else(|| {
        ToolRegistryError::PythonArtLauncherNotFound {
            id: tool.id.clone(),
        }
    })?;
    let plugin_path = resolve_python_art_path(&tool.id, art_id, art_path).ok_or_else(|| {
        ToolRegistryError::PythonArtNotFound {
            id: tool.id.clone(),
            art_id: art_id.to_owned(),
        }
    })?;
    let launcher_path = canonical_process_path(launcher_path);
    let plugin_path = canonical_process_path(plugin_path);
    let base_dir = launcher_path
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let request = serde_json::json!({
        "request_id": format!("loom-python-art-{}", tool.id),
        "art_id": art_id,
        "plugin_path": plugin_path,
        "params": arguments,
    });
    let request_json = serde_json::to_string(&request)?;
    let mut command = Command::new(canonical_process_path(resolve_python_executable()));
    configure_python_process(&mut command);
    command
        .arg(&launcher_path)
        .arg(request_json)
        .current_dir(base_dir);
    let mut process = ProcessSpec::from_command(&command);
    process.limits.timeout = SCRIPT_EXECUTION_TIMEOUT;
    let output = loom_process::run_with_input(&process, b"").map_err(|error| {
        ToolRegistryError::PythonArtSpawn {
            id: tool.id.clone(),
            art_id: art_id.to_owned(),
            source: std::io::Error::other(error.to_string()),
        }
    })?;

    if !output.status.success() {
        return Err(ToolRegistryError::PythonArtFailed {
            id: tool.id.clone(),
            art_id: art_id.to_owned(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if stdout.is_empty() {
        return Err(ToolRegistryError::PythonArtEmptyStdout {
            id: tool.id.clone(),
            art_id: art_id.to_owned(),
        });
    }

    let response: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|source| ToolRegistryError::PythonArtJson {
            id: tool.id.clone(),
            art_id: art_id.to_owned(),
            source,
            stdout,
        })?;
    let status = response
        .get("status")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(500);
    if status != 200 {
        let message = response
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Python Art execution failed")
            .to_owned();
        return Err(ToolRegistryError::PythonArtStatus {
            id: tool.id.clone(),
            art_id: art_id.to_owned(),
            status,
            message,
        });
    }

    let data = response
        .get("data")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(normalize_python_art_data(data))
}

fn normalize_python_art_data(data: serde_json::Value) -> serde_json::Value {
    if data
        .get("content")
        .and_then(serde_json::Value::as_array)
        .is_some()
    {
        return data;
    }
    if let Some(text) = data.get("text").and_then(serde_json::Value::as_str) {
        return text_content_response(text);
    }
    if let Some(image) = data
        .get("output_base64")
        .or_else(|| data.get("image_base64"))
        .or_else(|| data.get("image"))
        .and_then(serde_json::Value::as_str)
    {
        return image_content_response(image, "image/png");
    }
    if let Some(output_path) = data
        .get("output_path")
        .or_else(|| data.get("outputPath"))
        .and_then(serde_json::Value::as_str)
    {
        if let Ok(bytes) = fs::read(output_path) {
            return image_content_response(
                &format!("data:image/png;base64,{}", BASE64.encode(bytes)),
                "image/png",
            );
        }
    }
    text_content_response(&data.to_string())
}

fn resolve_python_launcher_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    // A framework package installed via the framework registry wins: the
    // `python_art` package may ship the launcher alongside its interpreter.
    if let Some(runtime_dir) = framework_packages_root_env() {
        for base in python_framework_package_dirs(Path::new(&runtime_dir)) {
            candidates.push(base.join("python").join("Launcher.py"));
            candidates.push(base.join("Launcher.py"));
        }
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        candidates.push(exe_dir.join("python").join("Launcher.py"));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("python").join("Launcher.py"));
        candidates.push(
            current_dir
                .join("Loom")
                .join("resources")
                .join("python")
                .join("Launcher.py"),
        );
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn resolve_python_art_path(tool_id: &str, art_id: &str, art_path: Option<&str>) -> Option<PathBuf> {
    if let Some(art_path) = art_path.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(art_path);
        if path.is_dir() {
            return Some(path);
        }
        if path.is_relative() {
            let control_plane_root = std::env::var("LOOM_CONTROL_PLANE_ROOT")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())?;
            let candidate = Path::new(&control_plane_root)
                .join("arts")
                .join(tool_id)
                .join(&path);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }

    python_arts_dirs()
        .into_iter()
        .find_map(|arts_dir| find_python_art_in_dir(&arts_dir, art_id))
}

fn python_arts_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // Arts bundled inside the installed python_art framework package win.
    if let Some(runtime_dir) = framework_packages_root_env() {
        for base in python_framework_package_dirs(Path::new(&runtime_dir)) {
            dirs.push(base.join("python").join("Arts"));
            dirs.push(base.join("Arts"));
        }
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir.join("python").join("Arts"));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        dirs.push(current_dir.join("python").join("Arts"));
        dirs.push(
            current_dir
                .join("Loom")
                .join("resources")
                .join("python")
                .join("Arts"),
        );
    }
    dirs
}

fn python_framework_package_dirs(runtime_root: &Path) -> Vec<PathBuf> {
    let legacy = runtime_root.join("python_art");
    let mut packages = Vec::new();
    if let Some(active) = framework::resolve_framework_package_dir(runtime_root, "python_art") {
        packages.push(active);
    }
    if packages.first() != Some(&legacy) {
        packages.push(legacy);
    }
    packages
}

#[cfg(windows)]
fn canonical_process_path(path: PathBuf) -> PathBuf {
    use std::os::windows::ffi::OsStrExt;

    const WINDOWS_DIRECTORY_PATH_LIMIT: usize = 248;
    if path.as_os_str().encode_wide().count() < WINDOWS_DIRECTORY_PATH_LIMIT {
        return path;
    }
    fs::canonicalize(&path).unwrap_or(path)
}

#[cfg(not(windows))]
fn canonical_process_path(path: PathBuf) -> PathBuf {
    path
}

fn find_python_art_in_dir(arts_dir: &Path, art_id: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(arts_dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let art_json_path = path.join("art.json");
        if !art_json_path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(art_json_path) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        if json
            .get("art_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|candidate| candidate == art_id)
        {
            return Some(path);
        }
    }
    None
}

fn execute_cloud_api_tool(
    tool: &ToolDefinition,
    endpoint: &str,
    method: &str,
    content_type: Option<&str>,
    headers: Option<&str>,
    body: Option<&str>,
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    let endpoint = substitute_cloud_template(endpoint, &arguments);
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
    // Bypass the system proxy: on Windows reqwest picks up the OS proxy setting
    // (e.g. a stale 127.0.0.1:7890 from a stopped Clash/V2Ray), which makes every
    // cloud API call fail with "error sending request". Cloud arts talk directly
    // to their endpoint, matching Hook's own no_proxy client.
    let client =
        crate::network_policy::secure_client("Loom/0.1 Cloud API", CLOUD_API_TIMEOUT, policy)
            .map_err(|reason| ToolRegistryError::CloudSecurity {
                id: tool.id.clone(),
                endpoint: endpoint.clone(),
                reason,
            })?;
    let mut request = client.request(method.clone(), &endpoint);
    let mut explicit_content_type = false;
    if let Some(headers) = headers.filter(|value| !value.trim().is_empty()) {
        let rendered_headers = substitute_cloud_template(headers, &arguments);
        let header_map = serde_json::from_str::<HashMap<String, String>>(&rendered_headers)
            .map_err(|source| ToolRegistryError::CloudTemplate {
                id: tool.id.clone(),
                field: "headers",
                reason: source.to_string(),
            })?;
        for (name, value) in header_map {
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
            request = request.multipart(build_cloud_multipart_form(tool, body, &arguments)?);
        } else if let Some(body) = body {
            let rendered_body = substitute_cloud_template(body, &arguments);
            if content_type_lower.contains("json") {
                let json_body = serde_json::from_str::<serde_json::Value>(&rendered_body).map_err(
                    |source| ToolRegistryError::CloudTemplate {
                        id: tool.id.clone(),
                        field: "body",
                        reason: source.to_string(),
                    },
                )?;
                request = request.json(&json_body);
            } else {
                request = request.body(rendered_body);
                if !explicit_content_type {
                    request = request.header(reqwest::header::CONTENT_TYPE, content_type.clone());
                }
            }
        } else {
            request = request.json(&arguments);
        }
    }
    let mut response = request
        .send()
        .map_err(|source| ToolRegistryError::CloudRequest {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            source,
        })?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    const MAX_CLOUD_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CLOUD_RESPONSE_BYTES as u64)
    {
        return Err(ToolRegistryError::CloudSecurity {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            reason: format!("response exceeds {MAX_CLOUD_RESPONSE_BYTES} bytes"),
        });
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_CLOUD_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(ToolRegistryError::Io)?;
    if bytes.len() > MAX_CLOUD_RESPONSE_BYTES {
        return Err(ToolRegistryError::CloudSecurity {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            reason: format!("response exceeds {MAX_CLOUD_RESPONSE_BYTES} bytes"),
        });
    }

    if !status.is_success() {
        return Err(ToolRegistryError::CloudHttpStatus {
            id: tool.id.clone(),
            endpoint,
            status: status.as_u16(),
            body: String::from_utf8_lossy(&bytes).trim().to_owned(),
        });
    }

    normalize_cloud_response(tool, &endpoint, &content_type, &bytes)
}

fn cloud_network_policy(tool: &ToolDefinition) -> crate::network_policy::OutboundPolicy {
    let network = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("permissionPolicy"))
        .and_then(|policy| policy.get("network"));
    crate::network_policy::OutboundPolicy {
        allow_http_loopback: network
            .and_then(|network| network.get("allowLocalhost"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
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

fn build_cloud_multipart_form(
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
        if rendered_value.is_empty()
            || rendered_value == "__DISABLED__"
            || rendered_value.contains("{{")
        {
            continue;
        }

        if is_cloud_multipart_file_field(&key, &value) && Path::new(&rendered_value).exists() {
            form = form
                .file(key, &rendered_value)
                .map_err(ToolRegistryError::Io)?;
        } else {
            form = form.text(key, rendered_value);
        }
    }
    Ok(form)
}

fn is_cloud_multipart_file_field(key: &str, template_value: &str) -> bool {
    template_value.contains(".path}}")
        || template_value.contains("inputs.image}}")
        || matches!(key, "file" | "image" | "image_file")
        || key.ends_with("_file")
}

fn parse_cloud_method(tool: &ToolDefinition, method: &str) -> ToolRegistryResult<Method> {
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

fn normalize_cloud_response(
    tool: &ToolDefinition,
    endpoint: &str,
    content_type: &str,
    bytes: &[u8],
) -> ToolRegistryResult<serde_json::Value> {
    if content_type.to_ascii_lowercase().contains("image/") {
        let mime_type = content_type
            .split(';')
            .next()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("image/png")
            .trim();
        return Ok(image_content_response(
            &format!("data:{mime_type};base64,{}", BASE64.encode(bytes)),
            mime_type,
        ));
    }

    let body = String::from_utf8_lossy(bytes).trim().to_owned();
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
                body,
            })
        }
        Err(_) => Ok(text_content_response(&body)),
    }
}

fn normalize_cloud_json_value(value: serde_json::Value) -> serde_json::Value {
    if value
        .get("content")
        .and_then(serde_json::Value::as_array)
        .is_some()
    {
        return value;
    }
    if let Some(output) = value.get("output") {
        if let Some(data) = output.get("data").and_then(serde_json::Value::as_str) {
            let mime_type = output
                .get("mimeType")
                .or_else(|| output.get("mime_type"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("image/png");
            return image_content_response(data, mime_type);
        }
    }
    if let Some(data) = value.get("data").and_then(serde_json::Value::as_str) {
        let mime_type = value
            .get("mimeType")
            .or_else(|| value.get("mime_type"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("image/png");
        if data.starts_with("data:image/") || looks_like_base64_payload(data) {
            return image_content_response(data, mime_type);
        }
    }
    if let Some(text) = value.get("text").and_then(serde_json::Value::as_str) {
        return text_content_response(text);
    }
    text_content_response(&value.to_string())
}

#[derive(Clone, Debug)]
struct McpImageCandidate {
    image_url: String,
    title: Option<String>,
    thumbnail_url: Option<String>,
    source_page_url: Option<String>,
    width: Option<u64>,
    height: Option<u64>,
}

fn normalize_mcp_result(
    tool: &ToolDefinition,
    arguments: &serde_json::Value,
    value: serde_json::Value,
) -> serde_json::Value {
    if mcp_result_already_contains_image(&value) {
        return value;
    }
    if tool_expects_image_output(tool) {
        if let Some(image) = normalize_mcp_image_result(arguments, &value) {
            return image;
        }
        if let Some(message) = friendly_mcp_image_result_message(&value) {
            let candidates = collect_mcp_image_candidates(&value);
            if !candidates.is_empty() {
                let selected_index =
                    selected_mcp_image_candidate_index(arguments, candidates.len());
                let mut response = text_content_response(&message);
                attach_mcp_image_search_metadata(&mut response, &candidates, selected_index);
                return response;
            }
            return text_content_response(&message);
        }
    }
    value
}

fn mcp_result_already_contains_image(value: &serde_json::Value) -> bool {
    value
        .get("content")
        .or_else(|| value.get("result").and_then(|result| result.get("content")))
        .and_then(serde_json::Value::as_array)
        .map(|content| {
            content.iter().any(|item| {
                let item_type = item
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                match item_type {
                    "image" => item
                        .get("data")
                        .and_then(serde_json::Value::as_str)
                        .is_some(),
                    "text" => item
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .map(|text| {
                            let trimmed = text.trim();
                            trimmed.starts_with("data:image/") || looks_like_base64_payload(trimmed)
                        })
                        .unwrap_or(false),
                    _ => false,
                }
            })
        })
        .unwrap_or(false)
}

fn tool_expects_image_output(tool: &ToolDefinition) -> bool {
    tool.outputs.iter().any(value_declares_image_output)
        || tool
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("artloomCompat"))
            .and_then(|compat| compat.get("execution"))
            .and_then(|execution| execution.get("outputs"))
            .and_then(serde_json::Value::as_array)
            .map(|outputs| outputs.iter().any(value_declares_image_output))
            .unwrap_or(false)
}

fn value_declares_image_output(value: &serde_json::Value) -> bool {
    let output_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if output_type == "image" {
        return true;
    }
    let execution_type = value
        .get("execution_type")
        .or_else(|| value.get("executionType"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(execution_type.as_str(), "image_buffer" | "image_path")
}

fn normalize_mcp_image_result(
    arguments: &serde_json::Value,
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    let candidates = collect_mcp_image_candidates(value);
    if candidates.is_empty() {
        return None;
    }
    let requested_index = selected_mcp_image_candidate_index(arguments, candidates.len());
    let (mut normalized, selected_index) =
        image_response_from_mcp_candidates(&candidates, requested_index)?;
    attach_mcp_image_search_metadata(&mut normalized, &candidates, selected_index);
    Some(normalized)
}

fn friendly_mcp_image_result_message(value: &serde_json::Value) -> Option<String> {
    if let Some(message) = mcp_image_search_empty_result_message(value) {
        return Some(message);
    }
    let candidates = collect_mcp_image_candidates(value);
    if !candidates.is_empty() {
        return Some("图片搜索已返回候选结果，但图片下载失败，请稍后重试。".to_owned());
    }
    None
}

fn mcp_image_search_empty_result_message(value: &serde_json::Value) -> Option<String> {
    if let Some(message) =
        mcp_image_search_empty_result_message_from_payload(value.get("structuredContent"))
    {
        return Some(message);
    }
    if let Some(message) = mcp_image_search_empty_result_message_from_payload(
        value
            .get("result")
            .and_then(|result| result.get("structuredContent")),
    ) {
        return Some(message);
    }
    if let Some(content) = value
        .get("content")
        .or_else(|| value.get("result").and_then(|result| result.get("content")))
        .and_then(serde_json::Value::as_array)
    {
        for item in content {
            let Some(text) = item.get("text").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) else {
                continue;
            };
            if let Some(message) = mcp_image_search_empty_result_message_from_payload(Some(&parsed))
            {
                return Some(message);
            }
        }
    }
    None
}

fn mcp_image_search_empty_result_message_from_payload(
    payload: Option<&serde_json::Value>,
) -> Option<String> {
    let payload = payload?;
    let items_len = mcp_image_search_items_len(payload);
    let count = payload
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            payload
                .get("count")
                .and_then(serde_json::Value::as_str)
                .and_then(|raw| raw.parse::<u64>().ok())
        });
    let has_no_items = matches!(items_len, Some(0)) || matches!(count, Some(0));
    if !has_no_items {
        return None;
    }
    let provider_flagged_sensitive = payload
        .get("might_be_offensive")
        .or_else(|| payload.get("mightBeOffensive"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if provider_flagged_sensitive {
        return Some(
            "图片搜索未返回可用结果：搜索服务将该查询判定为可能敏感，请尝试更换关键词。".to_owned(),
        );
    }
    Some("图片搜索未返回可用结果，请尝试更换关键词。".to_owned())
}

fn mcp_image_search_items_len(value: &serde_json::Value) -> Option<usize> {
    match value.get("items") {
        Some(serde_json::Value::Array(items)) => Some(items.len()),
        Some(serde_json::Value::String(raw)) => serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|parsed| parsed.as_array().map(Vec::len)),
        _ => None,
    }
}

fn image_response_from_mcp_candidates(
    candidates: &[McpImageCandidate],
    requested_index: usize,
) -> Option<(serde_json::Value, usize)> {
    if candidates.is_empty() {
        return None;
    }
    for candidate_index in std::iter::once(requested_index).chain(
        candidates
            .iter()
            .enumerate()
            .map(|(index, _)| index)
            .filter(|index| *index != requested_index),
    ) {
        let candidate = candidates.get(candidate_index)?;
        if let Some(response) = image_response_from_mcp_candidate(candidate) {
            return Some((response, candidate_index));
        }
    }
    None
}

fn collect_mcp_image_candidates(value: &serde_json::Value) -> Vec<McpImageCandidate> {
    let mut candidates = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    if let Some(structured_content) = value.get("structuredContent") {
        collect_mcp_image_candidates_from_value(structured_content, &mut candidates, &mut seen);
    }
    if let Some(structured_content) = value
        .get("result")
        .and_then(|result| result.get("structuredContent"))
    {
        collect_mcp_image_candidates_from_value(structured_content, &mut candidates, &mut seen);
    }
    if candidates.is_empty() {
        if let Some(content) = value
            .get("content")
            .or_else(|| value.get("result").and_then(|result| result.get("content")))
            .and_then(serde_json::Value::as_array)
        {
            for item in content {
                let Some(text) = item.get("text").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) else {
                    continue;
                };
                collect_mcp_image_candidates_from_value(&parsed, &mut candidates, &mut seen);
            }
        }
    }
    candidates
}

fn collect_mcp_image_candidates_from_value(
    value: &serde_json::Value,
    candidates: &mut Vec<McpImageCandidate>,
    seen: &mut std::collections::BTreeSet<String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(candidate) = image_candidate_from_object(map) {
                if seen.insert(candidate.image_url.clone()) {
                    candidates.push(candidate);
                }
                return;
            }
            for child in map.values() {
                collect_mcp_image_candidates_from_value(child, candidates, seen);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_mcp_image_candidates_from_value(child, candidates, seen);
            }
        }
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if (looks_like_image_url(trimmed) || trimmed.starts_with("data:image/"))
                && seen.insert(trimmed.to_owned())
            {
                candidates.push(McpImageCandidate {
                    image_url: trimmed.to_owned(),
                    title: None,
                    thumbnail_url: None,
                    source_page_url: None,
                    width: None,
                    height: None,
                });
                return;
            }
            if matches!(trimmed.as_bytes().first(), Some(b'{') | Some(b'[')) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    collect_mcp_image_candidates_from_value(&parsed, candidates, seen);
                }
            }
        }
        _ => {}
    }
}

fn image_candidate_from_object(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<McpImageCandidate> {
    let properties = map.get("properties").and_then(serde_json::Value::as_object);
    let image_url =
        find_image_url_in_object(map).or_else(|| properties.and_then(find_image_url_in_object))?;
    let title = first_string(map, &["title", "label", "name"]).or_else(|| {
        properties.and_then(|object| first_string(object, &["title", "label", "name"]))
    });
    let thumbnail_url = first_imageish_string(
        map,
        &["thumbnail_url", "thumbnailUrl", "thumbnail", "placeholder"],
    )
    .or_else(|| {
        properties.and_then(|object| {
            first_imageish_string(
                object,
                &["thumbnail_url", "thumbnailUrl", "thumbnail", "placeholder"],
            )
        })
    });
    let width = first_u64(map, &["width"])
        .or_else(|| properties.and_then(|object| first_u64(object, &["width"])));
    let height = first_u64(map, &["height"])
        .or_else(|| properties.and_then(|object| first_u64(object, &["height"])));
    let source_page_url = first_string(map, &["source_page_url", "sourcePageUrl"]).or_else(|| {
        map.get("url")
            .and_then(serde_json::Value::as_str)
            .filter(|url| {
                *url != image_url && (url.starts_with("http://") || url.starts_with("https://"))
            })
            .map(str::to_owned)
    });
    Some(McpImageCandidate {
        image_url,
        title,
        thumbnail_url,
        source_page_url,
        width,
        height,
    })
}

fn strip_image_url_modifiers(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let query_or_fragment_index = trimmed.find(['?', '#']).unwrap_or(trimmed.len());
    let (head, tail) = trimmed.split_at(query_or_fragment_index);
    let lower = head.to_ascii_lowercase();
    let mut trimmed_end = None;
    for suffix in [
        ".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".svg", ".avif",
    ] {
        let mut search_start = 0usize;
        while let Some(relative_index) = lower[search_start..].find(suffix) {
            let index = search_start + relative_index;
            let end = index + suffix.len();
            let next = head[end..].chars().next();
            if matches!(next, None | Some('!') | Some('/')) {
                trimmed_end = Some(end);
            }
            search_start = index + 1;
        }
    }
    let Some(end) = trimmed_end else {
        return None;
    };
    let normalized = format!("{}{}", &head[..end], tail).trim().to_owned();
    if normalized.is_empty() || normalized == trimmed {
        return None;
    }
    Some(normalized)
}

fn normalize_image_candidate_url(
    value: &str,
    allow_remote_without_extension: bool,
) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.starts_with("data:image/") || looks_like_image_url(trimmed) {
        return Some(trimmed.to_owned());
    }
    if let Some(stripped) = strip_image_url_modifiers(trimmed) {
        if looks_like_image_url(&stripped)
            || (allow_remote_without_extension && looks_like_remote_url(&stripped))
        {
            return Some(stripped);
        }
    }
    if allow_remote_without_extension && looks_like_remote_url(trimmed) {
        return Some(trimmed.to_owned());
    }
    None
}

fn find_image_url_in_object(map: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    for key in [
        "image_url",
        "imageUrl",
        "thumbnail_url",
        "thumbnailUrl",
        "src",
        "data",
    ] {
        if let Some(url) = map
            .get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(|value| normalize_image_candidate_url(value, true))
        {
            return Some(url);
        }
    }
    let url = map.get("url").and_then(serde_json::Value::as_str)?;
    if let Some(normalized) =
        normalize_image_candidate_url(url, object_looks_like_image_result(map))
    {
        return Some(normalized);
    }
    None
}

fn first_imageish_string(
    map: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        let Some(value) = map.get(*key) else {
            continue;
        };
        let key_implies_image = matches!(
            *key,
            "thumbnail_url" | "thumbnailUrl" | "thumbnail" | "placeholder"
        );
        match value {
            serde_json::Value::String(text) => {
                if let Some(url) = normalize_image_candidate_url(text, key_implies_image) {
                    return Some(url);
                }
            }
            serde_json::Value::Object(object) => {
                if let Some(url) = first_string(
                    object,
                    &[
                        "src",
                        "url",
                        "image_url",
                        "imageUrl",
                        "thumbnail_url",
                        "thumbnailUrl",
                    ],
                )
                .and_then(|candidate| normalize_image_candidate_url(&candidate, key_implies_image))
                {
                    return Some(url);
                }
            }
            _ => {}
        }
    }
    None
}

fn first_string(map: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}

fn first_u64(map: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(serde_json::Value::as_u64))
}

fn selected_mcp_image_candidate_index(
    arguments: &serde_json::Value,
    candidate_count: usize,
) -> usize {
    if candidate_count == 0 {
        return 0;
    }
    let selected = arguments
        .as_object()
        .and_then(|object| {
            [
                "result_index",
                "resultIndex",
                "selected_index",
                "selectedIndex",
                "image_index",
            ]
            .iter()
            .find_map(|key| object.get(*key))
        })
        .and_then(value_as_usize)
        .unwrap_or(0);
    selected.min(candidate_count.saturating_sub(1))
}

fn value_as_usize(value: &serde_json::Value) -> Option<usize> {
    match value {
        serde_json::Value::Number(number) => number.as_u64().map(|value| value as usize),
        serde_json::Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

fn attach_mcp_image_search_metadata(
    image_result: &mut serde_json::Value,
    candidates: &[McpImageCandidate],
    selected_index: usize,
) {
    let Some(result_object) = image_result.as_object_mut() else {
        return;
    };
    result_object.insert(
        "loomMetadata".to_owned(),
        serde_json::json!({
            "imageSearch": {
                "selectedIndex": selected_index,
                "candidates": candidates
                    .iter()
                    .enumerate()
                    .map(|(index, candidate)| serde_json::json!({
                        "index": index,
                        "title": candidate.title,
                        "imageUrl": candidate.image_url,
                        "thumbnailUrl": candidate.thumbnail_url,
                        "sourcePageUrl": candidate.source_page_url,
                        "width": candidate.width,
                        "height": candidate.height
                    }))
                    .collect::<Vec<_>>()
            }
        }),
    );
}

fn object_looks_like_image_result(map: &serde_json::Map<String, serde_json::Value>) -> bool {
    map.contains_key("width")
        || map.contains_key("height")
        || map.contains_key("thumbnail_url")
        || map.contains_key("thumbnailUrl")
        || map
            .get("mimeType")
            .or_else(|| map.get("mime_type"))
            .and_then(serde_json::Value::as_str)
            .map(|mime| mime.starts_with("image/"))
            .unwrap_or(false)
}

fn looks_like_image_url(value: &str) -> bool {
    if value.starts_with("data:image/") {
        return true;
    }
    if !looks_like_remote_url(value) {
        return false;
    }
    let path = value
        .split('?')
        .next()
        .unwrap_or(value)
        .split('#')
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".svg", ".avif",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix))
}

fn looks_like_remote_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn download_mcp_image_candidate(url: &str, referer: Option<&str>) -> Option<serde_json::Value> {
    download_mcp_image_candidate_with_reqwest(url, referer)
        .or_else(|| download_mcp_image_candidate_with_platform_fallback(url, referer))
}

fn download_mcp_image_candidate_with_reqwest(
    url: &str,
    referer: Option<&str>,
) -> Option<serde_json::Value> {
    let policy = network_policy::OutboundPolicy {
        allow_http_loopback: true,
        ..network_policy::OutboundPolicy::default()
    };
    let parsed_url = reqwest::Url::parse(url).ok()?;
    network_policy::validate_outbound_url(&parsed_url, &policy).ok()?;
    let client =
        network_policy::secure_client(MCP_IMAGE_FETCH_USER_AGENT, CLOUD_API_TIMEOUT, policy)
            .ok()?;
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
        .filter(|value| value.starts_with("image/"))
        .map(str::to_owned);
    let bytes = network_policy::read_bounded_response(response, MAX_MCP_IMAGE_BYTES).ok()?;
    let mime_type = header_mime_type
        .or_else(|| infer_image_mime_type_from_url(url))
        .or_else(|| infer_image_mime_type_from_bytes(&bytes))?;
    Some(image_content_response(&BASE64.encode(&bytes), &mime_type))
}

#[cfg(windows)]
fn download_mcp_image_candidate_with_platform_fallback(
    url: &str,
    referer: Option<&str>,
) -> Option<serde_json::Value> {
    let (mime_type, bytes) = download_image_bytes_with_powershell_httpclient(url, referer)?;
    Some(image_content_response(&BASE64.encode(&bytes), &mime_type))
}

#[cfg(not(windows))]
fn download_mcp_image_candidate_with_platform_fallback(
    _url: &str,
    _referer: Option<&str>,
) -> Option<serde_json::Value> {
    None
}

#[cfg(windows)]
fn download_image_bytes_with_powershell_httpclient(
    url: &str,
    referer: Option<&str>,
) -> Option<(String, Vec<u8>)> {
    let policy = network_policy::OutboundPolicy {
        allow_http_loopback: true,
        ..network_policy::OutboundPolicy::default()
    };
    let parsed_url = reqwest::Url::parse(url).ok()?;
    network_policy::validate_outbound_url(&parsed_url, &policy).ok()?;
    let script = r#"
Add-Type -AssemblyName System.Net.Http
$handler = New-Object System.Net.Http.HttpClientHandler
$handler.AllowAutoRedirect = $false
$client = New-Object System.Net.Http.HttpClient($handler)
$client.Timeout = [TimeSpan]::FromSeconds(30)
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
    process.limits.timeout = CLOUD_API_TIMEOUT;
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
        .filter(|value| value.starts_with("image/"))
        .map(str::to_owned)
        .or_else(|| infer_image_mime_type_from_url(url))
        .or_else(|| infer_image_mime_type_from_bytes(&bytes))?;
    Some((mime_type, bytes))
}

fn image_response_from_mcp_candidate(candidate: &McpImageCandidate) -> Option<serde_json::Value> {
    let referer = candidate.source_page_url.as_deref();
    image_response_from_mcp_candidate_url(&candidate.image_url, referer).or_else(|| {
        candidate
            .thumbnail_url
            .as_deref()
            .filter(|thumbnail_url| *thumbnail_url != candidate.image_url)
            .and_then(|thumbnail_url| image_response_from_mcp_candidate_url(thumbnail_url, referer))
    })
}

fn image_response_from_mcp_candidate_url(
    url: &str,
    referer: Option<&str>,
) -> Option<serde_json::Value> {
    if url.starts_with("data:image/") {
        if url.len() > MAX_MCP_IMAGE_BYTES.saturating_mul(4) / 3 + 4096 {
            return None;
        }
        let mime_type = data_url_mime_type(url).unwrap_or("image/png");
        return Some(image_content_response(url, mime_type));
    }
    for candidate_url in std::iter::once(url.to_owned()).chain(
        strip_image_url_modifiers(url)
            .into_iter()
            .filter(|normalized| normalized != url),
    ) {
        if let Some(response) = download_mcp_image_candidate(&candidate_url, referer) {
            return Some(response);
        }
    }
    None
}

fn infer_image_mime_type_from_url(url: &str) -> Option<String> {
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
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".avif") {
        "image/avif"
    } else {
        return None;
    };
    Some(mime_type.to_owned())
}

fn infer_image_mime_type_from_bytes(bytes: &[u8]) -> Option<String> {
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
    } else {
        return None;
    };
    Some(mime_type.to_owned())
}

fn data_url_mime_type(data_url: &str) -> Option<&str> {
    let data_url = data_url.strip_prefix("data:")?;
    let mime_type = data_url.split(';').next()?.trim();
    (!mime_type.is_empty()).then_some(mime_type)
}

fn image_content_response(data: &str, mime_type: &str) -> serde_json::Value {
    let data = if data.starts_with("data:image/") && data.contains(";base64,") {
        data.to_owned()
    } else {
        format!("data:{mime_type};base64,{data}")
    };
    serde_json::json!({
        "content": [
            {
                "type": "image",
                "data": data,
                "mimeType": mime_type
            }
        ]
    })
}

fn text_content_response(text: &str) -> serde_json::Value {
    serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ]
    })
}

fn looks_like_base64_payload(value: &str) -> bool {
    value.len() >= 8
        && !value.chars().any(char::is_whitespace)
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '-' | '_'))
}

fn substitute_cloud_template(template: &str, arguments: &serde_json::Value) -> String {
    let mut rendered = template.to_owned();
    let Some(arguments) = arguments.as_object() else {
        return rendered;
    };
    for (key, value) in arguments {
        let replacement = scalar_template_value(value);
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}.value}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}.path}}}}"), &replacement);
    }
    rendered
}

fn scalar_template_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

// Substitute one CLI arg token: {{input}}/{input} → input path, {{output}}/
// {output} → output path, and {{key}}/{key} → param values (bool → arg_true/
// arg_false or -key/--key flag forms). Mirrors Hook's CliEngine rules.
fn substitute_cli_token(
    token: &str,
    input_path: &str,
    output_path: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let mut out = token
        .replace("{{input}}", input_path)
        .replace("{{output}}", output_path)
        .replace("{input}", input_path)
        .replace("{output}", output_path);
    for (key, value) in params {
        let s_val = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Bool(b) => b.to_string(),
            other => other.to_string(),
        };
        let (flag_single, flag_double) = match value {
            serde_json::Value::Bool(true) => (format!("-{key}"), format!("--{key}")),
            serde_json::Value::Bool(false) => (String::new(), String::new()),
            serde_json::Value::String(s) => (s.clone(), s.clone()),
            _ => (s_val.clone(), s_val.clone()),
        };
        // Double-brace before single-brace so {key} doesn't match inside {{key}}.
        out = out
            .replace(&format!("{{{{-{key}}}}}"), &flag_single)
            .replace(&format!("{{{{--{key}}}}}"), &flag_double)
            .replace(&format!("{{{{{key}}}}}"), &s_val)
            .replace(&format!("{{{key}}}"), &s_val);
    }
    out
}

// Execute a cli_wrapper art in the image flow: decode the input image to a temp
// file, pre-copy it to an output file (so in-place tools like pingo work),
// substitute the command/args templates, run the process, then read the output
// file back as a base64 image. Returns the content-array shape the AHRP flow
// expects. Best-effort: input must be a decodable image container.
fn execute_cli_wrapper_tool(
    tool: &ToolDefinition,
    command: &str,
    args: &[String],
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    let obj = arguments.as_object().cloned().unwrap_or_default();
    let input_field = obj
        .get("input_base64")
        .and_then(serde_json::Value::as_str)
        .or_else(|| obj.get("input").and_then(serde_json::Value::as_str))
        .ok_or_else(|| ToolRegistryError::CliWrapperFailed {
            id: tool.id.clone(),
            reason: "missing input image".to_owned(),
        })?;
    let input_bytes = loom_image_io::decode_data_url_bytes(input_field).map_err(|error| {
        ToolRegistryError::CliWrapperFailed {
            id: tool.id.clone(),
            reason: format!("decode input image: {error}"),
        }
    })?;

    // Params = all arguments except the image input keys.
    let mut params = obj.clone();
    params.remove("input_base64");
    params.remove("input");

    let temp_dir = std::env::temp_dir().join("loom_cli");
    fs::create_dir_all(&temp_dir).map_err(|error| ToolRegistryError::CliWrapperFailed {
        id: tool.id.clone(),
        reason: format!("temp dir: {error}"),
    })?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let input_path = temp_dir.join(format!("{stamp}_in.png"));
    let output_path = temp_dir.join(format!("{stamp}_out.png"));
    fs::write(&input_path, &input_bytes).map_err(|error| ToolRegistryError::CliWrapperFailed {
        id: tool.id.clone(),
        reason: format!("write input: {error}"),
    })?;
    // Pre-fill output with input so in-place tools have a target.
    let _ = fs::copy(&input_path, &output_path);

    let input_str = input_path.to_string_lossy().to_string();
    let output_str = output_path.to_string_lossy().to_string();
    let program = substitute_cli_token(command, &input_str, &output_str, &params);
    let cli_args: Vec<String> = args
        .iter()
        .map(|arg| substitute_cli_token(arg, &input_str, &output_str, &params))
        .filter(|arg| !arg.is_empty())
        .collect();

    let mut cmd = Command::new(&program);
    cmd.args(&cli_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut process = ProcessSpec::from_command(&cmd);
    process.limits.timeout = SCRIPT_EXECUTION_TIMEOUT;
    let output = loom_process::run_with_input(&process, b"").map_err(|error| {
        ToolRegistryError::CliWrapperFailed {
            id: tool.id.clone(),
            reason: format!("run `{program}`: {error}"),
        }
    })?;
    if !output.status.success() {
        return Err(ToolRegistryError::CliWrapperFailed {
            id: tool.id.clone(),
            reason: format!(
                "process exited with {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }

    if !output_path.exists() {
        return Err(ToolRegistryError::CliWrapperFailed {
            id: tool.id.clone(),
            reason: format!(
                "no output produced (exit {:?}): {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    let out_bytes =
        fs::read(&output_path).map_err(|error| ToolRegistryError::CliWrapperFailed {
            id: tool.id.clone(),
            reason: format!("read output: {error}"),
        })?;
    let data = BASE64.encode(&out_bytes);
    Ok(serde_json::json!({
        "content": [ { "type": "image", "data": data, "mimeType": "image/png" } ]
    }))
}

fn execute_script_tool(
    tool: &ToolDefinition,
    path: &str,
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    let script_path = Path::new(path);
    if !script_path.exists() {
        return Err(ToolRegistryError::ScriptNotFound {
            id: tool.id.clone(),
            path: path.to_owned(),
        });
    }

    let payload = serde_json::to_string(&serde_json::json!({
        "tool_id": tool.id,
        "arguments": arguments,
    }))?;
    let output = run_script_process(tool, path, &payload)?;

    if !output.status.success() {
        return Err(ToolRegistryError::ScriptFailed {
            id: tool.id.clone(),
            path: path.to_owned(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if stdout.is_empty() {
        return Err(ToolRegistryError::ScriptEmptyStdout {
            id: tool.id.clone(),
            path: path.to_owned(),
        });
    }

    serde_json::from_str(&stdout).map_err(|source| ToolRegistryError::ScriptJson {
        id: tool.id.clone(),
        path: path.to_owned(),
        source,
        stdout,
    })
}

fn run_script_process(
    tool: &ToolDefinition,
    path: &str,
    payload: &str,
) -> ToolRegistryResult<Output> {
    let ScriptInvocation {
        command,
        staged_files,
    } = script_command(path, payload).map_err(|source| ToolRegistryError::ScriptSpawn {
        id: tool.id.clone(),
        path: path.to_owned(),
        source,
    })?;
    let mut process = ProcessSpec::from_command(&command);
    process.limits.timeout = SCRIPT_EXECUTION_TIMEOUT;
    let result = loom_process::run_with_input(&process, b"");
    cleanup_staged_script_files(&staged_files);
    match result {
        Ok(output) => Ok(Output {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        }),
        Err(ProcessError::Timeout { .. }) => Err(ToolRegistryError::ScriptTimedOut {
            id: tool.id.clone(),
            path: path.to_owned(),
            timeout_ms: SCRIPT_EXECUTION_TIMEOUT.as_millis(),
        }),
        Err(error) => Err(ToolRegistryError::ScriptSpawn {
            id: tool.id.clone(),
            path: path.to_owned(),
            source: std::io::Error::other(error.to_string()),
        }),
    }
}

struct ScriptInvocation {
    command: Command,
    staged_files: Vec<PathBuf>,
}

fn script_command(path: &str, payload: &str) -> std::io::Result<ScriptInvocation> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if extension == "ps1" {
        let payload_path = write_staged_script_file("payload", "json", payload.as_bytes())?;
        let wrapper = br#"
param(
    [Parameter(Mandatory = $true)][string]$PayloadPath,
    [Parameter(Mandatory = $true)][string]$ScriptPath
)
$ErrorActionPreference = 'Stop'
$payload = [System.IO.File]::ReadAllText($PayloadPath, [System.Text.UTF8Encoding]::new($false))
& $ScriptPath $payload
"#;
        let wrapper_path = write_staged_script_file("wrapper", "ps1", wrapper)?;
        let mut command = Command::new("powershell.exe");
        command
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&wrapper_path)
            .arg(&payload_path)
            .arg(path);
        return Ok(ScriptInvocation {
            command,
            staged_files: vec![payload_path, wrapper_path],
        });
    }

    if extension == "py" {
        let payload_path = write_staged_script_file("payload", "json", payload.as_bytes())?;
        let wrapper = br#"
import pathlib
import runpy
import sys

payload = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
script = sys.argv[2]
sys.argv = [script, payload]
runpy.run_path(script, run_name="__main__")
"#;
        let wrapper_path = write_staged_script_file("wrapper", "py", wrapper)?;
        let mut command = Command::new(resolve_python_executable());
        configure_python_process(&mut command);
        command.arg(&wrapper_path).arg(&payload_path).arg(path);
        return Ok(ScriptInvocation {
            command,
            staged_files: vec![payload_path, wrapper_path],
        });
    }

    let mut command = Command::new(path);
    command.arg(payload);
    Ok(ScriptInvocation {
        command,
        staged_files: Vec::new(),
    })
}

fn write_staged_script_file(stem: &str, extension: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = REGISTRY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("loom-script-{stem}-{nonce}-{sequence}.{extension}"));
    fs::write(&path, bytes)?;
    Ok(path)
}

fn cleanup_staged_script_files(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn configure_python_process(command: &mut Command) {
    command.env("PYTHONDONTWRITEBYTECODE", "1");
}

fn framework_packages_root_env() -> Option<String> {
    ["LOOM_FRAMEWORK_PACKAGES_DIR", "LOOM_FRAMEWORK_RUNTIMES_DIR"]
        .into_iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
}

fn resolve_python_executable() -> PathBuf {
    let loom_python = std::env::var("LOOM_PYTHON").ok();
    let framework_runtime_root = framework_packages_root_env();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let current_dir = std::env::current_dir().ok();

    resolve_python_executable_from(
        loom_python.as_deref(),
        framework_runtime_root.as_deref(),
        exe_dir.as_deref(),
        current_dir.as_deref(),
    )
}

fn resolve_python_executable_from(
    loom_python: Option<&str>,
    framework_runtime_root: Option<&str>,
    exe_dir: Option<&Path>,
    current_dir: Option<&Path>,
) -> PathBuf {
    if let Some(override_python) = loom_python.map(str::trim).filter(|value| !value.is_empty()) {
        return PathBuf::from(override_python);
    }

    if let Some(runtime_root) = framework_runtime_root
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        for package_dir in python_framework_package_dirs(Path::new(runtime_root)) {
            let candidate = package_dir.join("python-embed").join("python.exe");
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    let mut candidates = Vec::new();
    if let Some(exe_dir) = exe_dir {
        candidates.push(exe_dir.join("bin").join("python-embed").join("python.exe"));
    }
    if let Some(current_dir) = current_dir {
        candidates.push(
            current_dir
                .join("bin")
                .join("python-embed")
                .join("python.exe"),
        );
        candidates.push(
            current_dir
                .join("Loom")
                .join("resources")
                .join("python-embed")
                .join("python.exe"),
        );
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("python"))
}

fn execution_type_name(execution: &ToolExecution) -> &'static str {
    match execution {
        ToolExecution::CliWrapper { .. } => "cli_wrapper",
        ToolExecution::CloudApi { .. } => "cloud_api",
        ToolExecution::Script { .. } => "script",
        ToolExecution::PythonArt { .. } => "python_art",
        ToolExecution::Mcp { .. } => "mcp",
        ToolExecution::Workflow { .. } => "workflow",
        ToolExecution::FrameworkArt { .. } => "framework_art",
    }
}

fn require_non_empty(tool_id: &str, value: &str, field: &str) -> ToolRegistryResult<()> {
    if value.trim().is_empty() {
        return Err(ToolRegistryError::InvalidToolDefinition {
            id: tool_id.to_owned(),
            reason: format!("{field} is required"),
        });
    }
    Ok(())
}

fn require_no_path_separator(tool_id: &str, value: &str) -> ToolRegistryResult<()> {
    if value.contains("..") || value.contains('/') || value.contains('\\') || value.contains(':') {
        return Err(ToolRegistryError::InvalidToolDefinition {
            id: tool_id.to_owned(),
            reason: "id cannot contain path separators".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::io::{BufRead, Read, Write};
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn cli_token_substitutes_paths_params_and_flags() {
        let mut params = serde_json::Map::new();
        params.insert("level_num".to_owned(), serde_json::json!(6));
        params.insert("lossless".to_owned(), serde_json::json!(true));
        params.insert("off_flag".to_owned(), serde_json::json!(false));
        let sub = |t: &str| super::substitute_cli_token(t, "IN.png", "OUT.png", &params);
        assert_eq!(sub("{{input}}"), "IN.png");
        assert_eq!(sub("{output}"), "OUT.png");
        assert_eq!(sub("-s{{level_num}}"), "-s6");
        // {{-key}} bool-true → -key flag; bool-false → empty.
        assert_eq!(sub("{{-lossless}}"), "-lossless");
        assert_eq!(sub("{{-off_flag}}"), "");
    }
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("loom-tool-registry-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp tool registry root");
        root
    }

    #[cfg(windows)]
    #[test]
    fn registry_file_replacement_supports_extended_length_paths() {
        let root = temp_root("long-registry-path");
        let mut directory = root.clone();
        while directory.as_os_str().to_string_lossy().len() < 270 {
            directory = directory.join("extended-registry-segment");
        }
        fs::create_dir_all(&directory).expect("create extended-length directory");
        let source = directory.join("registry.json.tmp");
        let destination = directory.join("registry.json");
        fs::write(&source, b"replacement").expect("write temporary registry file");

        replace_registry_file(&source, &destination)
            .expect("atomically replace registry file at an extended-length path");

        assert!(!source.exists());
        assert_eq!(
            fs::read(&destination).expect("read registry file"),
            b"replacement"
        );
        fs::remove_dir_all(root).expect("remove extended-length test directory");
    }

    #[test]
    fn python_framework_paths_follow_the_active_immutable_version() {
        let root = temp_root("python-framework-active-version");
        let package_root = root.join("python_art");
        let active = package_root.join("versions").join("1.0.0-deadbeef0000");
        fs::create_dir_all(active.join("python")).expect("create active Python framework");
        fs::write(active.join("framework.manifest.json"), b"{}").expect("write framework manifest");
        fs::write(
            package_root.join("active.json"),
            br#"{"active":"versions/1.0.0-deadbeef0000"}"#,
        )
        .expect("write framework activation");

        assert_eq!(
            python_framework_package_dirs(&root),
            vec![active, package_root]
        );
        fs::remove_dir_all(root).expect("cleanup Python framework path fixture");
    }

    #[cfg(windows)]
    #[test]
    fn canonical_process_path_supports_extended_length_executables() {
        let root = temp_root("long-process-path");
        let mut directory = root.clone();
        while directory.as_os_str().to_string_lossy().len() < 270 {
            directory = directory.join("extended-process-segment");
        }
        fs::create_dir_all(&directory).expect("create extended process directory");
        let executable = directory.join("python.exe");
        fs::write(&executable, b"fixture").expect("write extended process fixture");

        let canonical = canonical_process_path(executable);
        assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
        fs::remove_dir_all(root).expect("cleanup extended process fixture");
    }

    #[cfg(windows)]
    #[test]
    fn canonical_process_path_preserves_ordinary_windows_paths() {
        let root = temp_root("ordinary-process-path");
        let executable = root.join("python.exe");
        fs::write(&executable, b"fixture").expect("write ordinary process fixture");

        assert_eq!(canonical_process_path(executable.clone()), executable);
        fs::remove_dir_all(root).expect("cleanup ordinary process fixture");
    }

    #[test]
    fn mcp_tool_definition_requires_server_and_tool_name() {
        let missing_server = ToolDefinition::new(
            "brave-search",
            "Brave Search",
            "Search the web through MCP",
            ToolExecution::Mcp {
                server_id: String::new(),
                tool_name: "brave_web_search".to_owned(),
            },
        );
        assert!(missing_server.validate().is_err());

        let missing_tool = ToolDefinition::new(
            "brave-search",
            "Brave Search",
            "Search the web through MCP",
            ToolExecution::Mcp {
                server_id: "brave".to_owned(),
                tool_name: " ".to_owned(),
            },
        );
        assert!(missing_tool.validate().is_err());

        let valid = ToolDefinition::new(
            "brave-search",
            "Brave Search",
            "Search the web through MCP",
            ToolExecution::Mcp {
                server_id: "brave".to_owned(),
                tool_name: "brave_web_search".to_owned(),
            },
        );
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn workflow_tool_definition_requires_workflow_id() {
        let invalid = ToolDefinition::new(
            "paint-flow",
            "Paint Flow",
            "Run a saved workflow",
            ToolExecution::Workflow {
                workflow_id: String::new(),
                workflow_bindings: None,
            },
        );
        assert!(invalid.validate().is_err());

        let valid = ToolDefinition::new(
            "paint-flow",
            "Paint Flow",
            "Run a saved workflow",
            ToolExecution::Workflow {
                workflow_id: "workflow-1".to_owned(),
                workflow_bindings: None,
            },
        );
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn framework_art_tool_definition_requires_a_safe_framework_id() {
        let invalid = ToolDefinition::new(
            "third-party-art",
            "Third-party Art",
            "Reject a framework path instead of treating it as a package id",
            ToolExecution::FrameworkArt {
                framework: "../outside".to_owned(),
            },
        );
        assert!(matches!(
            invalid.validate(),
            Err(ToolRegistryError::InvalidToolDefinition { reason, .. })
                if reason.contains("safe package id")
        ));

        let valid = ToolDefinition::new(
            "third-party-art",
            "Third-party Art",
            "Accept a safe dynamic framework id",
            ToolExecution::FrameworkArt {
                framework: "third-party.echo-v2".to_owned(),
            },
        );
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn python_art_tool_definition_accepts_installed_art_contract() {
        let tool: ToolDefinition = serde_json::from_str(
            r#"{
              "id": "python-art-loom-echo",
              "name": "Loom Echo",
              "description": "Installed Python Art",
              "enabled": true,
              "execution": {
                "type": "python_art",
                "artId": "loom_echo",
                "artPath": "python/Arts/Art_LoomEcho"
              }
            }"#,
        )
        .expect("deserialize Python Art tool definition");

        assert!(tool.validate().is_ok());
        assert_eq!(execution_type_name(&tool.execution), "python_art");
        assert!(matches!(
            tool.execution,
            ToolExecution::PythonArt {
                ref art_id,
                ref art_path,
            } if art_id == "loom_echo" && art_path.as_deref() == Some("python/Arts/Art_LoomEcho")
        ));
    }

    #[test]
    fn tool_definition_preserves_desktop_port_metadata() {
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "advanced-cli-art",
            "name": "Advanced CLI Art",
            "description": "Desktop Add Art advanced ports",
            "enabled": true,
            "execution": {
                "type": "cli_wrapper",
                "command": "ffmpeg",
                "args": ["-i", "{{inputs.image.path}}", "{{outputs.result.path}}"]
            },
            "inputs": [{
                "name": "image",
                "label": "Image",
                "type": "image",
                "executionType": "image_path",
                "default": "input.png"
            }],
            "outputs": [{
                "name": "result",
                "label": "Result",
                "type": "image",
                "executionType": "image_path",
                "captureMode": "derived_template",
                "filename": "{{inputs.image.path}}_out.png"
            }],
            "params": [{
                "id": "shaderMode",
                "label": "Shader mode",
                "widget": "checkbox",
                "dataType": "bool",
                "default": true
            }]
        }))
        .expect("deserialize advanced Add Art tool definition");

        assert_eq!(tool.inputs[0]["name"], "image");
        assert_eq!(tool.outputs[0]["captureMode"], "derived_template");
        assert_eq!(tool.params[0]["id"], "shaderMode");

        let serialized =
            serde_json::to_value(&tool).expect("serialize advanced Add Art tool definition");
        assert_eq!(serialized["inputs"][0]["executionType"], "image_path");
        assert_eq!(
            serialized["outputs"][0]["filename"],
            "{{inputs.image.path}}_out.png"
        );
        assert_eq!(serialized["params"][0]["default"], true);
    }

    #[test]
    fn legacy_execution_types_remain_representable() {
        let tools = [
            ToolDefinition::new(
                "ffmpeg",
                "FFmpeg",
                "Wrap local ffmpeg",
                ToolExecution::CliWrapper {
                    command: "ffmpeg".to_owned(),
                    args: vec!["-version".to_owned()],
                },
            ),
            ToolDefinition::new(
                "cloud-image",
                "Cloud Image",
                "Call a cloud image API",
                ToolExecution::CloudApi {
                    endpoint: "https://example.invalid/generate".to_owned(),
                    method: "POST".to_owned(),
                    content_type: None,
                    headers: None,
                    body: None,
                },
            ),
            ToolDefinition::new(
                "python-filter",
                "Python Filter",
                "Run a local script",
                ToolExecution::Script {
                    path: "filters/enhance.py".to_owned(),
                },
            ),
        ];

        for tool in tools {
            tool.validate().expect("legacy execution type validates");
        }
    }

    #[test]
    fn framework_art_execution_type_deserializes_without_host_specific_fields() {
        let value = serde_json::from_value::<ToolDefinition>(serde_json::json!({
            "id": "third-party-art",
            "name": "Third-party Art",
            "description": "External framework Art",
            "enabled": true,
            "execution": {
                "type": "framework_art",
                "framework": "script"
            }
        }));
        assert!(
            value.is_ok(),
            "framework_art execution should deserialize: {value:?}"
        );
    }

    #[test]
    fn python_script_command_disables_bytecode_writes() {
        let command = script_command("fixture.py", "{}").expect("build python script command");
        let value = command
            .command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new("PYTHONDONTWRITEBYTECODE"))
            .and_then(|(_, value)| value.map(|value| value.to_string_lossy().to_string()));

        assert_eq!(value.as_deref(), Some("1"));
    }

    #[test]
    fn registry_save_update_delete_roundtrip() {
        let root = temp_root("roundtrip");
        let registry = ToolRegistry::new(&root);

        let tool = ToolDefinition::new(
            "brave-search",
            "Brave Search",
            "Search the web through MCP",
            ToolExecution::Mcp {
                server_id: "brave".to_owned(),
                tool_name: "brave_web_search".to_owned(),
            },
        );

        registry.save_tool(tool.clone()).expect("save tool");
        assert!(root.join("tools.json").exists());
        assert_eq!(
            registry.list_tools().expect("list tools"),
            vec![tool.clone()]
        );
        assert_eq!(
            registry.get_tool("brave-search").expect("get tool"),
            Some(tool.clone())
        );

        let updated = ToolDefinition {
            name: "Brave Web Search".to_owned(),
            enabled: false,
            ..tool
        };
        registry.save_tool(updated.clone()).expect("update tool");
        assert_eq!(
            registry.get_tool("brave-search").expect("get updated"),
            Some(updated)
        );

        assert!(registry.delete_tool("brave-search").expect("delete tool"));
        assert!(registry.list_tools().expect("list after delete").is_empty());
        assert!(!registry.delete_tool("brave-search").expect("delete absent"));

        fs::remove_dir_all(root).expect("cleanup temp tool registry root");
    }

    #[test]
    fn registry_keeps_same_local_id_isolated_by_publisher() {
        let root = temp_root("publisher-namespace");
        let registry = ToolRegistry::new(&root);
        let make_tool = |publisher: &str, name: &str| {
            let mut tool = ToolDefinition::new(
                "shared-art",
                name,
                "Publisher-scoped Art",
                ToolExecution::CliWrapper {
                    command: "echo".to_owned(),
                    args: vec!["ok".to_owned()],
                },
            );
            tool.metadata = Some(serde_json::json!({
                "packageSecurity": {
                    "publisher": { "id": publisher, "name": publisher }
                }
            }));
            tool
        };
        let alpha = make_tool("publisher.alpha", "Alpha");
        let beta = make_tool("publisher.beta", "Beta");
        registry.save_tool(alpha.clone()).expect("save alpha");
        registry.save_tool(beta.clone()).expect("save beta");

        assert_eq!(registry.list_tools().expect("list").len(), 2);
        assert_eq!(
            registry
                .get_tool("publisher.alpha/shared-art")
                .expect("get qualified alpha"),
            Some(alpha)
        );
        assert!(matches!(
            registry.get_tool("shared-art"),
            Err(ToolRegistryError::AmbiguousToolId { .. })
        ));
        assert!(registry
            .delete_tool("publisher.beta/shared-art")
            .expect("delete qualified beta"));
        assert_eq!(
            registry
                .get_tool("shared-art")
                .expect("bare id becomes unambiguous")
                .expect("remaining alpha")
                .name,
            "Alpha"
        );
        fs::remove_dir_all(root).expect("cleanup publisher namespace registry");
    }

    #[test]
    fn registry_recovers_trailing_json_and_quarantines_original() {
        let root = temp_root("trailing-json");
        fs::create_dir_all(&root).expect("create registry root");
        let tool = ToolDefinition::new(
            "recovered-tool",
            "Recovered Tool",
            "Tool from a recoverable registry",
            ToolExecution::CliWrapper {
                command: "echo".to_owned(),
                args: vec!["ok".to_owned()],
            },
        );
        let valid = serde_json::to_string_pretty(&vec![tool.clone()]).expect("serialize tool");
        let corrupted = format!("{valid}\n  }}  }}\n]");
        fs::write(root.join("tools.json"), &corrupted).expect("write corrupted registry");

        let registry = ToolRegistry::new(&root);
        assert_eq!(registry.list_tools().expect("recover tools"), vec![tool]);

        let canonical =
            fs::read_to_string(root.join("tools.json")).expect("read repaired registry");
        let parsed: Vec<ToolDefinition> =
            serde_json::from_str(&canonical).expect("repaired registry is valid JSON");
        assert_eq!(parsed.len(), 1);

        let backups = fs::read_dir(&root)
            .expect("read registry directory")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("tools.json.corrupt-")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read_to_string(backups[0].path()).expect("read corruption backup"),
            corrupted
        );

        fs::remove_dir_all(root).expect("cleanup recovered registry root");
    }

    #[test]
    fn registry_does_not_recover_comma_only_trailing_json() {
        let root = temp_root("trailing-commas");
        fs::create_dir_all(&root).expect("create registry root");
        let tool = ToolDefinition::new(
            "preserved-tool",
            "Preserved Tool",
            "Tool in an unrecoverable registry",
            ToolExecution::CliWrapper {
                command: "echo".to_owned(),
                args: vec!["ok".to_owned()],
            },
        );
        let valid = serde_json::to_string_pretty(&vec![tool]).expect("serialize tool");
        let corrupted = format!("{valid}\n,,,");
        let registry_path = root.join("tools.json");
        fs::write(&registry_path, &corrupted).expect("write comma-corrupted registry");

        let registry = ToolRegistry::new(&root);
        assert!(matches!(
            registry.list_tools(),
            Err(ToolRegistryError::Json(_))
        ));
        assert_eq!(
            fs::read_to_string(&registry_path).expect("read unchanged registry"),
            corrupted
        );
        assert_eq!(
            fs::read_dir(&root)
                .expect("read registry directory")
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("tools.json.corrupt-"))
                .count(),
            0
        );

        fs::remove_dir_all(root).expect("cleanup comma-corrupted registry root");
    }

    #[test]
    fn execute_mcp_tool_calls_configured_server() {
        let tool = ToolDefinition::new(
            "fixture-echo",
            "Fixture Echo",
            "Echo through fixture MCP",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "echo".to_owned(),
            },
        );
        let server = current_test_binary_fixture_config();

        let result = execute_tool(
            &tool,
            &[server],
            serde_json::json!({ "text": "hello registry" }),
        )
        .expect("execute MCP-backed tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "hello registry");
    }

    #[test]
    fn execute_mcp_image_search_tool_downloads_structured_image_result() {
        let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let mut tool = ToolDefinition::new(
            "fixture-image-search",
            "Fixture Image Search",
            "Download the first MCP image-search result",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "brave_image_search".to_owned(),
            },
        );
        tool.outputs = vec![serde_json::json!({
            "name": "output",
            "label": "output",
            "type": "image",
            "execution_type": "image_buffer"
        })];
        let server = current_test_binary_fixture_config().env(
            "LOOM_MCP_FIXTURE_IMAGE_URL",
            image_fixture.url("/fixture.png"),
        );

        let result = execute_tool(
            &tool,
            &[server],
            serde_json::json!({ "query": "fixture cat", "count": 1 }),
        )
        .expect("execute MCP image-search tool");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn execute_mcp_image_search_tool_honors_result_index_and_preserves_candidates() {
        let first_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let second_fixture = HttpImageFixture::start("image/png", fixture_alt_image_bytes());
        let mut tool = ToolDefinition::new(
            "fixture-image-search-multi",
            "Fixture Image Search Multi",
            "Download the selected MCP image-search result",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "brave_image_search".to_owned(),
            },
        );
        tool.outputs = vec![serde_json::json!({
            "name": "output",
            "label": "output",
            "type": "image",
            "execution_type": "image_buffer"
        })];
        let server = current_test_binary_fixture_config()
            .env(
                "LOOM_MCP_FIXTURE_IMAGE_URL",
                first_fixture.url("/fixture-a.png"),
            )
            .env(
                "LOOM_MCP_FIXTURE_IMAGE_URL_ALT",
                second_fixture.url("/fixture-b.png"),
            );

        let result = execute_tool(
            &tool,
            &[server],
            serde_json::json!({ "query": "fixture cat", "count": 2, "result_index": 1 }),
        )
        .expect("execute MCP image-search tool with explicit result index");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE_ALT);
        assert_eq!(result["loomMetadata"]["imageSearch"]["selectedIndex"], 1);
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][0]["index"],
            0
        );
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][1]["index"],
            1
        );
    }

    #[test]
    fn normalize_mcp_image_search_falls_back_to_another_candidate_when_selected_one_cannot_download(
    ) {
        let second_fixture = HttpImageFixture::start("image/png", fixture_alt_image_bytes());
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Broken primary image",
                        "url": "https://example.invalid/broken",
                        "properties": {
                            "url": "http://127.0.0.1:9/broken.jpg",
                            "width": 1,
                            "height": 1
                        }
                    },
                    {
                        "title": "Working fallback image",
                        "url": "https://example.invalid/fallback",
                        "properties": {
                            "url": second_fixture.url("/fixture-b.png"),
                            "width": 1,
                            "height": 1
                        }
                    }
                ],
                "count": 2
            }
        });

        let result = normalize_mcp_image_result(&serde_json::json!({ "result_index": 0 }), &value)
            .expect("fallback to another candidate image");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE_ALT);
        assert_eq!(result["loomMetadata"]["imageSearch"]["selectedIndex"], 1);
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][0]["index"],
            0
        );
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][1]["index"],
            1
        );
    }

    #[test]
    fn normalize_mcp_image_search_retains_candidate_metadata_when_all_downloads_fail() {
        let mut tool = ToolDefinition::new(
            "fixture-image-search-download-failure",
            "Fixture Image Search Download Failure",
            "Return a friendly text message but keep the image-search candidates",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "brave_image_search".to_owned(),
            },
        );
        tool.outputs = vec![serde_json::json!({
            "name": "output",
            "label": "output",
            "type": "image",
            "execution_type": "image_buffer"
        })];
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Broken primary image",
                        "url": "https://example.invalid/broken",
                        "properties": {
                            "url": "http://127.0.0.1:9/broken.jpg",
                            "width": 1,
                            "height": 1
                        }
                    }
                ],
                "count": 1
            }
        });

        let result = normalize_mcp_result(&tool, &serde_json::json!({ "result_index": 0 }), value);

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(
            result["content"][0]["text"],
            "图片搜索已返回候选结果，但图片下载失败，请稍后重试。"
        );
        assert_eq!(result["loomMetadata"]["imageSearch"]["selectedIndex"], 0);
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][0]["imageUrl"],
            "http://127.0.0.1:9/broken.jpg"
        );
    }

    #[test]
    fn execute_mcp_image_search_tool_normalizes_legacy_hook_argument_types_and_aliases() {
        let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let mut tool = ToolDefinition::new(
            "fixture-image-search-legacy-hook",
            "Fixture Image Search Legacy Hook",
            "Normalize legacy Hook MCP image-search arguments before the MCP call",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "brave_image_search".to_owned(),
            },
        );
        tool.outputs = vec![serde_json::json!({
            "name": "output",
            "label": "output",
            "type": "image",
            "execution_type": "image_buffer"
        })];
        let server = current_test_binary_fixture_config().env(
            "LOOM_MCP_FIXTURE_IMAGE_URL",
            image_fixture.url("/fixture.png"),
        );

        let result = execute_tool(
            &tool,
            &[server],
            serde_json::json!({
                "query": "fixture cat",
                "count": "1",
                "search_lang": "ZH",
                "spellcheck": "true"
            }),
        )
        .expect("execute MCP image-search tool with legacy Hook arguments");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn execute_mcp_image_search_tool_normalizes_legacy_search_lang_without_enum_schema() {
        let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let mut tool = ToolDefinition::new(
            "fixture-image-search-realshape-legacy-hook",
            "Fixture Image Search Realshape Legacy Hook",
            "Normalize legacy Hook MCP image-search arguments even when search_lang schema has no enum",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "brave_image_search_realshape".to_owned(),
            },
        );
        tool.outputs = vec![serde_json::json!({
            "name": "output",
            "label": "output",
            "type": "image",
            "execution_type": "image_buffer"
        })];
        let server = current_test_binary_fixture_config().env(
            "LOOM_MCP_FIXTURE_IMAGE_URL",
            image_fixture.url("/fixture.png"),
        );

        let result = execute_tool(
            &tool,
            &[server],
            serde_json::json!({
                "query": "fixture cat",
                "count": "1",
                "search_lang": "ZH",
                "spellcheck": "true"
            }),
        )
        .expect("execute MCP image-search tool with Brave-like string-only search_lang schema");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn normalize_mcp_image_search_falls_back_to_nested_thumbnail_when_primary_image_download_fails()
    {
        let thumbnail_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let thumbnail_url = thumbnail_fixture.url("/thumb.png");
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Fixture image",
                        "url": "https://example.invalid/page",
                        "thumbnail": {
                            "src": thumbnail_url,
                            "width": 1,
                            "height": 1
                        },
                        "properties": {
                            "url": "http://127.0.0.1:9/primary.jpg",
                            "width": 1,
                            "height": 1
                        }
                    }
                ]
            }
        });

        let result = normalize_mcp_image_result(&serde_json::json!({}), &value)
            .expect("fallback to thumbnail image");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"]
                .as_array()
                .expect("candidate metadata")
                .len(),
            1
        );
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][0]["thumbnailUrl"],
            thumbnail_url
        );
    }

    #[test]
    fn normalize_mcp_image_search_accepts_octet_stream_thumbnail_without_extension() {
        let thumbnail_fixture =
            HttpImageFixture::start("application/octet-stream", fixture_image_bytes());
        let thumbnail_url = thumbnail_fixture.url("/thumb");
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Fixture image",
                        "url": "https://example.invalid/page",
                        "thumbnail": {
                            "src": thumbnail_url,
                            "width": 1,
                            "height": 1
                        },
                        "properties": {
                            "url": "http://127.0.0.1:9/primary-nope",
                            "width": 1,
                            "height": 1
                        }
                    }
                ]
            }
        });

        let result = normalize_mcp_image_result(&serde_json::json!({}), &value)
            .expect("fallback to octet-stream thumbnail image");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn normalize_mcp_image_search_parses_stringified_items_payloads() {
        let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let image_url = image_fixture.url("/image.png");
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": format!(
                    r#"[{{"title":"Fixture image","url":"https://example.invalid/page","properties":{{"url":"{image_url}","width":1,"height":1}}}}]"#
                ),
                "count": 1
            }
        });

        let result = normalize_mcp_image_result(&serde_json::json!({}), &value)
            .expect("normalize stringified image-search items");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][0]["imageUrl"],
            image_url
        );
    }

    #[test]
    fn normalize_mcp_image_search_downloads_from_hosts_requiring_image_accept_header() {
        let image_fixture = HeaderAwareHttpImageFixture::start(
            "image/png",
            fixture_image_bytes(),
            "accept: image/",
        );
        let image_url = image_fixture.url("/guarded.png");
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Guarded fixture image",
                        "url": "https://example.invalid/page",
                        "properties": {
                            "url": image_url,
                            "width": 1,
                            "height": 1
                        }
                    }
                ]
            }
        });

        let result = normalize_mcp_image_result(&serde_json::json!({}), &value)
            .expect("normalize guarded image-search candidate");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn normalize_mcp_image_search_strips_broken_cdn_modifiers_from_candidate_urls() {
        let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let image_url = image_fixture.url("/image.png");
        let decorated_image_url = format!("{image_url}!/clip/0x300a0a0");
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Modifier fixture image",
                        "url": "https://example.invalid/page",
                        "properties": {
                            "url": decorated_image_url,
                            "width": 1,
                            "height": 1
                        }
                    }
                ],
                "count": 1
            }
        });

        let result = normalize_mcp_image_result(&serde_json::json!({}), &value)
            .expect("normalize image-search url with broken modifiers");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][0]["imageUrl"],
            image_url
        );
    }

    #[test]
    fn normalize_mcp_image_search_strips_trailing_path_modifiers_after_image_extension() {
        let image_fixture = ExactPathHttpImageFixture::start(
            "image/png",
            fixture_image_bytes(),
            "/image.png_300.png",
        );
        let image_url = image_fixture.url("/image.png_300.png");
        let decorated_image_url = format!("{image_url}/dpi/0x300a0!");
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Modifier fixture image",
                        "url": "https://example.invalid/page",
                        "properties": {
                            "url": decorated_image_url,
                            "width": 1,
                            "height": 1
                        }
                    }
                ],
                "count": 1
            }
        });

        let result = normalize_mcp_image_result(&serde_json::json!({}), &value)
            .expect("normalize image-search url with trailing path modifiers");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
        assert_eq!(
            result["loomMetadata"]["imageSearch"]["candidates"][0]["imageUrl"],
            image_url
        );
    }

    #[test]
    fn normalize_mcp_image_search_returns_friendly_message_for_provider_blocked_queries() {
        let mut tool = ToolDefinition::new(
            "fixture-image-search-provider-blocked",
            "Fixture Image Search Provider Blocked",
            "Return a friendly message when the provider flags the query as sensitive",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "brave_image_search".to_owned(),
            },
        );
        tool.outputs = vec![serde_json::json!({
            "name": "output",
            "label": "output",
            "type": "image",
            "execution_type": "image_buffer"
        })];

        let result = normalize_mcp_result(
            &tool,
            &serde_json::json!({ "query": "japanese beauty girl" }),
            serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": "{\"type\":\"object\",\"items\":[],\"count\":0,\"might_be_offensive\":true}"
                    }
                ],
                "structuredContent": {
                    "type": "object",
                    "items": [],
                    "count": 0,
                    "might_be_offensive": true
                }
            }),
        );

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(
            result["content"][0]["text"],
            "图片搜索未返回可用结果：搜索服务将该查询判定为可能敏感，请尝试更换关键词。"
        );
    }

    #[test]
    fn normalize_mcp_image_search_returns_friendly_message_for_empty_results() {
        let mut tool = ToolDefinition::new(
            "fixture-image-search-empty-results",
            "Fixture Image Search Empty Results",
            "Return a friendly message when the provider yields no images",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "brave_image_search".to_owned(),
            },
        );
        tool.outputs = vec![serde_json::json!({
            "name": "output",
            "label": "output",
            "type": "image",
            "execution_type": "image_buffer"
        })];

        let result = normalize_mcp_result(
            &tool,
            &serde_json::json!({ "query": "no results please" }),
            serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": "{\"type\":\"object\",\"items\":[],\"count\":0}"
                    }
                ],
                "structuredContent": {
                    "type": "object",
                    "items": [],
                    "count": 0
                }
            }),
        );

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(
            result["content"][0]["text"],
            "图片搜索未返回可用结果，请尝试更换关键词。"
        );
    }

    #[cfg(windows)]
    #[test]
    fn powershell_httpclient_fallback_sends_browserish_accept_header() {
        let fixture = HeaderAwareHttpImageFixture::start(
            "image/png",
            fixture_image_bytes(),
            "accept: image/",
        );

        let (mime_type, bytes) =
            download_image_bytes_with_powershell_httpclient(&fixture.url("/thumb"), None)
                .expect("download image bytes via powershell fallback with image accept header");

        assert_eq!(mime_type, "image/png");
        assert_eq!(bytes, fixture_image_bytes());
    }

    #[cfg(windows)]
    #[test]
    fn powershell_httpclient_fallback_downloads_image_candidate_bytes() {
        let fixture = HttpImageFixture::start("application/octet-stream", fixture_image_bytes());
        let (mime_type, bytes) =
            download_image_bytes_with_powershell_httpclient(&fixture.url("/thumb"), None)
                .expect("download image bytes via powershell fallback");

        assert_eq!(mime_type, "image/png");
        assert_eq!(bytes, fixture_image_bytes());
    }

    #[test]
    fn execute_cloud_api_tool_posts_json_arguments_to_fixture() {
        let fixture = CloudFixture::start(CloudFixtureMode::Text);
        let tool = ToolDefinition::new(
            "fixture-cloud",
            "Fixture Cloud",
            "Call fixture cloud API",
            ToolExecution::CloudApi {
                endpoint: fixture.url("/text"),
                method: "POST".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );

        let result = execute_tool(&tool, &[], serde_json::json!({ "prompt": "hello cloud" }))
            .expect("execute cloud API tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "cloud saw hello cloud");
    }

    #[test]
    fn execute_cloud_api_tool_normalizes_image_json_response() {
        let fixture = CloudFixture::start(CloudFixtureMode::Image);
        let tool = ToolDefinition::new(
            "fixture-cloud-image",
            "Fixture Cloud Image",
            "Call fixture cloud image API",
            ToolExecution::CloudApi {
                endpoint: fixture.url("/image"),
                method: "POST".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );

        let result = execute_tool(
            &tool,
            &[],
            serde_json::json!({ "input_base64": CLOUD_FIXTURE_IMAGE }),
        )
        .expect("execute cloud image API tool");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn execute_cloud_api_tool_reports_http_errors() {
        let fixture = CloudFixture::start(CloudFixtureMode::Error);
        let tool = ToolDefinition::new(
            "fixture-cloud-error",
            "Fixture Cloud Error",
            "Call fixture cloud API that fails",
            ToolExecution::CloudApi {
                endpoint: fixture.url("/error"),
                method: "POST".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );

        let error = execute_tool(&tool, &[], serde_json::json!({}))
            .expect_err("cloud API HTTP error fails");

        assert!(error.to_string().contains("cloud API"));
    }

    #[test]
    fn execute_cloud_api_tool_supports_artloom_multipart_template_contract() {
        let root = temp_root("cloud-multipart-template");
        let upload_path = root.join("upload.png");
        fs::write(&upload_path, b"loom-upload").expect("write upload fixture");

        let fixture = CloudFixture::start(CloudFixtureMode::MultipartText);
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "fixture-cloud-multipart",
            "name": "Fixture Cloud Multipart",
            "description": "Call old ArtLoom-style multipart cloud API",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "url": fixture.url("/upload/{{inputs.route.value}}?mode={{mode}}"),
                "method": "POST",
                "contentType": "multipart/form-data",
                "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\",\"X-Mode\":\"{{mode}}\"}",
                "body": "{\"file\":\"{{inputs.image.path}}\",\"prompt\":\"{{inputs.prompt.value}}\",\"literal\":\"fixed\",\"skipEmpty\":\"{{inputs.empty.value}}\",\"skipDisabled\":\"{{inputs.disabled.value}}\"}"
            }
        }))
        .expect("old ArtLoom-style cloud API execution deserializes");

        let result = execute_tool(
            &tool,
            &[],
            serde_json::json!({
                "route": "image",
                "mode": "fast",
                "trace": "trace-42",
                "image": upload_path.display().to_string(),
                "prompt": "hello multipart",
                "empty": "",
                "disabled": "__DISABLED__"
            }),
        )
        .expect("execute ArtLoom-style multipart cloud API tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "cloud saw multipart");

        let request = fixture.request();
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /upload/image?mode=fast HTTP/1.1"));
        assert!(request_lower.contains("x-trace: trace-42"));
        assert!(request_lower.contains("x-mode: fast"));
        assert!(request_lower.contains("content-type: multipart/form-data; boundary="));
        assert!(request.contains("name=\"file\""));
        assert!(request.contains("filename=\"upload.png\""));
        assert!(request.contains("loom-upload"));
        assert!(request.contains("name=\"prompt\""));
        assert!(request.contains("\r\nhello multipart\r\n"));
        assert!(request.contains("name=\"literal\""));
        assert!(request.contains("\r\nfixed\r\n"));
        assert!(!request.contains("skipEmpty"));
        assert!(!request.contains("skipDisabled"));
        assert!(!request.contains("{{"));

        fs::remove_dir_all(root).expect("cleanup multipart template root");
    }

    #[test]
    fn execute_script_tool_passes_arguments_to_fixture() {
        let root = temp_root("script-text");
        let script_path = write_script_fixture(&root);
        let tool = ToolDefinition::new(
            "fixture-script",
            "Fixture Script",
            "Echo through script",
            ToolExecution::Script {
                path: script_path.display().to_string(),
            },
        );

        let result = execute_tool(&tool, &[], serde_json::json!({ "text": "hello registry" }))
            .expect("execute script-backed tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "script saw hello registry");

        fs::remove_dir_all(root).expect("cleanup script text root");
    }

    #[test]
    fn execute_script_tool_accepts_image_content_fixture() {
        let root = temp_root("script-image");
        let script_path = write_script_fixture(&root);
        let tool = ToolDefinition::new(
            "fixture-script-image",
            "Fixture Script Image",
            "Return image through script",
            ToolExecution::Script {
                path: script_path.display().to_string(),
            },
        );
        let image_data = "data:image/png;base64,abc123";

        let result = execute_tool(&tool, &[], serde_json::json!({ "image": image_data }))
            .expect("execute image script-backed tool");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], image_data);

        fs::remove_dir_all(root).expect("cleanup script image root");
    }

    #[test]
    fn execute_script_tool_blends_input_and_reference_images_with_mix_ratio() {
        let tool = ToolDefinition::new(
            "fixture-script-blend",
            "Fixture Script Blend",
            "Blend two images through script",
            ToolExecution::Script {
                path: workspace_image_blend_script().display().to_string(),
            },
        );
        let source = one_pixel_png_data_url([200, 50, 0, 255]);
        let reference = one_pixel_png_data_url([0, 150, 200, 255]);

        let result = execute_tool(
            &tool,
            &[],
            serde_json::json!({
                "input": source,
                "reference": reference,
                "mix_ratio": 25
            }),
        )
        .expect("execute script blend tool");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");

        let output = loom_image_io::decode_image_base64_to_rgba8(
            result["content"][0]["data"]
                .as_str()
                .expect("script image blend output data"),
        )
        .expect("decode blend output");
        assert_eq!(output.width, 1);
        assert_eq!(output.height, 1);
        assert_eq!(output.data, vec![150, 75, 50, 255]);
    }

    #[cfg(windows)]
    #[test]
    fn execute_script_image_blend_art_accepts_large_payloads_with_valid_images() {
        let tool = ToolDefinition::new(
            "fixture-script-blend-large-images",
            "Fixture Script Blend Large Images",
            "Blend large valid images through script",
            ToolExecution::Script {
                path: workspace_image_blend_script().display().to_string(),
            },
        );
        let source = one_pixel_png_data_url([200, 50, 0, 255]);
        let reference = one_pixel_png_data_url([0, 150, 200, 255]);
        let debug_padding = "x".repeat(40_000);

        let result = execute_tool(
            &tool,
            &[],
            serde_json::json!({
                "input": source,
                "reference": reference,
                "mix_ratio": 50,
                "debug_padding": debug_padding
            }),
        )
        .expect("execute script blend tool with large payload and valid images");

        let output = loom_image_io::decode_image_base64_to_rgba8(
            result["content"][0]["data"]
                .as_str()
                .expect("large payload blend output"),
        )
        .expect("decode large payload blend output");
        assert_eq!(output.width, 1);
        assert_eq!(output.height, 1);
    }

    #[cfg(windows)]
    #[test]
    fn execute_script_image_blend_art_handles_4k_images_within_timeout() {
        let tool = ToolDefinition::new(
            "fixture-script-blend-4k-images",
            "Fixture Script Blend 4K Images",
            "Blend 4K images through script without timing out",
            ToolExecution::Script {
                path: workspace_image_blend_script().display().to_string(),
            },
        );
        let source = solid_color_png_data_url(4096, 4096, [200, 50, 0, 255]);
        let reference = solid_color_png_data_url(4096, 4096, [0, 150, 200, 255]);

        let result = execute_tool(
            &tool,
            &[],
            serde_json::json!({
                "input": source,
                "reference": reference,
                "mix_ratio": 25
            }),
        )
        .expect("execute script blend tool with 4k images");

        let output = loom_image_io::decode_image_base64_to_rgba8(
            result["content"][0]["data"]
                .as_str()
                .expect("4k blend output"),
        )
        .expect("decode 4k blend output");
        assert_eq!(output.width, 4096);
        assert_eq!(output.height, 4096);
        assert_eq!(&output.data[0..4], &[150, 75, 50, 255]);
    }

    #[cfg(windows)]
    #[test]
    fn execute_script_tool_supports_large_payloads_without_hitting_windows_command_limit() {
        let root = temp_root("script-large-payload");
        let script_path = write_script_fixture(&root);
        let tool = ToolDefinition::new(
            "fixture-script-large-payload",
            "Fixture Script Large Payload",
            "Echo large payload through script",
            ToolExecution::Script {
                path: script_path.display().to_string(),
            },
        );
        let large_text = "x".repeat(40_000);

        let result = execute_tool(&tool, &[], serde_json::json!({ "text": large_text }))
            .expect("execute script-backed tool with large payload");

        let text = result["content"][0]["text"]
            .as_str()
            .expect("script large payload response text");
        assert!(text.starts_with("script saw "));
        assert_eq!(text.len(), "script saw ".len() + 40_000);

        fs::remove_dir_all(root).expect("cleanup large payload script root");
    }

    #[test]
    fn execute_script_tool_reports_missing_script() {
        let root = temp_root("script-missing");
        let tool = ToolDefinition::new(
            "missing-script",
            "Missing Script",
            "Missing script fixture",
            ToolExecution::Script {
                path: root.join("missing.ps1").display().to_string(),
            },
        );

        let error =
            execute_tool(&tool, &[], serde_json::json!({})).expect_err("missing script fails");

        assert!(error.to_string().contains("script"));

        fs::remove_dir_all(root).expect("cleanup missing script root");
    }

    #[test]
    fn resolve_python_executable_prefers_loom_python_env() {
        let root = temp_root("python-env");
        let override_python = root.join("custom-python.exe");
        fs::write(&override_python, b"").expect("write override python fixture");
        let packaged_python = root.join("bin").join("python-embed").join("python.exe");
        fs::create_dir_all(packaged_python.parent().expect("packaged python parent"))
            .expect("create packaged python parent");
        fs::write(&packaged_python, b"").expect("write packaged python fixture");

        let resolved = resolve_python_executable_from(
            Some(override_python.to_string_lossy().as_ref()),
            None,
            Some(&root),
            Some(&root),
        );

        assert_eq!(resolved, override_python);

        fs::remove_dir_all(root).expect("cleanup python env root");
    }

    #[test]
    fn resolve_python_executable_prefers_packaged_python() {
        let root = temp_root("python-packaged");
        let packaged_python = root.join("bin").join("python-embed").join("python.exe");
        fs::create_dir_all(packaged_python.parent().expect("packaged python parent"))
            .expect("create packaged python parent");
        fs::write(&packaged_python, b"").expect("write packaged python fixture");

        let resolved = resolve_python_executable_from(None, None, Some(&root), Some(&root));

        assert_eq!(resolved, packaged_python);

        fs::remove_dir_all(root).expect("cleanup packaged python root");
    }

    #[test]
    fn resolve_python_executable_prefers_framework_runtime_dir() {
        // A python_art framework package installed via the framework registry
        // (<control-plane>/frameworks/) must win over a packaged
        // python-embed next to the exe/cwd — this is what wires "installing the
        // framework" to "executing an art with it" (方向 A).
        let root = temp_root("python-runtime-dir");
        let runtime_python = root
            .join("python_art")
            .join("python-embed")
            .join("python.exe");
        fs::create_dir_all(runtime_python.parent().expect("runtime python parent"))
            .expect("create runtime python parent");
        fs::write(&runtime_python, b"").expect("write runtime python fixture");

        // A competing packaged python under a separate cwd candidate.
        let cwd = temp_root("python-runtime-cwd");
        let packaged_python = cwd.join("bin").join("python-embed").join("python.exe");
        fs::create_dir_all(packaged_python.parent().expect("packaged python parent"))
            .expect("create packaged python parent");
        fs::write(&packaged_python, b"").expect("write packaged python fixture");

        let resolved = resolve_python_executable_from(
            None,
            Some(root.to_string_lossy().as_ref()),
            None,
            Some(&cwd),
        );

        assert_eq!(resolved, runtime_python);

        fs::remove_dir_all(root).expect("cleanup runtime dir root");
        fs::remove_dir_all(cwd).expect("cleanup runtime cwd root");
    }

    #[test]
    fn resolve_python_art_path_prefers_installed_art_dir_for_relative_art_path() {
        let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
        let root = temp_root("python-installed-art-root");
        let plugin_dir = root
            .join("arts")
            .join("store-python-tool")
            .join("python")
            .join("Arts")
            .join("StorePythonEcho");
        fs::create_dir_all(&plugin_dir).expect("create installed python art dir");
        fs::write(
            plugin_dir.join("art.json"),
            r#"{"art_id":"store_python_echo","label":"Store Python Echo"}"#,
        )
        .expect("write installed python art json");
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);

        let resolved = resolve_python_art_path(
            "store-python-tool",
            "store_python_echo",
            Some("python/Arts/StorePythonEcho"),
        );

        assert_eq!(resolved, Some(plugin_dir));

        match previous_root {
            Some(value) => std::env::set_var("LOOM_CONTROL_PLANE_ROOT", value),
            None => std::env::remove_var("LOOM_CONTROL_PLANE_ROOT"),
        }
        fs::remove_dir_all(root).expect("cleanup installed python art root");
    }

    #[test]
    fn mcp_registry_fixture_server() {
        if std::env::var("LOOM_TOOL_REGISTRY_MCP_FIXTURE_SERVER")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }

        run_mcp_fixture_server();
        std::process::exit(0);
    }

    fn current_test_binary_fixture_config() -> loom_mcp::McpServerConfig {
        let exe = std::env::current_exe().expect("current test executable");
        loom_mcp::McpServerConfig::new("fixture", "Fixture MCP", exe.display().to_string())
            .arg("tests::mcp_registry_fixture_server")
            .arg("--exact")
            .arg("--nocapture")
            .env("LOOM_TOOL_REGISTRY_MCP_FIXTURE_SERVER", "1")
    }

    fn run_mcp_fixture_server() {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let fixture_image_url = std::env::var("LOOM_MCP_FIXTURE_IMAGE_URL").ok();
        let fixture_image_url_alt = std::env::var("LOOM_MCP_FIXTURE_IMAGE_URL_ALT").ok();

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
                                "name": "tool-registry-fixture",
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
                                },
                                {
                                    "name": "brave_image_search",
                                    "description": "Return structured image-search results",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "query": { "type": "string" },
                                            "count": { "type": "integer" },
                                            "search_lang": {
                                                "type": "string",
                                                "enum": ["zh-hans", "en"]
                                            },
                                            "spellcheck": { "type": "boolean" }
                                        },
                                        "required": ["query"]
                                    }
                                },
                                {
                                    "name": "brave_image_search_realshape",
                                    "description": "Return structured image-search results with Brave-like string-only search_lang schema",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "query": { "type": "string" },
                                            "count": { "type": "integer" },
                                            "search_lang": { "type": "string" },
                                            "spellcheck": { "type": "boolean" }
                                        },
                                        "required": ["query"]
                                    }
                                }
                            ]
                        }
                    }),
                ),
                "tools/call" => {
                    let tool_name = request["params"]["name"].as_str().unwrap_or_default();
                    match tool_name {
                        "echo" => {
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
                        "brave_image_search" | "brave_image_search_realshape" => {
                            let arguments = &request["params"]["arguments"];
                            if arguments.get("count").is_some()
                                && !arguments["count"].is_i64()
                                && !arguments["count"].is_u64()
                            {
                                write_fixture_response(
                                    &mut stdout,
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": request["id"].clone(),
                                        "error": {
                                            "code": -32602,
                                            "message": "count must be an integer"
                                        }
                                    }),
                                );
                                continue;
                            }
                            if arguments.get("spellcheck").is_some()
                                && !arguments["spellcheck"].is_boolean()
                            {
                                write_fixture_response(
                                    &mut stdout,
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": request["id"].clone(),
                                        "error": {
                                            "code": -32602,
                                            "message": "spellcheck must be a boolean"
                                        }
                                    }),
                                );
                                continue;
                            }
                            if let Some(search_lang) = arguments
                                .get("search_lang")
                                .and_then(serde_json::Value::as_str)
                            {
                                if !matches!(search_lang, "zh-hans" | "en") {
                                    write_fixture_response(
                                        &mut stdout,
                                        serde_json::json!({
                                            "jsonrpc": "2.0",
                                            "id": request["id"].clone(),
                                            "error": {
                                                "code": -32602,
                                                "message": "search_lang must be one of [\"zh-hans\", \"en\"]"
                                            }
                                        }),
                                    );
                                    continue;
                                }
                            } else if arguments.get("search_lang").is_some() {
                                write_fixture_response(
                                    &mut stdout,
                                    serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": request["id"].clone(),
                                        "error": {
                                            "code": -32602,
                                            "message": "search_lang must be a string"
                                        }
                                    }),
                                );
                                continue;
                            }
                            let query = request["params"]["arguments"]["query"]
                                .as_str()
                                .unwrap_or_default();
                            let image_url = fixture_image_url.clone().unwrap_or_else(|| {
                                "https://example.invalid/fixture.png".to_owned()
                            });
                            let alternate_image_url = fixture_image_url_alt
                                .clone()
                                .unwrap_or_else(|| image_url.clone());
                            write_fixture_response(
                                &mut stdout,
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": request["id"].clone(),
                                    "result": {
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": format!("fixture brave_image_search results for {query}")
                                            }
                                        ],
                                        "structuredContent": {
                                            "type": "object",
                                            "items": [
                                                {
                                                    "title": "Fixture image",
                                                    "url": "https://example.invalid/page",
                                                    "properties": {
                                                        "url": image_url,
                                                        "width": 1,
                                                        "height": 1
                                                    }
                                                },
                                                {
                                                    "title": "Fixture image alternate",
                                                    "url": "https://example.invalid/page-2",
                                                    "properties": {
                                                        "url": alternate_image_url,
                                                        "width": 1,
                                                        "height": 1
                                                    }
                                                }
                                            ]
                                        }
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
                                    "message": format!("unknown tool {tool_name}")
                                }
                            }),
                        ),
                    }
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

    const CLOUD_FIXTURE_IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";
    const CLOUD_FIXTURE_IMAGE_ALT: &str =
        "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAEElEQVR4AQEFAPr/AAoUHv8BpAE8tOS4KAAAAABJRU5ErkJggg==";

    fn fixture_image_bytes() -> Vec<u8> {
        loom_image_io::decode_data_url_bytes(CLOUD_FIXTURE_IMAGE)
            .expect("decode fixture image data url")
    }

    fn fixture_alt_image_bytes() -> Vec<u8> {
        loom_image_io::decode_data_url_bytes(CLOUD_FIXTURE_IMAGE_ALT)
            .expect("decode alternate fixture image data url")
    }

    enum CloudFixtureMode {
        Text,
        Image,
        Error,
        MultipartText,
    }

    struct CloudFixture {
        port: u16,
        worker: Option<JoinHandle<()>>,
        captured_request: Arc<Mutex<Option<String>>>,
    }

    impl CloudFixture {
        fn start(mode: CloudFixtureMode) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind cloud fixture");
            let port = listener.local_addr().expect("cloud fixture address").port();
            let captured_request = Arc::new(Mutex::new(None));
            let worker_captured_request = Arc::clone(&captured_request);
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept cloud fixture request");
                let request = read_http_request(&mut stream);
                *worker_captured_request
                    .lock()
                    .expect("lock cloud request capture") = Some(request.clone());
                let Some((_, body)) = request.split_once("\r\n\r\n") else {
                    return;
                };
                let prompt = serde_json::from_str::<serde_json::Value>(body)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("prompt")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_default();
                let response = match mode {
                    CloudFixtureMode::Text => serde_json::json!({
                        "content": [
                            {
                                "type": "text",
                                "text": format!("cloud saw {prompt}")
                            }
                        ]
                    }),
                    CloudFixtureMode::Image => serde_json::json!({
                        "content": [
                            {
                                "type": "image",
                                "data": CLOUD_FIXTURE_IMAGE,
                                "mimeType": "image/png"
                            }
                        ]
                    }),
                    CloudFixtureMode::MultipartText => serde_json::json!({
                        "content": [
                            {
                                "type": "text",
                                "text": "cloud saw multipart"
                            }
                        ]
                    }),
                    CloudFixtureMode::Error => {
                        write_http_response(
                            &mut stream,
                            "500 Internal Server Error",
                            "text/plain",
                            "fixture cloud error",
                        );
                        return;
                    }
                };
                write_http_response(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    &response.to_string(),
                );
            });
            Self {
                port,
                worker: Some(worker),
                captured_request,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }

        fn request(&self) -> String {
            self.captured_request
                .lock()
                .expect("lock cloud request capture")
                .clone()
                .expect("captured cloud request")
        }
    }

    impl Drop for CloudFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    struct HttpImageFixture {
        port: u16,
        worker: Option<JoinHandle<()>>,
    }

    impl HttpImageFixture {
        fn start(content_type: &'static str, body: Vec<u8>) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP image fixture");
            let port = listener
                .local_addr()
                .expect("HTTP image fixture address")
                .port();
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener
                    .accept()
                    .expect("accept HTTP image fixture request");
                let _ = read_http_request(&mut stream);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            });
            Self {
                port,
                worker: Some(worker),
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }
    }

    impl Drop for HttpImageFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    struct HeaderAwareHttpImageFixture {
        port: u16,
        worker: Option<JoinHandle<()>>,
    }

    impl HeaderAwareHttpImageFixture {
        fn start(content_type: &'static str, body: Vec<u8>, required_header: &'static str) -> Self {
            let listener =
                TcpListener::bind(("127.0.0.1", 0)).expect("bind guarded HTTP image fixture");
            let port = listener
                .local_addr()
                .expect("guarded HTTP image fixture address")
                .port();
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener
                    .accept()
                    .expect("accept guarded HTTP image fixture request");
                let request = read_http_request(&mut stream);
                if request.to_ascii_lowercase().contains(required_header) {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                } else {
                    write_http_response(
                        &mut stream,
                        "403 Forbidden",
                        "text/plain",
                        "missing required header",
                    );
                }
            });
            Self {
                port,
                worker: Some(worker),
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }
    }

    impl Drop for HeaderAwareHttpImageFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    struct ExactPathHttpImageFixture {
        port: u16,
        worker: Option<JoinHandle<()>>,
    }

    impl ExactPathHttpImageFixture {
        fn start(content_type: &'static str, body: Vec<u8>, expected_path: &'static str) -> Self {
            let listener =
                TcpListener::bind(("127.0.0.1", 0)).expect("bind exact-path HTTP image fixture");
            let port = listener
                .local_addr()
                .expect("exact-path HTTP image fixture address")
                .port();
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener
                    .accept()
                    .expect("accept exact-path HTTP image fixture request");
                let request = read_http_request(&mut stream);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or_default();
                if path == expected_path {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                } else {
                    write_http_response(&mut stream, "404 Not Found", "text/plain", "not found");
                }
            });
            Self {
                port,
                worker: Some(worker),
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }
    }

    impl Drop for ExactPathHttpImageFixture {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                let _ = TcpStream::connect(("127.0.0.1", self.port));
                let _ = worker.join();
            }
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        let header_end = loop {
            let read = stream
                .read(&mut buffer)
                .expect("read fixture request headers");
            if read == 0 {
                return String::from_utf8_lossy(&request).to_string();
            }
            request.extend_from_slice(&buffer[..read]);
            if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };

        let content_length = String::from_utf8_lossy(&request[..header_end])
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or_default();
        let expected_length = header_end + content_length;

        while request.len() < expected_length {
            let read = stream.read(&mut buffer).expect("read fixture request body");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }

        String::from_utf8_lossy(&request).to_string()
    }

    fn write_http_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.flush();
    }

    fn write_script_fixture(root: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let script_path = root.join("fixture-script.ps1");
            let source = r#"
$ErrorActionPreference = "Stop"
$payload = $args[0] | ConvertFrom-Json
if ($payload.arguments.image) {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "image"
                data = [string]$payload.arguments.image
                mimeType = "image/png"
            }
        )
    }
} else {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "text"
                text = "script saw $($payload.arguments.text)"
            }
        )
    }
}
[Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
            fs::write(&script_path, source).expect("write PowerShell script fixture");
            script_path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;

            let script_path = root.join("fixture-script.sh");
            let source = r#"#!/usr/bin/env sh
python3 - "$1" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
arguments = payload.get("arguments", {})
if arguments.get("image"):
    response = {
        "content": [
            {
                "type": "image",
                "data": arguments["image"],
                "mimeType": "image/png",
            }
        ]
    }
else:
    response = {
        "content": [
            {
                "type": "text",
                "text": "script saw " + str(arguments.get("text", "")),
            }
        ]
    }
print(json.dumps(response))
PY
"#;
            fs::write(&script_path, source).expect("write shell script fixture");
            let mut permissions = fs::metadata(&script_path)
                .expect("script fixture metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&script_path, permissions).expect("make shell fixture executable");
            script_path
        }
    }

    fn workspace_image_blend_script() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|candidate| {
                let script = candidate
                    .join("resources")
                    .join("script-arts")
                    .join("image-blend")
                    .join("main.ps1");
                script.exists().then_some(script)
            })
            .expect("locate Loom/resources/script-arts/image-blend/main.ps1")
    }

    fn one_pixel_png_data_url(rgba: [u8; 4]) -> String {
        loom_image_io::rgba8_to_png_data_url(1, 1, &rgba).expect("encode one pixel png")
    }

    fn solid_color_png_data_url(width: u32, height: u32, rgba: [u8; 4]) -> String {
        let pixels = usize::try_from(width)
            .expect("width usize")
            .saturating_mul(usize::try_from(height).expect("height usize"));
        let mut data = Vec::with_capacity(pixels.saturating_mul(4));
        for _ in 0..pixels {
            data.extend_from_slice(&rgba);
        }
        loom_image_io::rgba8_to_png_data_url(width, height, &data).expect("encode solid color png")
    }
}
