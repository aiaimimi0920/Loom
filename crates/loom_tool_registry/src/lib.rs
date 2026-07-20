//! User-managed tool and Art registry contracts for Loom.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use reqwest::blocking::{multipart, Client};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const TOOLS_FILE: &str = "tools.json";
const SCRIPT_EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
const CLOUD_API_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum ToolRegistryError {
    #[error("invalid tool definition `{id}`: {reason}")]
    InvalidToolDefinition { id: String, reason: String },
    #[error("tool `{id}` is disabled")]
    ExecutionRejected { id: String },
    #[error("tool `{id}` execution type `{execution_type}` is not supported by this runtime")]
    UnsupportedExecution {
        id: String,
        execution_type: &'static str,
    },
    #[error("MCP server `{server_id}` for tool `{tool_id}` was not found or is disabled")]
    MissingMcpServer { tool_id: String, server_id: String },
    #[error("MCP execution failed: {0}")]
    Mcp(#[from] loom_mcp::McpError),
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
        self.execution.validate(&self.id)
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
        if let Some(existing) = tools.iter_mut().find(|existing| existing.id == tool.id) {
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
        Ok(self.list_tools()?.into_iter().find(|tool| tool.id == id))
    }

    pub fn delete_tool(&self, id: &str) -> ToolRegistryResult<bool> {
        self.ensure_root()?;
        let mut tools = self.read_tools()?;
        let before = tools.len();
        tools.retain(|tool| tool.id != id);
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
        serde_json::from_str(&content).map_err(ToolRegistryError::from)
    }

    fn write_tools(&self, tools: &[ToolDefinition]) -> ToolRegistryResult<()> {
        let content = serde_json::to_string_pretty(tools)?;
        fs::write(self.tools_path(), content)?;
        Ok(())
    }
}

fn sort_tools(tools: &mut [ToolDefinition]) {
    tools.sort_by(|left, right| left.id.cmp(&right.id));
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
            client
                .call_tool(tool_name, arguments)
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
        ToolExecution::PythonArt { art_id, art_path } => {
            execute_python_art_tool(tool, art_id, art_path.as_deref(), arguments)
        }
        _ => Err(ToolRegistryError::UnsupportedExecution {
            id: tool.id.clone(),
            execution_type: execution_type_name(&tool.execution),
        }),
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
    let plugin_path = resolve_python_art_path(art_id, art_path).ok_or_else(|| {
        ToolRegistryError::PythonArtNotFound {
            id: tool.id.clone(),
            art_id: art_id.to_owned(),
        }
    })?;
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
    let mut command = Command::new(resolve_python_executable());
    configure_python_process(&mut command);
    let output = command
        .arg(&launcher_path)
        .arg(request_json)
        .current_dir(base_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|source| ToolRegistryError::PythonArtSpawn {
            id: tool.id.clone(),
            art_id: art_id.to_owned(),
            source,
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

fn resolve_python_art_path(art_id: &str, art_path: Option<&str>) -> Option<PathBuf> {
    if let Some(art_path) = art_path.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(art_path);
        if path.is_dir() {
            return Some(path);
        }
    }

    python_arts_dirs()
        .into_iter()
        .find_map(|arts_dir| find_python_art_in_dir(&arts_dir, art_id))
}

fn python_arts_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
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
    let client = Client::builder()
        .timeout(CLOUD_API_TIMEOUT)
        .build()
        .map_err(|source| ToolRegistryError::CloudRequest {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            source,
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
    let response = request
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
    let bytes = response
        .bytes()
        .map_err(|source| ToolRegistryError::CloudRequest {
            id: tool.id.clone(),
            endpoint: endpoint.clone(),
            source,
        })?;

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
    let mut command = script_command(path, payload);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| ToolRegistryError::ScriptSpawn {
            id: tool.id.clone(),
            path: path.to_owned(),
            source,
        })?;
    let started = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|source| ToolRegistryError::ScriptSpawn {
                        id: tool.id.clone(),
                        path: path.to_owned(),
                        source,
                    });
            }
            Ok(None) if started.elapsed() >= SCRIPT_EXECUTION_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ToolRegistryError::ScriptTimedOut {
                    id: tool.id.clone(),
                    path: path.to_owned(),
                    timeout_ms: SCRIPT_EXECUTION_TIMEOUT.as_millis(),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(source) => {
                return Err(ToolRegistryError::ScriptSpawn {
                    id: tool.id.clone(),
                    path: path.to_owned(),
                    source,
                })
            }
        }
    }
}

fn script_command(path: &str, payload: &str) -> Command {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let mut command = if extension == "ps1" {
        let mut command = Command::new("powershell.exe");
        command
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(path);
        command
    } else if extension == "py" {
        let mut command = Command::new(resolve_python_executable());
        configure_python_process(&mut command);
        command.arg(path);
        command
    } else {
        Command::new(path)
    };
    command.arg(payload);
    command
}

fn configure_python_process(command: &mut Command) {
    command.env("PYTHONDONTWRITEBYTECODE", "1");
}

fn resolve_python_executable() -> PathBuf {
    let loom_python = std::env::var("LOOM_PYTHON").ok();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let current_dir = std::env::current_dir().ok();

    resolve_python_executable_from(
        loom_python.as_deref(),
        exe_dir.as_deref(),
        current_dir.as_deref(),
    )
}

fn resolve_python_executable_from(
    loom_python: Option<&str>,
    exe_dir: Option<&Path>,
    current_dir: Option<&Path>,
) -> PathBuf {
    if let Some(override_python) = loom_python.map(str::trim).filter(|value| !value.is_empty()) {
        return PathBuf::from(override_python);
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
    fn python_script_command_disables_bytecode_writes() {
        let command = script_command("fixture.py", "{}");
        let value = command
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

        let resolved = resolve_python_executable_from(None, Some(&root), Some(&root));

        assert_eq!(resolved, packaged_python);

        fs::remove_dir_all(root).expect("cleanup packaged python root");
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

    const CLOUD_FIXTURE_IMAGE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";

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

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut buffer = [0_u8; 8192];
        let read = stream.read(&mut buffer).expect("read fixture request");
        String::from_utf8_lossy(&buffer[..read]).to_string()
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
}
