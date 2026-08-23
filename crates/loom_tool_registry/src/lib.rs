//! User-managed tool and Art registry contracts for Loom.

pub mod art_settings;
pub mod credentials;
pub mod dependency;
pub mod framework;
pub mod framework_process;
pub mod install;

/// Outbound network policy, shared with `loom_mcp` through the `loom_security` leaf crate.
///
/// The module used to live here, but `loom_tool_registry` depends on `loom_mcp`, so the
/// dependency could not be reversed to let both sides enforce one policy. Existing call sites
/// keep using `loom_tool_registry::network_policy`.
pub use loom_security::network as network_policy;

/// Hardened archive extraction, shared through the same leaf crate as [`network_policy`].
pub(crate) use loom_security::archive as secure_zip;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use loom_process::ProcessSpec;
use loom_protocol::{
    is_safe_publisher_id, is_safe_surface_identifier, PublisherIdentity, SurfaceActionRisk,
    SurfacePackageManifest, SurfaceRuntimeKind, SURFACE_API_VERSION, SURFACE_PROTOCOL_VERSION,
};
use reqwest::{multipart, Method};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const TOOLS_FILE: &str = "tools.json";
/// Deadline for a cloud API call when neither the caller nor the package asks for anything else.
const CLOUD_API_TIMEOUT: Duration = Duration::from_secs(30);
/// Host ceiling for a cloud API call. Generous on purpose: image generation and background removal
/// — the cloud Art use cases this product ships — routinely run past a minute, and the previous
/// behaviour clamped every request down to [`CLOUD_API_TIMEOUT`], so a caller asking for two
/// minutes silently got thirty seconds.
const CLOUD_API_MAX_TIMEOUT: Duration = Duration::from_secs(600);
const MCP_IMAGE_FETCH_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
const MCP_IMAGE_FETCH_ACCEPT: &str =
    "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";
const MCP_IMAGE_FETCH_ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";
const MAX_MCP_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CLOUD_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
/// How much borrowed text an error message from this crate may carry.
///
/// Errors travel further than the code that raises them: into the log, into the Surface error payload,
/// and into the canvas. Text that came from a process or a remote API is not sized by anything this
/// crate controls — a stray `print` in a framework runtime, or an API that answers a failure with a
/// megabyte of HTML, would otherwise turn a diagnosable error into a payload nobody can read or store.
/// Two kilobytes is past the end of every real diagnostic and short of every accident.
const MAX_BORROWED_ERROR_TEXT_BYTES: usize = 2 * 1024;
/// Bound text from a process or a remote response before it becomes part of an error message.
///
/// Only the head is kept, because a diagnostic states its problem first and pads afterwards. The count
/// of what was dropped stays in the message so that a truncated error is not mistaken for a short one.
/// The cut lands on a character boundary; text this size may be UTF-8 and slicing it blindly would
/// panic.
fn bounded_error_text(text: &str) -> String {
    let text = text.trim();
    if text.len() <= MAX_BORROWED_ERROR_TEXT_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_BORROWED_ERROR_TEXT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let omitted = text.len() - end;
    format!("{}… [{omitted} more bytes omitted]", &text[..end])
}
/// The file extensions that make a URL worth treating as an image candidate.
///
/// SVG is deliberately absent. It is an active-content format — script and external references live
/// inside the document — and nothing downstream of here treats it differently from a raster image, so
/// an untrusted search result could hand the canvas a document rather than a picture. The byte
/// sniffing in `infer_image_mime_type_from_bytes` never had an SVG branch either, so an SVG whose URL
/// lacked the extension was already rejected; accepting the same bytes because the URL said `.svg`
/// was the inconsistency, not the rejection.
const IMAGE_URL_EXTENSIONS: [&str; 7] = [".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".avif"];
/// The image MIME types this crate will hand to the canvas.
///
/// Same list as `IMAGE_URL_EXTENSIONS` and same reason for what is missing: a server may declare
/// `image/svg+xml` in a `Content-Type` header, and that declaration is not a reason to accept it.
const SUPPORTED_IMAGE_MIME_TYPES: [&str; 6] = [
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/bmp",
    "image/avif",
];
static REGISTRY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum ToolRegistryError {
    #[error("invalid tool definition `{id}`: {reason}")]
    InvalidToolDefinition { id: String, reason: String },
    #[error("tool `{id}` is disabled")]
    ExecutionRejected { id: String },
    #[error("tool `{id}` execution was cancelled")]
    ExecutionCancelled { id: String },
    #[error("tool `{id}` parameter binding failed: {reason}")]
    ParameterBinding { id: String, reason: String },
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
    #[error("MCP dependency `{server_id}` for tool `{tool_id}` failed [{code}]: {reason}")]
    McpDependency {
        tool_id: String,
        server_id: String,
        code: String,
        reason: String,
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
    #[error("Art settings error: {0}")]
    ArtSettings(#[from] art_settings::ArtSettingsError),
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
        if let Some(surface) = self.surface_manifest()? {
            validate_surface_package_manifest(&self.id, &surface)?;
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

    pub fn surface_manifest(&self) -> ToolRegistryResult<Option<SurfacePackageManifest>> {
        let Some(surface) = self
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("capabilities"))
            .and_then(|capabilities| capabilities.get("surface"))
        else {
            return Ok(None);
        };
        serde_json::from_value(surface.clone())
            .map(Some)
            .map_err(|error| ToolRegistryError::InvalidToolDefinition {
                id: self.id.clone(),
                reason: format!("Surface manifest is invalid: {error}"),
            })
    }
}

fn validate_surface_package_manifest(
    tool_id: &str,
    surface: &SurfacePackageManifest,
) -> ToolRegistryResult<()> {
    let invalid = |reason: String| ToolRegistryError::InvalidToolDefinition {
        id: tool_id.to_owned(),
        reason,
    };
    if surface.protocol_version != SURFACE_PROTOCOL_VERSION {
        return Err(invalid(format!(
            "unsupported Surface protocol {}",
            surface.protocol_version
        )));
    }
    if surface.api_version != SURFACE_API_VERSION {
        return Err(invalid(format!(
            "unsupported Surface API {}",
            surface.api_version
        )));
    }
    if surface.variants.is_empty() && surface.fallback_scene.is_none() {
        return Err(invalid(
            "Surface manifest must declare a runtime variant or fallback scene".to_owned(),
        ));
    }
    if surface.state_schema_version == 0 {
        return Err(invalid(
            "Surface state schema version must be at least 1".to_owned(),
        ));
    }
    for variant in &surface.variants {
        validate_surface_entry_path(tool_id, &variant.entry)?;
        let expected_extension = match variant.runtime {
            SurfaceRuntimeKind::Declarative => "json",
            SurfaceRuntimeKind::Javascript => "js",
            SurfaceRuntimeKind::Shader => "json",
            SurfaceRuntimeKind::LoomRemote => "json",
        };
        if Path::new(&variant.entry)
            .extension()
            .and_then(|value| value.to_str())
            != Some(expected_extension)
        {
            return Err(invalid(format!(
                "Surface {:?} entry must use .{expected_extension}",
                variant.runtime
            )));
        }
        for capability in &variant.required_capabilities {
            if !is_safe_surface_identifier(capability) {
                return Err(invalid(format!(
                    "unsafe Surface capability id {capability}"
                )));
            }
        }
    }
    if let Some(fallback) = &surface.fallback_scene {
        validate_surface_entry_path(tool_id, fallback)?;
        if Path::new(fallback)
            .extension()
            .and_then(|value| value.to_str())
            != Some("json")
        {
            return Err(invalid("Surface fallback scene must use .json".to_owned()));
        }
    }
    let mut migration_sources = HashSet::new();
    for migration in &surface.migrations {
        if migration.from == 0
            || migration.to == 0
            || migration.from >= migration.to
            || migration.to > surface.state_schema_version
        {
            return Err(invalid(format!(
                "Surface migration {} -> {} is invalid for state schema {}",
                migration.from, migration.to, surface.state_schema_version
            )));
        }
        if !migration_sources.insert(migration.from) {
            return Err(invalid(format!(
                "Surface state schema {} has more than one migration",
                migration.from
            )));
        }
        validate_surface_entry_path(tool_id, &migration.entry)?;
        if Path::new(&migration.entry)
            .extension()
            .and_then(|value| value.to_str())
            != Some("json")
        {
            return Err(invalid(
                "Surface state migration entries must use .json".to_owned(),
            ));
        }
    }
    for node in &surface.required_nodes {
        if !is_safe_surface_identifier(node) {
            return Err(invalid(format!("unsafe Surface node type {node}")));
        }
    }
    for capability in &surface.required_capabilities {
        if !is_safe_surface_identifier(capability) {
            return Err(invalid(format!(
                "unsafe Surface capability id {capability}"
            )));
        }
    }
    let mut view_ids = HashSet::new();
    for view in &surface.views {
        if !is_safe_surface_identifier(&view.id) {
            return Err(invalid(format!("unsafe Surface view id {}", view.id)));
        }
        if !view_ids.insert(view.id.as_str()) {
            return Err(invalid(format!("duplicate Surface view id {}", view.id)));
        }
        if view.label.trim().is_empty() || view.label.chars().count() > 80 {
            return Err(invalid(format!(
                "Surface view {} must declare a non-empty label of at most 80 characters",
                view.id
            )));
        }
        if view.full_size.width == 0
            || view.full_size.height == 0
            || view.full_size.width > 16_384
            || view.full_size.height > 16_384
        {
            return Err(invalid(format!(
                "Surface view {} full size must be between 1 and 16384 pixels",
                view.id
            )));
        }
    }
    if let Some(default_view_id) = surface.default_view_id.as_deref() {
        if !view_ids.contains(default_view_id) {
            return Err(invalid(format!(
                "Surface default view id {default_view_id} is not declared"
            )));
        }
    } else if !surface.views.is_empty() {
        return Err(invalid(
            "Surface manifests with views must declare defaultViewId".to_owned(),
        ));
    }
    let mut action_ids = HashSet::new();
    for action in &surface.actions {
        if !is_safe_surface_identifier(&action.id) {
            return Err(invalid(format!("unsafe Surface action id {}", action.id)));
        }
        if !action_ids.insert(action.id.as_str()) {
            return Err(invalid(format!(
                "duplicate Surface action id {}",
                action.id
            )));
        }
        if action.risk == SurfaceActionRisk::High && !action.confirmation {
            return Err(invalid(format!(
                "high-risk Surface action {} must require Host confirmation",
                action.id
            )));
        }
        if action
            .timeout_ms
            .is_some_and(|timeout| timeout == 0 || timeout > 300_000)
        {
            return Err(invalid(format!(
                "Surface action {} timeout must be between 1 and 300000 ms",
                action.id
            )));
        }
    }
    Ok(())
}

fn validate_surface_entry_path(tool_id: &str, entry: &str) -> ToolRegistryResult<()> {
    let path = Path::new(entry);
    let safe = !entry.trim().is_empty()
        && !entry.contains('\\')
        && !path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        });
    if safe {
        Ok(())
    } else {
        Err(ToolRegistryError::InvalidToolDefinition {
            id: tool_id.to_owned(),
            reason: format!("Surface entry path is unsafe: {entry}"),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ToolExecution {
    #[serde(rename_all = "camelCase")]
    CloudApi {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_output: Option<WorkflowOutputBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview_required_nodes: Vec<String>,
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
            Self::CloudApi {
                endpoint, method, ..
            } => {
                require_non_empty(tool_id, endpoint, "endpoint")?;
                require_non_empty(tool_id, method, "method")
            }
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
        self.save_tool_inner(tool, false)
    }

    pub(crate) fn save_packaged_tool(
        &self,
        tool: ToolDefinition,
    ) -> ToolRegistryResult<ToolDefinition> {
        self.save_tool_inner(tool, true)
    }

    fn save_tool_inner(
        &self,
        mut tool: ToolDefinition,
        replace_unpublished: bool,
    ) -> ToolRegistryResult<ToolDefinition> {
        self.apply_persisted_art_settings(&mut tool)?;
        tool.validate()?;
        self.ensure_root()?;

        let mut tools = self.read_tools()?;
        if replace_unpublished && tool.publisher_identity().is_some() {
            tools.retain(|existing| {
                existing.id != tool.id || existing.publisher_identity().is_some()
            });
        }
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
        let mut tools = match serde_json::from_str(&content) {
            Ok(tools) => tools,
            Err(error) => {
                let Some(tools) = recover_tools_with_trailing_delimiters(&content) else {
                    return Err(ToolRegistryError::Json(error));
                };
                self.write_corruption_backup(&content)?;
                self.write_tools(&tools)?;
                tools
            }
        };
        for tool in &mut tools {
            self.apply_persisted_art_settings(tool)?;
        }
        Ok(tools)
    }

    fn apply_persisted_art_settings(&self, tool: &mut ToolDefinition) -> ToolRegistryResult<()> {
        let Some(control_plane_root) = self.root.parent().filter(|_| {
            self.root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("tools"))
        }) else {
            return Ok(());
        };
        // A preferences lookup must never fail a registry read. The store now recovers from a
        // damaged settings file on its own, but the id validation in `get_optional` can still
        // reject a qualified id that `ToolDefinition::validate` accepted, and the read itself can
        // fail for reasons that have nothing to do with this tool (a permission error on the
        // control-plane directory). In every one of those cases the honest answer is "this Art has
        // no stored settings", not "the registry is unreadable and every Art disappears".
        let settings = art_settings::ArtSettingsStore::new(control_plane_root)
            .get_optional(&tool.qualified_id())
            .unwrap_or_default();
        if let Some(metadata) = tool
            .metadata
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        {
            metadata.remove("artUserSettings");
        }
        if let Some(settings) = settings {
            art_settings::apply_settings_metadata(tool, &settings);
        }
        Ok(())
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

const MAX_CACHED_MCP_SESSIONS: usize = 8;
const MCP_SESSION_IDLE_LIFETIME: Duration = Duration::from_secs(60);

struct CachedMcpSession {
    key: String,
    client: loom_mcp::McpClient,
    tools: Option<serde_json::Value>,
    listing_failure: Option<String>,
    reusable: bool,
    last_used: Instant,
}

#[derive(Default)]
struct McpSessionPool {
    sessions: Vec<CachedMcpSession>,
}

thread_local! {
    static MCP_SESSION_POOL: RefCell<McpSessionPool> = RefCell::new(McpSessionPool::default());
}

fn mcp_session_key(
    server: &loom_mcp::McpServerConfig,
    timeout: Option<Duration>,
) -> ToolRegistryResult<String> {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(server)?);
    hasher.update(format!("{:?}", crate::network_policy::runtime_proxy()));
    hasher.update(
        timeout
            .map(|value| value.as_millis())
            .unwrap_or_default()
            .to_le_bytes(),
    );
    Ok(format!("{:x}", hasher.finalize()))
}

fn take_cached_mcp_session(key: &str) -> Option<CachedMcpSession> {
    let now = Instant::now();
    let (session, expired) = MCP_SESSION_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let mut expired = Vec::new();
        let mut index = 0;
        while index < pool.sessions.len() {
            if now.saturating_duration_since(pool.sessions[index].last_used)
                >= MCP_SESSION_IDLE_LIFETIME
            {
                expired.push(pool.sessions.remove(index));
            } else {
                index += 1;
            }
        }
        let session = pool
            .sessions
            .iter()
            .position(|session| session.key == key)
            .map(|index| pool.sessions.remove(index));
        (session, expired)
    });
    for mut session in expired {
        let _ = session.client.close();
    }
    session
}

fn return_cached_mcp_session(mut session: CachedMcpSession) {
    session.last_used = Instant::now();
    let evicted = MCP_SESSION_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let duplicate = pool.sessions.iter().any(|cached| cached.key == session.key);
        if duplicate {
            Some(session)
        } else {
            pool.sessions.push(session);
            if pool.sessions.len() > MAX_CACHED_MCP_SESSIONS {
                let oldest = pool
                    .sessions
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, session)| session.last_used)
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                Some(pool.sessions.remove(oldest))
            } else {
                None
            }
        }
    });
    if let Some(mut evicted) = evicted {
        let _ = evicted.client.close();
    }
}

#[cfg(test)]
fn clear_cached_mcp_sessions_for_current_thread() {
    let sessions = MCP_SESSION_POOL.with(|pool| std::mem::take(&mut pool.borrow_mut().sessions));
    for mut session in sessions {
        let _ = session.client.close();
    }
}

fn acquire_mcp_session(
    tool: &ToolDefinition,
    server: &loom_mcp::McpServerConfig,
    timeout: Option<Duration>,
    cancellation: Option<&AtomicBool>,
) -> ToolRegistryResult<CachedMcpSession> {
    let key = mcp_session_key(server, timeout)?;
    if let Some(session) = take_cached_mcp_session(&key) {
        return Ok(session);
    }
    let mut client = match timeout {
        Some(timeout) => loom_mcp::McpClient::connect_with_timeout(server, timeout)?,
        None => loom_mcp::McpClient::connect(server)?,
    };
    let initialize = match cancellation {
        Some(cancellation) => client.initialize_cancellable(cancellation),
        None => client.initialize(),
    };
    if let Err(error) = initialize {
        client.cancel();
        return Err(mcp_execution_error(tool, error));
    }
    let listing = match cancellation {
        Some(cancellation) => client.list_tools_cancellable(cancellation),
        None => client.list_tools(),
    };
    let (tools, listing_failure, reusable) = match listing {
        Ok(tools) => (Some(tools), None, true),
        Err(loom_mcp::McpError::Cancelled) => {
            client.cancel();
            return Err(ToolRegistryError::ExecutionCancelled {
                id: tool.id.clone(),
            });
        }
        Err(error) => (None, Some(error.to_string()), false),
    };
    Ok(CachedMcpSession {
        key,
        client,
        tools,
        listing_failure,
        reusable,
        last_used: Instant::now(),
    })
}

pub fn execute_tool(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    execute_tool_with_optional_timeout(tool, mcp_servers, arguments, None, None)
}

pub fn execute_tool_with_timeout(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
    timeout: Duration,
) -> ToolRegistryResult<serde_json::Value> {
    execute_tool_with_optional_timeout(
        tool,
        mcp_servers,
        arguments,
        Some(timeout.max(Duration::from_millis(1))),
        None,
    )
}

pub fn execute_tool_with_timeout_and_cancellation(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> ToolRegistryResult<serde_json::Value> {
    execute_tool_with_optional_timeout(
        tool,
        mcp_servers,
        arguments,
        Some(timeout.max(Duration::from_millis(1))),
        Some(cancellation),
    )
}

fn execute_tool_with_optional_timeout(
    tool: &ToolDefinition,
    mcp_servers: &[loom_mcp::McpServerConfig],
    arguments: serde_json::Value,
    timeout: Option<Duration>,
    cancellation: Option<&AtomicBool>,
) -> ToolRegistryResult<serde_json::Value> {
    tool.validate()?;
    if !tool.enabled {
        return Err(ToolRegistryError::ExecutionRejected {
            id: tool.id.clone(),
        });
    }

    let arguments = prepare_tool_arguments(tool, arguments)?;
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

            let stop_if_cancelled = || -> ToolRegistryResult<()> {
                if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
                    return Err(ToolRegistryError::ExecutionCancelled {
                        id: tool.id.clone(),
                    });
                }
                Ok(())
            };

            stop_if_cancelled()?;
            let mut session = acquire_mcp_session(tool, server, timeout, cancellation)?;
            let operation = (|| -> ToolRegistryResult<serde_json::Value> {
                stop_if_cancelled()?;
                let normalized_arguments = normalize_mcp_call_arguments(
                    &arguments,
                    session
                        .tools
                        .as_ref()
                        .and_then(|tools| find_mcp_tool_input_schema(tools, tool_name)),
                );
                stop_if_cancelled()?;
                let call = match cancellation {
                    Some(cancellation) => session.client.call_tool_cancellable(
                        tool_name,
                        normalized_arguments.clone(),
                        cancellation,
                    ),
                    None => session
                        .client
                        .call_tool(tool_name, normalized_arguments.clone()),
                };
                let result = match call {
                    Ok(value) => normalize_mcp_result(tool, &normalized_arguments, value),
                    Err(loom_mcp::McpError::Cancelled) => {
                        session.reusable = false;
                        return Err(ToolRegistryError::ExecutionCancelled {
                            id: tool.id.clone(),
                        });
                    }
                    Err(error) => {
                        session.reusable = matches!(error, loom_mcp::McpError::JsonRpc(_));
                        return Err(mcp_call_error(error, session.listing_failure.as_deref()));
                    }
                };
                stop_if_cancelled()?;
                Ok(result)
            })();
            let cancelled = cancellation.is_some_and(|token| token.load(Ordering::Acquire));
            if operation.is_ok() && session.reusable && !cancelled {
                return_cached_mcp_session(session);
            } else if cancelled {
                session.client.cancel();
            } else {
                let _ = session.client.close();
            }
            operation
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
            cloud_api_timeout(tool, timeout),
            cancellation,
        ),
        ToolExecution::FrameworkArt { framework } => match (timeout, cancellation) {
            (Some(timeout), Some(cancellation)) => {
                framework_process::execute_framework_art_with_timeout_and_cancellation(
                    tool,
                    framework,
                    arguments,
                    timeout,
                    cancellation,
                )
            }
            (Some(timeout), None) => framework_process::execute_framework_art_with_timeout(
                tool, framework, arguments, timeout,
            ),
            (None, _) => framework_process::execute_framework_art(tool, framework, arguments),
        },
        _ => Err(ToolRegistryError::UnsupportedExecution {
            id: tool.id.clone(),
            execution_type: execution_type_name(&tool.execution),
        }),
    }
}

pub fn prepare_tool_arguments(
    tool: &ToolDefinition,
    arguments: serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    let arguments = art_settings::merge_tool_arguments(tool, arguments);
    art_settings::resolve_tool_value_bindings(tool, arguments).map_err(|error| {
        ToolRegistryError::ParameterBinding {
            id: tool.qualified_id(),
            reason: error.to_string(),
        }
    })
}

fn mcp_execution_error(tool: &ToolDefinition, error: loom_mcp::McpError) -> ToolRegistryError {
    match error {
        loom_mcp::McpError::Cancelled => ToolRegistryError::ExecutionCancelled {
            id: tool.id.clone(),
        },
        error => ToolRegistryError::Mcp(error),
    }
}

/// Turn a failed MCP call into the error that leaves this crate, folding in an earlier listing failure.
///
/// When the server listed its tools normally, the call error is reported as it arrived. When the
/// listing failed first, the two are reported together: the arguments were sent without the schema
/// that would have shaped them, which is a plausible cause of the rejection and is otherwise invisible
/// to whoever reads the error. Both texts are bounded, since either can carry a server's response body.
fn mcp_call_error(error: loom_mcp::McpError, listing_failure: Option<&str>) -> ToolRegistryError {
    match listing_failure {
        Some(reason) => ToolRegistryError::Mcp(loom_mcp::McpError::Protocol(format!(
            "{}; the server's tool listing failed first, so the arguments were sent without schema \
             guidance: {}",
            bounded_error_text(&error.to_string()),
            bounded_error_text(reason)
        ))),
        None => ToolRegistryError::from(error),
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
        .and_then(|tool| tool.get("inputSchema"))
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
        normalized.insert(key.clone(), normalize_mcp_argument_value(value, schema));
    }
    serde_json::Value::Object(normalized)
}

fn normalize_mcp_argument_value(
    value: &serde_json::Value,
    schema: Option<&serde_json::Value>,
) -> serde_json::Value {
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

fn parse_bool_string(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn execute_cloud_api_tool(
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

fn run_cloud_future<F, T>(future: F) -> Result<T, String>
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
enum CloudTransportError {
    Cancelled,
    Request(reqwest::Error),
    ResponseTooLarge,
    InvalidUtf8,
}

struct CloudWireResponse {
    status: reqwest::StatusCode,
    content_type: String,
    body: CloudResponseBody,
}

enum CloudResponseBody {
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

enum CloudBodyAccumulator {
    Image { data_url: String, pending: Vec<u8> },
    Text(Vec<u8>),
}

impl CloudBodyAccumulator {
    fn new(image_mime_type: Option<&str>, content_length: Option<u64>) -> Self {
        let raw_capacity = content_length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or_default()
            .min(MAX_CLOUD_RESPONSE_BYTES);
        if let Some(mime_type) = image_mime_type {
            let encoded_capacity = raw_capacity
                .saturating_add(2)
                .saturating_div(3)
                .saturating_mul(4)
                .saturating_add(mime_type.len() + 13);
            let mut data_url = String::with_capacity(encoded_capacity);
            data_url.push_str("data:");
            data_url.push_str(mime_type);
            data_url.push_str(";base64,");
            Self::Image {
                data_url,
                pending: Vec::with_capacity(2),
            }
        } else {
            Self::Text(Vec::with_capacity(raw_capacity))
        }
    }

    fn push(&mut self, mut chunk: &[u8]) {
        match self {
            Self::Text(bytes) => bytes.extend_from_slice(chunk),
            Self::Image { data_url, pending } => {
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
        }
    }

    fn finish(mut self) -> Result<CloudResponseBody, CloudTransportError> {
        match &mut self {
            Self::Image { data_url, pending } => {
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

async fn wait_for_cloud_cancellation(cancellation: &AtomicBool) {
    while !cancellation.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn execute_cloud_http_request(
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

fn cloud_image_mime_type(content_type: &str) -> Option<&str> {
    content_type
        .split(';')
        .next()
        .map(str::trim)
        .filter(|mime_type| mime_type.to_ascii_lowercase().starts_with("image/"))
}

/// Resolve the deadline for a cloud API call.
///
/// A caller that states a deadline gets it: `execute_tool_with_timeout` is how the daemon passes
/// the run budget down, and clamping it to the 30 s default meant the budget could only ever be
/// shortened, never honoured. A package may declare `metadata.cloudApi.timeoutMs` for the API it
/// wraps, which applies when the caller states nothing. Both are bounded by
/// [`CLOUD_API_MAX_TIMEOUT`] so a bad number cannot pin a worker thread indefinitely, and by one
/// millisecond at the bottom because `reqwest` treats a zero timeout as "no timeout at all".
fn cloud_api_timeout(tool: &ToolDefinition, requested: Option<Duration>) -> Duration {
    let declared = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("cloudApi"))
        .and_then(|cloud| cloud.get("timeoutMs"))
        .and_then(serde_json::Value::as_u64)
        .map(Duration::from_millis);
    requested
        .or(declared)
        .unwrap_or(CLOUD_API_TIMEOUT)
        .clamp(Duration::from_millis(1), CLOUD_API_MAX_TIMEOUT)
}

fn cloud_network_policy(tool: &ToolDefinition) -> crate::network_policy::OutboundPolicy {
    let network = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("permissionPolicy"))
        .and_then(|policy| policy.get("network"));
    crate::network_policy::OutboundPolicy {
        // Loopback is off unless the package asks for it, matching `OutboundPolicy::default`. A
        // cloud Art that declares no network policy at all used to be allowed to call
        // `http://localhost:*` and `http://127.0.0.1:*` in cleartext, which reaches the Loom
        // daemon's own HTTP surface, Hook, and any local model server — while carrying the Art's
        // credential headers. An Art that genuinely talks to a local service knows it does, so it
        // can say so.
        allow_http_loopback: network
            .and_then(|network| network.get("allowLocalhost"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
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

/// Outbound policy for downloading an image candidate that an MCP server chose.
///
/// The candidate URL comes out of the search result, so its host is whatever CDN the upstream
/// service happens to serve images from and a domain allowlist cannot be applied here — the
/// domains an image-search tool declares name its API host, not the image hosts. What can be
/// applied is the local-network boundary. Both download paths used to hardcode loopback on, which
/// handed any MCP server a request primitive into the Loom daemon's own HTTP surface, Hook, or a
/// local model server, just by returning `http://127.0.0.1:<port>/...` as an image URL. Loopback
/// and private networks are now off unless the tool declares them, which is the same lever a cloud
/// Art uses.
fn mcp_image_download_policy(tool: &ToolDefinition) -> crate::network_policy::OutboundPolicy {
    let declared = cloud_network_policy(tool);
    crate::network_policy::OutboundPolicy {
        allow_http_loopback: declared.allow_http_loopback,
        allow_private_networks: declared.allow_private_networks,
        ..crate::network_policy::OutboundPolicy::default()
    }
}

async fn build_cloud_multipart_form(
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
        // `__DISABLED__` is the author's own way of saying "leave this field out", so it is honoured
        // in silence.
        if rendered_value == "__DISABLED__" {
            continue;
        }
        // A placeholder the template declared and no argument filled used to remove the field from the
        // request, so the API answered with a confusing complaint about a parameter Loom never sent.
        // The check reads the template's own placeholders rather than looking for `{{` in the result,
        // because an argument value is allowed to contain braces and used to be dropped for it.
        if let Some(placeholder) = unresolved_cloud_template_placeholder(&value, &rendered_value) {
            return Err(ToolRegistryError::CloudTemplate {
                id: tool.id.clone(),
                field: "body",
                reason: format!(
                    "multipart field `{key}` still contains the unresolved placeholder \
                     `{placeholder}`"
                ),
            });
        }
        if rendered_value.is_empty() {
            continue;
        }

        if is_cloud_multipart_file_field(&value) {
            if rendered_value.starts_with("data:") {
                let mime =
                    data_url_mime_type(&rendered_value).unwrap_or("application/octet-stream");
                let extension = match mime {
                    "image/jpeg" => "jpg",
                    "image/webp" => "webp",
                    _ => "png",
                };
                let bytes =
                    loom_image_io::decode_data_url_bytes(&rendered_value).map_err(|error| {
                        ToolRegistryError::CloudTemplate {
                            id: tool.id.clone(),
                            field: "body",
                            reason: error.to_string(),
                        }
                    })?;
                let part = multipart::Part::bytes(bytes)
                    .file_name(format!("loom-cloud-input.{extension}"))
                    .mime_str(mime)
                    .map_err(|error| ToolRegistryError::CloudTemplate {
                        id: tool.id.clone(),
                        field: "body",
                        reason: error.to_string(),
                    })?;
                form = form.part(key, part);
            } else if is_remote_url_value(&rendered_value) {
                // Some hosted APIs take the image as a URL in the same field an author binds a
                // path to. A remote URL is not a local file, so it travels as a plain text field.
                form = form.text(key, rendered_value);
            } else {
                let path = cloud_multipart_upload_path(tool, &key, &rendered_value)?;
                let part = multipart::Part::file(path)
                    .await
                    .map_err(ToolRegistryError::Io)?;
                form = form.part(key, part);
            }
        } else {
            form = form.text(key, rendered_value);
        }
    }
    Ok(form)
}

/// Decide whether a multipart field carries a file.
///
/// Only the author's own template decides this. The heuristic used to also treat any field *named*
/// `file`, `image`, `image_file`, or `*_file` as a file field, so a caller could pass an arbitrary
/// absolute path as the value of an ordinary text field and the host would read that file off disk
/// and upload it to the third-party endpoint. An author who wants a file upload writes the path
/// binding — `{{inputs.x.path}}` — which is exactly what the Desktop cloud editor's multipart help
/// text tells them to write.
fn is_cloud_multipart_file_field(template_value: &str) -> bool {
    template_value.contains(".path}}") || template_value.contains("inputs.image}}")
}

fn is_remote_url_value(rendered_value: &str) -> bool {
    let lowered = rendered_value.to_ascii_lowercase();
    lowered.starts_with("http://") || lowered.starts_with("https://")
}

/// Resolve the local file a declared multipart file field wants to upload.
///
/// The rendered value comes from the execution arguments, so the previous `Path::exists` check
/// meant "read whatever path the caller names and upload it": a caller could aim a hosted Art at
/// an SSH key or a credential store and exfiltrate it through the Art's own endpoint. The path now
/// has to canonicalize to a real file inside a root Loom itself owns, the way the framework arm
/// confines every path it accepts.
fn cloud_multipart_upload_path(
    tool: &ToolDefinition,
    field: &str,
    rendered_value: &str,
) -> ToolRegistryResult<PathBuf> {
    let template_error = |reason: String| ToolRegistryError::CloudTemplate {
        id: tool.id.clone(),
        field: "body",
        reason,
    };
    let canonical = fs::canonicalize(rendered_value).map_err(|error| {
        template_error(format!(
            "multipart field `{field}` cannot resolve upload path `{rendered_value}`: {error}"
        ))
    })?;
    if !canonical.is_file() {
        return Err(template_error(format!(
            "multipart field `{field}` upload path `{}` is not a file",
            canonical.display()
        )));
    }
    let inside_allowed_root = cloud_multipart_upload_roots(tool)
        .iter()
        .any(|root| cloud_upload_root_allows(root, &canonical));
    if !inside_allowed_root {
        return Err(template_error(format!(
            "multipart field `{field}` upload path `{}` resolves outside the Art package, control plane, and staged input roots",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// Roots a cloud Art may upload a local file from: its own package directory, the control plane
/// root that holds Art state, cache, and outputs, and the host temp directory the daemon stages
/// call inputs in.
fn cloud_multipart_upload_roots(tool: &ToolDefinition) -> Vec<PathBuf> {
    let mut roots = vec![std::env::temp_dir()];
    if let Some(package_dir) = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("dir"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
    {
        roots.push(PathBuf::from(package_dir));
    }
    if let Some(control_plane_root) = crate::art_settings::control_plane_root_for_tool(tool) {
        roots.push(control_plane_root);
    }
    roots
}

/// The host temp directory is shared with every other program on the machine, so being inside it
/// is not by itself a reason to upload a file. Only Loom's own staging entries — every temp path
/// this workspace creates is prefixed `loom-` — count as allowed inside it. Any other allowed root
/// vouches for its whole subtree, including a control plane root that happens to live under temp.
fn cloud_upload_root_allows(root: &Path, canonical: &Path) -> bool {
    let Ok(canonical_root) = fs::canonicalize(root) else {
        return false;
    };
    if !canonical.starts_with(&canonical_root) {
        return false;
    }
    if fs::canonicalize(std::env::temp_dir()).is_ok_and(|temp_root| temp_root == canonical_root) {
        return canonical
            .strip_prefix(&canonical_root)
            .ok()
            .and_then(|relative| relative.components().next())
            .and_then(|component| component.as_os_str().to_str())
            .is_some_and(|first| first.starts_with("loom-"));
    }
    true
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
    body: CloudResponseBody,
) -> ToolRegistryResult<serde_json::Value> {
    let body = match body {
        CloudResponseBody::ImageDataUrl(data_url) => {
            let mime_type = cloud_image_mime_type(content_type).unwrap_or("image/png");
            return Ok(image_content_response(&data_url, mime_type));
        }
        CloudResponseBody::Text(body) => body.trim().to_owned(),
    };
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
                body: bounded_error_text(&body),
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
        if let Some(image) = cloud_json_image_response(output) {
            return image;
        }
    }
    if let Some(image) = cloud_json_image_response(&value) {
        return image;
    }
    if let Some(text) = value.get("text").and_then(serde_json::Value::as_str) {
        return text_content_response(text);
    }
    text_content_response(&value.to_string())
}

/// Read a `data` string out of a cloud JSON object as an image, when the response gives a reason to
/// believe it is one.
///
/// Three things count as a reason: the value carries its own `data:image/` prefix, the response
/// labels it with an `image/*` MIME type, or the value is long enough and regular enough for
/// `looks_like_base64_payload` to recognize. Without one of those, an opaque token, a signature, or
/// an encoded cursor under `data` used to reach the canvas as `data:image/png;base64,<token>` — a
/// broken image with no diagnostic anywhere. Such a value now falls through to the text handling in
/// `normalize_cloud_json_value`, which is where an API that returns a string means it to go.
fn cloud_json_image_response(value: &serde_json::Value) -> Option<serde_json::Value> {
    let data = value.get("data").and_then(serde_json::Value::as_str)?;
    let declared = value
        .get("mimeType")
        .or_else(|| value.get("mime_type"))
        .and_then(serde_json::Value::as_str);
    let labelled_image = declared
        .is_some_and(|mime_type| mime_type.trim().to_ascii_lowercase().starts_with("image/"));
    if !(data.starts_with("data:image/") || labelled_image || looks_like_base64_payload(data)) {
        return None;
    }
    Some(image_content_response(
        data,
        declared.unwrap_or("image/png"),
    ))
}

#[derive(Clone, Debug, Default)]
struct McpImageCandidate {
    image_url: String,
    /// The server's own string for `image_url`, kept when normalization rewrote it.
    ///
    /// The rewrite drops a CDN modifier off the end of the URL, which is often the only thing making
    /// the URL fetchable. But it cuts at the last image extension in the path, so for a path like
    /// `/logo.png/v2/actual` it cuts away the real file and leaves a URL for a different image. The
    /// string it was derived from is kept here so the download can fall back to it rather than give up
    /// on an address nobody ever sent.
    alternate_image_url: Option<String>,
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
        let download_policy = mcp_image_download_policy(tool);
        if let Some(image) = normalize_mcp_image_result(arguments, &value, &download_policy) {
            return image;
        }
        if let Some(message) = friendly_mcp_image_result_message(&value) {
            let candidates = collect_mcp_image_candidates(&value);
            if !candidates.is_empty() {
                let selection = selected_mcp_image_candidate_index(arguments, candidates.len());
                let mut response = text_content_response(&message);
                attach_mcp_image_candidate_metadata(
                    &mut response,
                    &candidates,
                    &selection,
                    selection.index,
                );
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
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(execution_type.as_str(), "image_buffer" | "image_path")
}

fn normalize_mcp_image_result(
    arguments: &serde_json::Value,
    value: &serde_json::Value,
    policy: &crate::network_policy::OutboundPolicy,
) -> Option<serde_json::Value> {
    let candidates = collect_mcp_image_candidates(value);
    if candidates.is_empty() {
        return None;
    }
    let selection = selected_mcp_image_candidate_index(arguments, candidates.len());
    let (mut normalized, delivered_index) = image_response_from_mcp_candidates(
        &candidates,
        selection.index,
        policy,
        McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET),
    )?;
    attach_mcp_image_candidate_metadata(&mut normalized, &candidates, &selection, delivered_index);
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

/// Wall-clock budget for the whole download loop over one MCP tool result's image candidates.
///
/// A candidate does not only fail fast. One candidate expands into the image URL and then the
/// thumbnail, each of those into the URL as given and then the modifier-stripped form, and each of
/// those into a reqwest attempt and then the PowerShell fallback — every attempt bounded only by
/// [`CLOUD_API_TIMEOUT`]. A result whose candidates all point at a host that accepts the connection
/// and then never answers therefore used to hold one tool call for minutes per candidate, and a
/// result carrying the full [`MAX_MCP_IMAGE_CANDIDATES`] for about an hour. The loop now runs
/// against one deadline and every network attempt is bounded by whatever is left of it.
const MCP_IMAGE_DOWNLOAD_BUDGET: Duration = Duration::from_secs(90);

/// Ceiling on how many candidates one call tries to download before giving up.
///
/// The candidate list is as long as the MCP server chose to make it. Retrying dozens of them is not
/// how a usable image search behaves: if the first handful cannot be fetched, reporting that back is
/// better than spending the whole budget.
const MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS: usize = 6;

/// The least remaining budget worth spending on one more network attempt.
const MIN_MCP_IMAGE_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);

/// Remaining wall-clock budget for one MCP image download loop.
#[derive(Clone, Copy, Debug)]
struct McpImageDownloadDeadline {
    deadline: Instant,
}

impl McpImageDownloadDeadline {
    fn starting_now(budget: Duration) -> Self {
        Self {
            deadline: Instant::now()
                .checked_add(budget)
                .unwrap_or_else(Instant::now),
        }
    }

    /// The timeout for one more network attempt, or `None` when too little of the budget is left for
    /// another request to be worth starting.
    fn next_attempt_timeout(&self) -> Option<Duration> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        (remaining >= MIN_MCP_IMAGE_ATTEMPT_TIMEOUT).then(|| remaining.min(CLOUD_API_TIMEOUT))
    }
}

fn image_response_from_mcp_candidates(
    candidates: &[McpImageCandidate],
    requested_index: usize,
    policy: &crate::network_policy::OutboundPolicy,
    deadline: McpImageDownloadDeadline,
) -> Option<(serde_json::Value, usize)> {
    if candidates.is_empty() {
        return None;
    }
    let ordered = std::iter::once(requested_index).chain(
        candidates
            .iter()
            .enumerate()
            .map(|(index, _)| index)
            .filter(|index| *index != requested_index),
    );
    for candidate_index in ordered.take(MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS) {
        if deadline.next_attempt_timeout().is_none() {
            break;
        }
        let candidate = candidates.get(candidate_index)?;
        if let Some(response) = image_response_from_mcp_candidate(candidate, policy, deadline) {
            return Some((response, candidate_index));
        }
    }
    None
}

/// Nesting limit for the walk over an MCP tool result.
///
/// The walk needs a counter of its own because a string inside the result that begins with `{`
/// or `[` is parsed again as JSON, and each parse starts serde's nesting budget over while the
/// walk is already that many frames into the native stack. A result that is individually
/// shallow at every hop therefore used to be able to drive the walk arbitrarily deep and abort
/// the process on a stack overflow. Tool results come from servers Loom does not control, so
/// the limit is generous enough for real image-search payloads and nothing more.
const MAX_MCP_IMAGE_CANDIDATE_DEPTH: usize = 24;

/// Ceiling on how many image candidates one MCP tool result may contribute.
///
/// Without it a flat array of a million URLs is copied into the response metadata and sent to
/// the client, which is a much cheaper attack than nesting.
const MAX_MCP_IMAGE_CANDIDATES: usize = 64;

/// Accumulator for the image-candidate walk, holding the results found so far and the dedup set.
#[derive(Default)]
struct McpImageCandidateWalk {
    candidates: Vec<McpImageCandidate>,
    seen: std::collections::BTreeSet<String>,
}

impl McpImageCandidateWalk {
    fn is_full(&self) -> bool {
        self.candidates.len() >= MAX_MCP_IMAGE_CANDIDATES
    }

    fn push(&mut self, candidate: McpImageCandidate) {
        if self.seen.insert(candidate.image_url.clone()) {
            self.candidates.push(candidate);
        }
    }
}

fn collect_mcp_image_candidates(value: &serde_json::Value) -> Vec<McpImageCandidate> {
    let mut walk = McpImageCandidateWalk::default();
    if let Some(structured_content) = value.get("structuredContent") {
        collect_mcp_image_candidates_from_value(structured_content, 0, &mut walk);
    }
    if let Some(structured_content) = value
        .get("result")
        .and_then(|result| result.get("structuredContent"))
    {
        collect_mcp_image_candidates_from_value(structured_content, 0, &mut walk);
    }
    if walk.candidates.is_empty() {
        if let Some(content) = value
            .get("content")
            .or_else(|| value.get("result").and_then(|result| result.get("content")))
            .and_then(serde_json::Value::as_array)
        {
            for item in content {
                if walk.is_full() {
                    break;
                }
                let Some(text) = item.get("text").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let Some(parsed) = parse_mcp_image_candidate_json(text, 0) else {
                    continue;
                };
                collect_mcp_image_candidates_from_value(&parsed, 0, &mut walk);
            }
        }
    }
    walk.candidates
}

fn collect_mcp_image_candidates_from_value(
    value: &serde_json::Value,
    depth: usize,
    walk: &mut McpImageCandidateWalk,
) {
    if depth > MAX_MCP_IMAGE_CANDIDATE_DEPTH || walk.is_full() {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            if let Some(candidate) = image_candidate_from_object(map) {
                walk.push(candidate);
                return;
            }
            for child in map.values() {
                collect_mcp_image_candidates_from_value(child, depth + 1, walk);
                if walk.is_full() {
                    return;
                }
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_mcp_image_candidates_from_value(child, depth + 1, walk);
                if walk.is_full() {
                    return;
                }
            }
        }
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if (looks_like_image_url(trimmed) || trimmed.starts_with("data:image/"))
                && walk.seen.insert(trimmed.to_owned())
            {
                walk.candidates.push(McpImageCandidate {
                    image_url: trimmed.to_owned(),
                    alternate_image_url: None,
                    title: None,
                    thumbnail_url: None,
                    source_page_url: None,
                    width: None,
                    height: None,
                });
                return;
            }
            if matches!(trimmed.as_bytes().first(), Some(b'{') | Some(b'[')) {
                if let Some(parsed) = parse_mcp_image_candidate_json(trimmed, depth) {
                    collect_mcp_image_candidates_from_value(&parsed, depth + 1, walk);
                }
            }
        }
        _ => {}
    }
}

/// Parse a string inside an MCP tool result that itself looks like a JSON document.
///
/// The nesting budget handed to the parser is what is *left* of the walk's budget rather than a
/// fresh one, which is the whole point: the parse happens `depth` frames into the walk, so
/// letting each hop spend the full budget again is what made the recursion unbounded.
fn parse_mcp_image_candidate_json(text: &str, depth: usize) -> Option<serde_json::Value> {
    let remaining = MAX_MCP_IMAGE_CANDIDATE_DEPTH.checked_sub(depth)?;
    loom_security::json::parse_within_limits(
        text,
        "MCP tool result",
        loom_security::json::MAX_PROCESS_RESPONSE_BYTES,
        remaining,
    )
    .ok()
}

fn image_candidate_from_object(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<McpImageCandidate> {
    let properties = map.get("properties").and_then(serde_json::Value::as_object);
    let CandidateUrl {
        url: image_url,
        original: alternate_image_url,
    } = find_image_url_in_object(map).or_else(|| properties.and_then(find_image_url_in_object))?;
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
            // A `url` that is the image itself is not the page the image sits on. Both forms of the
            // image URL are excluded, because the rewritten one is what `image_url` holds while the
            // original is what this field usually carries.
            .filter(|url| {
                *url != image_url
                    && Some(*url) != alternate_image_url.as_deref()
                    && (url.starts_with("http://") || url.starts_with("https://"))
            })
            .map(str::to_owned)
    });
    Some(McpImageCandidate {
        image_url,
        alternate_image_url,
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
    for suffix in IMAGE_URL_EXTENSIONS {
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

/// A candidate URL in the form it will be requested in, together with the string it came from.
///
/// Normalization sometimes rewrites the server's string. When it does, both forms are worth keeping:
/// the rewrite is what usually works, and the original is what is right when the rewrite guessed wrong.
struct CandidateUrl {
    url: String,
    /// Present only when `url` is a rewrite, so a caller can tell the two cases apart.
    original: Option<String>,
}

impl CandidateUrl {
    fn verbatim(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            original: None,
        }
    }
}

fn normalize_image_candidate_url(
    value: &str,
    allow_remote_without_extension: bool,
) -> Option<CandidateUrl> {
    let trimmed = value.trim();
    if trimmed.starts_with("data:image/") || looks_like_image_url(trimmed) {
        return Some(CandidateUrl::verbatim(trimmed));
    }
    if let Some(stripped) = strip_image_url_modifiers(trimmed) {
        if looks_like_image_url(&stripped)
            || (allow_remote_without_extension && looks_like_remote_url(&stripped))
        {
            return Some(CandidateUrl {
                url: stripped,
                original: Some(trimmed.to_owned()),
            });
        }
    }
    if allow_remote_without_extension && looks_like_remote_url(trimmed) {
        return Some(CandidateUrl::verbatim(trimmed));
    }
    None
}

fn find_image_url_in_object(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<CandidateUrl> {
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
                    return Some(url.url);
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
                    return Some(url.url);
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

/// Which candidate the arguments asked for, and which one this crate can actually start from.
struct McpImageCandidateSelection {
    /// The index the arguments named, kept exactly as asked so an out-of-range request stays reportable.
    requested: Option<usize>,
    /// The index to start from: the requested one when it exists, the last candidate otherwise.
    index: usize,
}

fn selected_mcp_image_candidate_index(
    arguments: &serde_json::Value,
    candidate_count: usize,
) -> McpImageCandidateSelection {
    let requested = arguments
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
        .and_then(value_as_usize);
    if candidate_count == 0 {
        return McpImageCandidateSelection {
            requested,
            index: 0,
        };
    }
    McpImageCandidateSelection {
        requested,
        index: requested
            .unwrap_or(0)
            .min(candidate_count.saturating_sub(1)),
    }
}

/// Say why the candidate that was delivered is not the one the arguments named.
///
/// Two things move the choice, and both used to happen in silence. An index past the end of the list is
/// clamped to the last candidate, so asking for the eighth of three quietly returned the third. A
/// candidate that cannot be downloaded falls through to another one, so the canvas was told a different
/// image had been selected than the one it asked for. Neither is an error worth failing the call over —
/// an image still arrived — but neither should be invisible either.
fn mcp_image_selection_note(
    requested: usize,
    clamped: usize,
    delivered: usize,
    candidate_count: usize,
) -> Option<String> {
    let mut notes = Vec::new();
    if requested >= candidate_count {
        notes.push(format!(
            "requested index {requested} is past the last of {candidate_count} candidates, \
             so candidate {clamped} was used instead"
        ));
    }
    if delivered != clamped {
        notes.push(format!(
            "candidate {clamped} could not be downloaded, so candidate {delivered} was used instead"
        ));
    }
    if notes.is_empty() {
        None
    } else {
        Some(notes.join("; "))
    }
}

fn value_as_usize(value: &serde_json::Value) -> Option<usize> {
    match value {
        serde_json::Value::Number(number) => number.as_u64().map(|value| value as usize),
        serde_json::Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

fn attach_mcp_image_candidate_metadata(
    image_result: &mut serde_json::Value,
    candidates: &[McpImageCandidate],
    selection: &McpImageCandidateSelection,
    delivered_index: usize,
) {
    let Some(result_object) = image_result.as_object_mut() else {
        return;
    };
    // `selectedIndex` stays the candidate the canvas is actually showing — the daemon reads it to know
    // which item is on screen. When that is not the one the arguments named, both the request and the
    // reason are recorded alongside it instead of leaving the difference unexplained.
    let note = selection.requested.and_then(|requested| {
        mcp_image_selection_note(
            requested,
            selection.index,
            delivered_index,
            candidates.len(),
        )
    });
    let mut candidate_metadata = serde_json::json!({
        "kind": "image.candidates",
        "selectedIndex": delivered_index,
        "items": candidates
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
    });
    if let (Some(note), Some(requested)) = (note, selection.requested) {
        let object = candidate_metadata
            .as_object_mut()
            .expect("candidate metadata is built as an object");
        object.insert("requestedIndex".to_owned(), requested.into());
        object.insert("selectionNote".to_owned(), note.into());
    }
    result_object.insert(
        "loomMetadata".to_owned(),
        serde_json::json!({ "candidates": candidate_metadata }),
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
    IMAGE_URL_EXTENSIONS
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

/// Whether a declared MIME type is one this crate is willing to deliver as an image.
fn is_supported_image_mime_type(mime_type: &str) -> bool {
    let mime_type = mime_type.trim().to_ascii_lowercase();
    SUPPORTED_IMAGE_MIME_TYPES
        .iter()
        .any(|supported| *supported == mime_type)
}

fn looks_like_remote_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn download_mcp_image_candidate(
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

fn download_mcp_image_candidate_with_reqwest(
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
fn download_mcp_image_candidate_with_platform_fallback(
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
fn download_mcp_image_candidate_with_platform_fallback(
    _url: &str,
    _referer: Option<&str>,
    _policy: &crate::network_policy::OutboundPolicy,
    _timeout: Duration,
) -> Option<serde_json::Value> {
    None
}

#[cfg(windows)]
fn download_image_bytes_with_powershell_httpclient(
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

fn image_response_from_mcp_candidate(
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

fn image_response_from_mcp_candidate_url(
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
fn image_response_from_image_data_url(url: &str) -> Option<serde_json::Value> {
    // Checked before the decode so an absurd string is rejected while there is still only one copy
    // of it: 4 encoded characters per 3 decoded bytes, plus room for the header.
    if url.len() > MAX_MCP_IMAGE_BYTES.saturating_mul(4) / 3 + 4096 {
        return None;
    }
    let (header, payload) = url.split_once(',')?;
    if !header.trim_end().ends_with(";base64") {
        return None;
    }
    let bytes = BASE64.decode(payload.trim()).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_MCP_IMAGE_BYTES {
        return None;
    }
    let mime_type = infer_image_mime_type_from_bytes(&bytes)?;
    Some(image_content_response(&BASE64.encode(&bytes), &mime_type))
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

/// Whether a string is plausibly a base64 image payload that no other signal identified.
///
/// The rule itself lives in `loom_image_io` so that the workflow runtime, which has to answer the
/// same question about the same values, cannot drift from it.
fn looks_like_base64_payload(value: &str) -> bool {
    loom_image_io::looks_like_base64_image_payload(value)
}

/// Find a placeholder that a template declared and substitution did not fill.
///
/// The template's own `{{…}}` tokens are the only ones that count. Looking for `{{` in the rendered
/// text instead would also catch braces that arrived inside an argument's value, which is legitimate
/// content — a caption or a code snippet may well contain them — and used to make the field vanish.
fn unresolved_cloud_template_placeholder<'a>(template: &'a str, rendered: &str) -> Option<&'a str> {
    let mut remainder = template;
    while let Some(start) = remainder.find("{{") {
        let after_start = &remainder[start..];
        let Some(end) = after_start.find("}}") else {
            // An unterminated `{{` is not a placeholder; nothing can substitute it and nothing else in
            // the template can close it either.
            return None;
        };
        let placeholder = &after_start[..end + 2];
        if rendered.contains(placeholder) {
            return Some(placeholder);
        }
        remainder = &after_start[end + 2..];
    }
    None
}

fn substitute_cloud_template(template: &str, arguments: &serde_json::Value) -> String {
    substitute_cloud_template_with(template, arguments, str::to_owned)
}

/// Substitute the cloud template forms with each argument passed through `render`.
///
/// `render` is where the destination's escaping rule lives: a value going into a URL is
/// percent-encoded, a value going into a plain text body or a multipart field is used as written.
fn substitute_cloud_template_with(
    template: &str,
    arguments: &serde_json::Value,
    render: impl Fn(&str) -> String,
) -> String {
    let mut rendered = template.to_owned();
    let Some(arguments) = arguments.as_object() else {
        return rendered;
    };
    for (key, value) in arguments {
        let replacement = render(&scalar_template_value(value));
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}.value}}}}"), &replacement);
        rendered = rendered.replace(&format!("{{{{inputs.{key}.path}}}}"), &replacement);
    }
    rendered
}

/// Percent-encode an argument that is being substituted into an endpoint URL.
///
/// Substitution used to splice the raw value in, so an argument could rewrite the request's
/// authority: an endpoint of `https://api.example.com{{inputs.suffix}}` with a suffix of
/// `@127.0.0.1:8787/` produced a URL whose host was `127.0.0.1` and whose userinfo was
/// `api.example.com`, sending the Art's own credential headers wherever the caller chose. Everything
/// outside the unreserved set is encoded, so a substituted value can no longer end the path, open a
/// query, or introduce userinfo — it can only ever be one path segment or one parameter value.
fn percent_encode_cloud_template_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(*byte));
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// The authority a cloud endpoint declares: the text between `://` and the first `/`, `?`, or `#`.
fn cloud_endpoint_authority(endpoint: &str) -> Option<&str> {
    let after_scheme = endpoint.split_once("://")?.1;
    let end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    Some(&after_scheme[..end])
}

/// Confirm substitution did not move the endpoint to a different host.
///
/// Percent-encoding already prevents an argument from introducing userinfo or a port, and
/// `validate_outbound_url` re-checks the rendered URL against the declared domains. This is the
/// remaining invariant worth stating outright: when the author wrote a fixed authority, the rendered
/// request has to still carry exactly that authority, whatever the arguments contained.
fn validate_rendered_cloud_authority(template: &str, rendered: &str) -> Result<(), String> {
    let Some(declared) = cloud_endpoint_authority(template) else {
        return Ok(());
    };
    if declared.contains("{{") {
        return Ok(());
    }
    let rendered_authority = cloud_endpoint_authority(rendered).unwrap_or_default();
    if rendered_authority == declared {
        return Ok(());
    }
    Err(format!(
        "rendered endpoint authority `{rendered_authority}` does not match the declared authority `{declared}`"
    ))
}

/// Render a JSON-shaped cloud template — the header block, or a JSON request body — by substituting
/// into the parsed document's strings instead of splicing text into the serialized form.
///
/// Splicing let an argument close the string it landed in and add members beside it: a `text`
/// argument of `x","stream":true` turned `{"prompt":"{{inputs.text}}"}` into a two-member object that
/// still parsed, so a caller could set request fields the author never exposed. Substituting after
/// the parse keeps every argument a single string value no matter what punctuation it carries.
///
/// A template that is not itself valid JSON — a placeholder standing in for an unquoted number, say
/// — cannot be parsed before substitution, so it keeps the original splice-then-parse path.
fn render_cloud_json_template(
    tool: &ToolDefinition,
    field: &'static str,
    template: &str,
    arguments: &serde_json::Value,
) -> ToolRegistryResult<serde_json::Value> {
    if let Ok(mut document) = serde_json::from_str::<serde_json::Value>(template) {
        substitute_cloud_json_document(&mut document, arguments);
        return Ok(document);
    }
    let rendered = substitute_cloud_template(template, arguments);
    serde_json::from_str::<serde_json::Value>(&rendered).map_err(|source| {
        ToolRegistryError::CloudTemplate {
            id: tool.id.clone(),
            field,
            reason: source.to_string(),
        }
    })
}

fn substitute_cloud_json_document(document: &mut serde_json::Value, arguments: &serde_json::Value) {
    match document {
        serde_json::Value::String(value) => {
            *value = substitute_cloud_template(value, arguments);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                substitute_cloud_json_document(value, arguments);
            }
        }
        serde_json::Value::Object(entries) => {
            *entries = entries
                .iter()
                .map(|(key, value)| (substitute_cloud_template(key, arguments), value.clone()))
                .collect();
            for (_, value) in entries.iter_mut() {
                substitute_cloud_json_document(value, arguments);
            }
        }
        _ => {}
    }
}

fn header_text_has_control_character(text: &str) -> bool {
    text.chars().any(char::is_control)
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

fn execution_type_name(execution: &ToolExecution) -> &'static str {
    match execution {
        ToolExecution::CloudApi { .. } => "cloud_api",
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
    use std::fs;
    use std::io::{BufRead, Read, Write};
    use std::net::{TcpListener, TcpStream};

    use std::path::PathBuf;
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
    fn surface_manifest_requires_safe_package_local_entries() {
        let mut tool = ToolDefinition::new(
            "stock-price",
            "Stock Price",
            "Interactive stock card",
            ToolExecution::FrameworkArt {
                framework: "framework_art".to_owned(),
            },
        );
        tool.metadata = Some(serde_json::json!({
            "capabilities": {
                "surface": {
                    "protocolVersion": "loom.surface.v1",
                    "apiVersion": "1.0",
                    "variants": [{
                        "runtime": "declarative",
                        "entry": "surface/main.json"
                    }],
                    "fallbackScene": "surface/fallback.json",
                    "requiredNodes": ["column", "text", "button"]
                }
            }
        }));
        assert!(tool.validate().is_ok());

        tool.metadata.as_mut().expect("metadata")["capabilities"]["surface"]["variants"][0]
            ["entry"] = serde_json::json!("../escape.json");
        assert!(matches!(
            tool.validate(),
            Err(ToolRegistryError::InvalidToolDefinition { reason, .. })
                if reason.contains("entry path is unsafe")
        ));
    }

    #[test]
    fn surface_manifest_validates_named_view_full_sizes_and_default() {
        let mut tool = ToolDefinition::new(
            "stock-price",
            "Stock Price",
            "Interactive stock card",
            ToolExecution::FrameworkArt {
                framework: "framework_art".to_owned(),
            },
        );
        tool.metadata = Some(serde_json::json!({
            "capabilities": {
                "surface": {
                    "protocolVersion": "loom.surface.v1",
                    "apiVersion": "1.0",
                    "variants": [{
                        "runtime": "javascript",
                        "entry": "surface/main.js"
                    }],
                    "views": [
                        { "id": "full", "label": "Full", "fullSize": { "width": 960, "height": 820 } },
                        { "id": "price", "label": "Price", "fullSize": { "width": 620, "height": 560 } }
                    ],
                    "defaultViewId": "full"
                }
            }
        }));
        assert!(tool.validate().is_ok());

        tool.metadata.as_mut().expect("metadata")["capabilities"]["surface"]["defaultViewId"] =
            serde_json::json!("missing");
        assert!(matches!(
            tool.validate(),
            Err(ToolRegistryError::InvalidToolDefinition { reason, .. })
                if reason.contains("default view id missing is not declared")
        ));
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
    fn tool_definition_preserves_desktop_port_metadata() {
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "advanced-cli-art",
            "name": "Advanced CLI Art",
            "description": "Desktop Add Art advanced ports",
            "enabled": true,
            "execution": { "type": "framework_art", "framework": "process" },
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
    fn framework_art_execution_type_deserializes_without_host_specific_fields() {
        let value = serde_json::from_value::<ToolDefinition>(serde_json::json!({
            "id": "third-party-art",
            "name": "Third-party Art",
            "description": "External framework Art",
            "enabled": true,
            "execution": {
                "type": "framework_art",
                "framework": "process"
            }
        }));
        assert!(
            value.is_ok(),
            "framework_art execution should deserialize: {value:?}"
        );
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
    fn registry_rehydrates_persisted_art_settings_after_restart() {
        let root = temp_root("persisted-art-settings");
        let registry = ToolRegistry::new(root.join("tools"));
        let mut tool = ToolDefinition::new(
            "image-search",
            "Image Search",
            "Package-backed image search",
            ToolExecution::FrameworkArt {
                framework: "mcp".to_owned(),
            },
        );
        tool.metadata = Some(serde_json::json!({
            "packageSecurity": {
                "publisher": { "id": "publisher.example", "name": "Publisher" }
            }
        }));
        registry.save_tool(tool).expect("save package tool");

        art_settings::ArtSettingsStore::new(&root)
            .save(
                "publisher.example/image-search",
                art_settings::ArtUserSettings {
                    credential_bindings: std::collections::BTreeMap::from([(
                        "api_key".to_owned(),
                        "stored-secret".to_owned(),
                    )]),
                    ..art_settings::ArtUserSettings::default()
                },
            )
            .expect("save Art settings independently of the registry projection");

        let restarted = ToolRegistry::new(root.join("tools"));
        let rehydrated = restarted
            .get_tool("publisher.example/image-search")
            .expect("read restarted registry")
            .expect("rehydrated tool");
        assert_eq!(
            rehydrated
                .metadata
                .as_ref()
                .and_then(|metadata| {
                    metadata.pointer("/artUserSettings/credentialBindings/api_key")
                })
                .and_then(serde_json::Value::as_str),
            Some("stored-secret")
        );

        fs::remove_dir_all(root).expect("cleanup persisted Art settings root");
    }

    #[test]
    fn a_damaged_art_settings_file_does_not_hide_every_art() {
        let root = temp_root("damaged-art-settings");
        let registry = ToolRegistry::new(root.join("tools"));
        let mut tool = ToolDefinition::new(
            "image-search",
            "Image Search",
            "Package-backed image search",
            ToolExecution::FrameworkArt {
                framework: "mcp".to_owned(),
            },
        );
        tool.metadata = Some(serde_json::json!({
            "packageSecurity": {
                "publisher": { "id": "publisher.example", "name": "Publisher" }
            }
        }));
        registry.save_tool(tool).expect("save package tool");

        let settings_path = root.join("art-user-settings.json");
        fs::write(&settings_path, b"{\"arts\":{\"publisher.example/image-s")
            .expect("truncate the Art settings file");

        // Before this fix the truncated preferences file propagated its parse error out of
        // `read_tools`, so every registry operation failed and the Art list came back empty.
        let restarted = ToolRegistry::new(root.join("tools"));
        let tools = restarted
            .list_tools()
            .expect("list tools past damaged settings");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].qualified_id(), "publisher.example/image-search");
        assert!(restarted
            .get_tool("publisher.example/image-search")
            .expect("get tool past damaged settings")
            .is_some());
        assert!(
            fs::read_dir(&root)
                .expect("read control plane root")
                .filter_map(|entry| entry.ok())
                .any(|entry| entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("art-user-settings.json.corrupt-"))),
            "the damaged settings file should be copied aside before it is reset"
        );

        fs::remove_dir_all(root).expect("cleanup damaged Art settings root");
    }

    #[test]
    fn registry_removes_stale_art_settings_without_a_persisted_entry() {
        let root = temp_root("stale-art-settings");
        let tools_root = root.join("tools");
        fs::create_dir_all(&tools_root).expect("create canonical registry root");
        let mut tool = ToolDefinition::new(
            "image-search",
            "Image Search",
            "Package-backed image search",
            ToolExecution::FrameworkArt {
                framework: "mcp".to_owned(),
            },
        );
        tool.metadata = Some(serde_json::json!({
            "packageSecurity": {
                "publisher": { "id": "publisher.example", "name": "Publisher" }
            },
            "artUserSettings": {
                "credentialBindings": { "api_key": "stale-secret" }
            }
        }));
        fs::write(
            tools_root.join(TOOLS_FILE),
            serde_json::to_vec_pretty(&vec![tool]).expect("serialize stale registry"),
        )
        .expect("write stale registry");

        let restarted = ToolRegistry::new(&tools_root);
        let sanitized = restarted
            .get_tool("publisher.example/image-search")
            .expect("read restarted registry")
            .expect("sanitized tool");
        assert!(sanitized
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("artUserSettings"))
            .is_none());
        assert!(sanitized
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("packageSecurity"))
            .is_some());

        fs::remove_dir_all(root).expect("cleanup stale Art settings root");
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
                ToolExecution::FrameworkArt {
                    framework: "process".to_owned(),
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
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
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
    fn registry_does_not_remove_unknown_future_execution_entries() {
        let root = temp_root("future-execution");
        fs::create_dir_all(&root).expect("create registry root");
        let original = serde_json::to_string_pretty(&serde_json::json!([{
            "id": "future-art",
            "name": "Future Art",
            "description": "unknown future execution",
            "enabled": true,
            "execution": { "type": "future_runtime" }
        }]))
        .expect("serialize future tool");
        let registry_path = root.join("tools.json");
        fs::write(&registry_path, &original).expect("write future registry");

        let registry = ToolRegistry::new(&root);
        assert!(matches!(
            registry.list_tools(),
            Err(ToolRegistryError::Json(_))
        ));
        assert_eq!(
            fs::read_to_string(&registry_path).expect("read unchanged registry"),
            original
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

        fs::remove_dir_all(root).expect("cleanup future registry root");
    }

    #[test]
    fn registry_does_not_recover_comma_only_trailing_json() {
        let root = temp_root("trailing-commas");
        fs::create_dir_all(&root).expect("create registry root");
        let tool = ToolDefinition::new(
            "preserved-tool",
            "Preserved Tool",
            "Tool in an unrecoverable registry",
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
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
    fn repeated_mcp_calls_reuse_the_initialized_session() {
        let tool = ToolDefinition::new(
            "fixture-counter",
            "Fixture Counter",
            "Count calls in one MCP server process",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "counter".to_owned(),
            },
        );
        let server = current_test_binary_fixture_config();

        let first = execute_tool(&tool, std::slice::from_ref(&server), serde_json::json!({}))
            .expect("first pooled MCP call");
        let second =
            execute_tool(&tool, &[server], serde_json::json!({})).expect("second pooled MCP call");

        assert_eq!(first["content"][0]["text"], "1");
        assert_eq!(second["content"][0]["text"], "2");
        clear_cached_mcp_sessions_for_current_thread();
    }

    #[test]
    fn a_cancelled_mcp_run_stops_before_the_server_is_started() {
        let tool = ToolDefinition::new(
            "fixture-echo",
            "Fixture Echo",
            "Echo through fixture MCP",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "echo".to_owned(),
            },
        );
        // A command that cannot be spawned: reaching the connect step at all would fail with an MCP
        // error rather than a cancellation, so the assertion below proves the run stopped before it.
        let server = loom_mcp::McpServerConfig::new(
            "fixture",
            "Fixture MCP",
            "loom-nonexistent-mcp-server-binary",
        );
        let cancellation = AtomicBool::new(true);

        let error = execute_tool_with_timeout_and_cancellation(
            &tool,
            &[server],
            serde_json::json!({ "text": "hello registry" }),
            Duration::from_secs(5),
            &cancellation,
        )
        .expect_err("a cancelled run does not execute");

        assert!(
            matches!(error, ToolRegistryError::ExecutionCancelled { ref id } if id == "fixture-echo"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn an_in_flight_mcp_round_trip_is_cancelled() {
        let tool = ToolDefinition::new(
            "fixture-echo-cancel",
            "Fixture Echo Cancel",
            "Cancel a hung MCP round trip",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "echo".to_owned(),
            },
        );
        let server =
            current_test_binary_fixture_config().env("LOOM_TOOL_REGISTRY_MCP_FIXTURE_MODE", "hang");
        let cancellation = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&cancellation);
        let trigger_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            trigger.store(true, Ordering::Release);
        });

        let started = Instant::now();
        let error = execute_tool_with_timeout_and_cancellation(
            &tool,
            &[server],
            serde_json::json!({ "text": "never returned" }),
            Duration::from_secs(5),
            cancellation.as_ref(),
        )
        .expect_err("the hung MCP round trip must be cancelled");
        let elapsed = started.elapsed();

        trigger_thread
            .join()
            .expect("join MCP cancellation trigger");
        assert!(matches!(
            error,
            ToolRegistryError::ExecutionCancelled { ref id } if id == "fixture-echo-cancel"
        ));
        assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
    }

    #[test]
    fn an_uncancelled_mcp_run_still_reaches_the_server() {
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
        let cancellation = AtomicBool::new(false);

        let result = execute_tool_with_timeout_and_cancellation(
            &tool,
            &[server],
            serde_json::json!({ "text": "hello registry" }),
            Duration::from_secs(30),
            &cancellation,
        )
        .expect("an uncancelled run executes normally");

        assert_eq!(result["content"][0]["text"], "hello registry");
    }

    #[test]
    fn data_url_candidate_is_decoded_and_identified_from_its_bytes() {
        let response = image_response_from_image_data_url(CLOUD_FIXTURE_IMAGE)
            .expect("data URL candidate resolves");

        assert_eq!(response["content"][0]["type"], "image");
        assert_eq!(response["content"][0]["mimeType"], "image/png");
        assert_eq!(
            response["content"][0]["data"],
            format!(
                "data:image/png;base64,{}",
                BASE64.encode(fixture_image_bytes())
            )
        );
    }

    #[test]
    fn data_url_candidate_mime_type_comes_from_the_bytes_not_the_url() {
        let mislabelled = format!(
            "data:image/webp;base64,{}",
            BASE64.encode(fixture_image_bytes())
        );

        let response = image_response_from_image_data_url(&mislabelled)
            .expect("mislabelled data URL resolves");

        assert_eq!(response["content"][0]["mimeType"], "image/png");
    }

    #[test]
    fn malformed_or_non_raster_data_url_candidates_are_rejected() {
        let svg = format!(
            "data:image/svg+xml;base64,{}",
            BASE64.encode(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>")
        );

        for value in [
            "data:image/png;base64,not valid base64",
            "data:image/png;base64,",
            "data:image/png,%3Csvg%3E",
            svg.as_str(),
        ] {
            assert!(
                image_response_from_image_data_url(value).is_none(),
                "`{value}` should not resolve as an image"
            );
        }
    }

    #[test]
    fn svg_urls_and_mime_types_are_not_accepted_as_images() {
        assert!(!looks_like_image_url("https://host/logo.svg"));
        assert!(looks_like_image_url("https://host/logo.png"));
        assert!(infer_image_mime_type_from_url("https://host/logo.svg").is_none());
        assert!(!is_supported_image_mime_type("image/svg+xml"));
        assert!(is_supported_image_mime_type("IMAGE/PNG"));
    }

    #[test]
    fn short_borrowed_error_text_is_kept_whole() {
        assert_eq!(
            bounded_error_text("  quota exceeded for this key  "),
            "quota exceeded for this key"
        );
        assert_eq!(bounded_error_text(""), "");
        let exact = "e".repeat(MAX_BORROWED_ERROR_TEXT_BYTES);
        assert_eq!(bounded_error_text(&exact), exact);
    }

    #[test]
    fn long_borrowed_error_text_keeps_its_head_and_says_what_it_dropped() {
        let body = format!("failure: {}", "x".repeat(64 * 1024));

        let bounded = bounded_error_text(&body);

        assert!(bounded.starts_with("failure: xxx"));
        assert!(bounded.contains(&format!(
            "[{} more bytes omitted]",
            body.len() - MAX_BORROWED_ERROR_TEXT_BYTES
        )));
        assert!(bounded.len() < body.len());
    }

    #[test]
    fn borrowed_error_text_is_cut_on_a_character_boundary() {
        // A multi-byte character straddling the bound would panic a naive slice, and the text that
        // reaches here — an API error message, a runtime's stderr — is regularly not ASCII.
        let body = "配额已用尽".repeat(4096);

        let bounded = bounded_error_text(&body);

        assert!(bounded.starts_with("配额已用尽"));
        assert!(bounded.contains("more bytes omitted"));
        assert!(bounded.len() <= MAX_BORROWED_ERROR_TEXT_BYTES + 64);
    }

    #[test]
    fn a_failed_call_after_a_successful_listing_reports_only_itself() {
        let error = mcp_call_error(
            loom_mcp::McpError::Protocol("tool rejected input".into()),
            None,
        );

        let message = error.to_string();
        assert!(message.contains("tool rejected input"));
        assert!(!message.contains("tool listing failed"));
    }

    #[test]
    fn a_failed_call_after_a_failed_listing_reports_both() {
        let error = mcp_call_error(
            loom_mcp::McpError::Protocol("unknown argument `query`".into()),
            Some("MCP request timed out after 5000ms; stderr: "),
        );

        let message = error.to_string();
        assert!(message.contains("unknown argument `query`"));
        assert!(message.contains("tool listing failed first"));
        assert!(message.contains("timed out after 5000ms"));
    }

    #[test]
    fn a_folded_listing_failure_stays_bounded() {
        let error = mcp_call_error(
            loom_mcp::McpError::Protocol("x".repeat(64 * 1024)),
            Some(&"y".repeat(64 * 1024)),
        );

        let message = error.to_string();
        assert!(message.contains("more bytes omitted"));
        assert!(message.len() < 8 * 1024);
    }

    #[test]
    fn cloud_json_data_string_stays_text_without_an_image_signal() {
        let response = normalize_cloud_json_value(serde_json::json!({ "data": "completed" }));

        assert_eq!(response["content"][0]["type"], "text");
        assert!(response["content"][0]["text"]
            .as_str()
            .expect("text content")
            .contains("completed"));
    }

    #[test]
    fn cloud_json_nested_output_data_stays_text_without_an_image_signal() {
        let response = normalize_cloud_json_value(
            serde_json::json!({ "output": { "data": "req_01HX9ZQK7T2M4V8N" } }),
        );

        assert_eq!(response["content"][0]["type"], "text");
    }

    #[test]
    fn cloud_json_data_string_is_an_image_when_the_response_labels_it() {
        let response = normalize_cloud_json_value(
            serde_json::json!({ "data": "aGVsbG8=", "mime_type": "image/jpeg" }),
        );

        assert_eq!(response["content"][0]["type"], "image");
        assert_eq!(response["content"][0]["mimeType"], "image/jpeg");
        assert_eq!(
            response["content"][0]["data"],
            "data:image/jpeg;base64,aGVsbG8="
        );
    }

    #[test]
    fn cloud_json_data_url_is_an_image_whatever_its_length() {
        let response =
            normalize_cloud_json_value(serde_json::json!({ "data": CLOUD_FIXTURE_IMAGE }));

        assert_eq!(response["content"][0]["type"], "image");
        assert_eq!(response["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn mcp_text_result_is_not_an_image_just_because_it_is_alphanumeric() {
        let value = serde_json::json!({
            "content": [{ "type": "text", "text": "completed" }]
        });

        assert!(!mcp_result_already_contains_image(&value));
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
        tool.metadata = Some(loopback_cloud_metadata());
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
        tool.metadata = Some(loopback_cloud_metadata());
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
        assert_eq!(result["loomMetadata"]["candidates"]["selectedIndex"], 1);
        assert_eq!(result["loomMetadata"]["candidates"]["items"][0]["index"], 0);
        assert_eq!(result["loomMetadata"]["candidates"]["items"][1]["index"], 1);
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

        let result = normalize_mcp_image_result(
            &serde_json::json!({ "result_index": 0 }),
            &value,
            &loopback_mcp_image_policy(),
        )
        .expect("fallback to another candidate image");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE_ALT);
        assert_eq!(result["loomMetadata"]["candidates"]["selectedIndex"], 1);
        assert_eq!(result["loomMetadata"]["candidates"]["items"][0]["index"], 0);
        assert_eq!(result["loomMetadata"]["candidates"]["items"][1]["index"], 1);
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
        assert_eq!(result["loomMetadata"]["candidates"]["selectedIndex"], 0);
        assert_eq!(
            result["loomMetadata"]["candidates"]["items"][0]["imageUrl"],
            "http://127.0.0.1:9/broken.jpg"
        );
    }

    #[test]
    fn an_in_range_candidate_request_is_reported_without_a_note() {
        let candidates = vec![
            McpImageCandidate {
                image_url: "https://example.com/a.png".to_owned(),
                ..McpImageCandidate::default()
            },
            McpImageCandidate {
                image_url: "https://example.com/b.png".to_owned(),
                ..McpImageCandidate::default()
            },
        ];
        let selection =
            selected_mcp_image_candidate_index(&serde_json::json!({ "result_index": 1 }), 2);
        let mut result = serde_json::json!({ "content": [] });

        attach_mcp_image_candidate_metadata(&mut result, &candidates, &selection, selection.index);

        let metadata = &result["loomMetadata"]["candidates"];
        assert_eq!(metadata["selectedIndex"], 1);
        assert!(metadata.get("requestedIndex").is_none());
        assert!(metadata.get("selectionNote").is_none());
    }

    #[test]
    fn an_out_of_range_candidate_request_reports_the_clamp_it_used() {
        let candidates = vec![
            McpImageCandidate {
                image_url: "https://example.com/a.png".to_owned(),
                ..McpImageCandidate::default()
            },
            McpImageCandidate {
                image_url: "https://example.com/b.png".to_owned(),
                ..McpImageCandidate::default()
            },
        ];
        let selection =
            selected_mcp_image_candidate_index(&serde_json::json!({ "result_index": 7 }), 2);
        assert_eq!(selection.requested, Some(7));
        assert_eq!(selection.index, 1);
        let mut result = serde_json::json!({ "content": [] });

        attach_mcp_image_candidate_metadata(&mut result, &candidates, &selection, selection.index);

        let metadata = &result["loomMetadata"]["candidates"];
        assert_eq!(metadata["selectedIndex"], 1);
        assert_eq!(metadata["requestedIndex"], 7);
        assert_eq!(
            metadata["selectionNote"],
            "requested index 7 is past the last of 2 candidates, so candidate 1 was used instead"
        );
    }

    #[test]
    fn a_download_fallback_reports_the_candidate_it_could_not_use() {
        // The requested candidate existed but failed to download, so the response carries a different
        // image than the one asked for. Without the note the canvas cannot tell why.
        assert_eq!(
            mcp_image_selection_note(1, 1, 0, 3).expect("a fallback is worth reporting"),
            "candidate 1 could not be downloaded, so candidate 0 was used instead"
        );

        // Both causes at once: past the end of the list *and* the clamped candidate would not download.
        let note = mcp_image_selection_note(7, 2, 0, 3).expect("both causes are worth reporting");
        assert!(note.contains("requested index 7 is past the last of 3 candidates"));
        assert!(note.contains("candidate 2 could not be downloaded"));

        // Nothing moved the choice, so there is nothing to explain.
        assert!(mcp_image_selection_note(1, 1, 1, 3).is_none());
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

        let result = normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &loopback_mcp_image_policy(),
        )
        .expect("fallback to thumbnail image");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
        assert_eq!(
            result["loomMetadata"]["candidates"]["items"]
                .as_array()
                .expect("candidate metadata")
                .len(),
            1
        );
        assert_eq!(
            result["loomMetadata"]["candidates"]["items"][0]["thumbnailUrl"],
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

        let result = normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &loopback_mcp_image_policy(),
        )
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

        let result = normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &loopback_mcp_image_policy(),
        )
        .expect("normalize stringified image-search items");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
        assert_eq!(
            result["loomMetadata"]["candidates"]["items"][0]["imageUrl"],
            image_url
        );
    }

    #[test]
    fn mcp_image_candidates_stop_at_the_nesting_limit() {
        // A candidate that sits within the budget is still found.
        let shallow = serde_json::json!({
            "structuredContent": {"items": [{"url": "https://example.invalid/a.png"}]}
        });
        assert_eq!(collect_mcp_image_candidates(&shallow).len(), 1);

        // Past the budget the walk stops instead of following the value down.
        let mut deep = serde_json::json!({"url": "https://example.invalid/a.png"});
        for _ in 0..(MAX_MCP_IMAGE_CANDIDATE_DEPTH + 4) {
            deep = serde_json::json!({"nested": deep});
        }
        let value = serde_json::json!({"structuredContent": deep});
        assert!(collect_mcp_image_candidates(&value).is_empty());
    }

    #[test]
    fn mcp_image_candidates_bound_chained_stringified_payloads() {
        // Every hop is a shallow document on its own, so only a counter that survives the
        // re-parse keeps this from walking as deep as the attacker cares to nest. Reaching this
        // assertion at all is the point: the previous walk aborted the process here.
        //
        // Each hop costs two levels of budget (the object, then the string re-parsed inside it)
        // and roughly doubles the encoded size, so fourteen hops is both comfortably past the
        // limit of MAX_MCP_IMAGE_CANDIDATE_DEPTH and small enough to build in a test.
        fn chained(hops: usize) -> serde_json::Value {
            let mut text = r#"{"url":"https://example.invalid/a.png"}"#.to_owned();
            for _ in 0..hops {
                text = format!(
                    r#"{{"items":{}}}"#,
                    serde_json::to_string(&text).expect("encode hop")
                );
            }
            serde_json::json!({"content": [{"type": "text", "text": text}]})
        }

        // A short chain is a real payload shape and still resolves.
        assert_eq!(collect_mcp_image_candidates(&chained(2)).len(), 1);
        assert!(collect_mcp_image_candidates(&chained(14)).is_empty());
    }

    #[test]
    fn mcp_image_candidates_are_capped() {
        let items = (0..(MAX_MCP_IMAGE_CANDIDATES * 4))
            .map(|index| serde_json::json!({"url": format!("https://example.invalid/{index}.png")}))
            .collect::<Vec<_>>();
        let value = serde_json::json!({"structuredContent": {"items": items}});
        assert_eq!(
            collect_mcp_image_candidates(&value).len(),
            MAX_MCP_IMAGE_CANDIDATES
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

        let result = normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &loopback_mcp_image_policy(),
        )
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

        let result = normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &loopback_mcp_image_policy(),
        )
        .expect("normalize image-search url with broken modifiers");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
        assert_eq!(
            result["loomMetadata"]["candidates"]["items"][0]["imageUrl"],
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

        let result = normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &loopback_mcp_image_policy(),
        )
        .expect("normalize image-search url with trailing path modifiers");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
        assert_eq!(
            result["loomMetadata"]["candidates"]["items"][0]["imageUrl"],
            image_url
        );
    }

    #[test]
    fn a_rewritten_candidate_url_keeps_the_string_it_came_from() {
        let candidate_from = |url: &str| {
            let value = serde_json::json!({ "url": url });
            image_candidate_from_object(value.as_object().expect("candidate object"))
                .expect("candidate from url")
        };

        let modifier = candidate_from("https://host/a.jpg!600x400");
        assert_eq!(modifier.image_url, "https://host/a.jpg");
        assert_eq!(
            modifier.alternate_image_url.as_deref(),
            Some("https://host/a.jpg!600x400")
        );

        let nested = candidate_from("https://host/logo.png/v2/actual");
        assert_eq!(nested.image_url, "https://host/logo.png");
        assert_eq!(
            nested.alternate_image_url.as_deref(),
            Some("https://host/logo.png/v2/actual")
        );
        // The string a rewritten URL came from is a download fallback, not the page the image sits on.
        assert!(nested.source_page_url.is_none());
    }

    #[test]
    fn normalize_mcp_image_search_falls_back_to_the_unstripped_candidate_url() {
        // The rewrite cuts this path at `logo.png`, which the fixture does not serve; only retrying the
        // URL the server actually sent can reach the image.
        let image_fixture = RetryingExactPathHttpImageFixture::start(
            "image/png",
            fixture_image_bytes(),
            "/logo.png/v2/actual",
        );
        let image_url = image_fixture.url("/logo.png/v2/actual");
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Nested path fixture image",
                        "url": "https://example.invalid/page",
                        "properties": {
                            "url": image_url,
                            "width": 1,
                            "height": 1
                        }
                    }
                ],
                "count": 1
            }
        });

        let result = normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &loopback_mcp_image_policy(),
        )
        .expect("normalize image-search url whose rewrite cuts a real path");

        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        assert_eq!(result["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
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

        let (mime_type, bytes) = download_image_bytes_with_powershell_httpclient(
            &fixture.url("/thumb"),
            None,
            &loopback_mcp_image_policy(),
            CLOUD_API_TIMEOUT,
        )
        .expect("download image bytes via powershell fallback with image accept header");

        assert_eq!(mime_type, "image/png");
        assert_eq!(bytes, fixture_image_bytes());
    }

    #[cfg(windows)]
    #[test]
    fn powershell_httpclient_fallback_downloads_image_candidate_bytes() {
        let fixture = HttpImageFixture::start("application/octet-stream", fixture_image_bytes());
        let (mime_type, bytes) = download_image_bytes_with_powershell_httpclient(
            &fixture.url("/thumb"),
            None,
            &loopback_mcp_image_policy(),
            CLOUD_API_TIMEOUT,
        )
        .expect("download image bytes via powershell fallback");

        assert_eq!(mime_type, "image/png");
        assert_eq!(bytes, fixture_image_bytes());
    }

    /// A cloud Art no longer reaches loopback unless it declares that it wants to, so every
    /// fixture-backed test has to declare it the way a real local-service Art would. The same
    /// declaration now governs an MCP image-search tool's image downloads.
    fn loopback_cloud_metadata() -> serde_json::Value {
        serde_json::json!({
            "permissionPolicy": { "network": { "allowLocalhost": true } }
        })
    }

    /// The download policy an MCP image-search tool gets once it declares `allowLocalhost`, for the
    /// tests that call the download helpers directly against a loopback fixture.
    fn loopback_mcp_image_policy() -> crate::network_policy::OutboundPolicy {
        crate::network_policy::OutboundPolicy {
            allow_http_loopback: true,
            ..crate::network_policy::OutboundPolicy::default()
        }
    }

    #[test]
    fn an_mcp_image_candidate_is_not_downloaded_from_loopback_by_default() {
        let image_fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let image_url = image_fixture.url("/fixture.png");
        let value = serde_json::json!({
            "structuredContent": {
                "type": "object",
                "items": [
                    {
                        "title": "Loopback image",
                        "url": "https://example.invalid/page",
                        "properties": {
                            "url": image_url,
                            "width": 1,
                            "height": 1
                        }
                    }
                ],
                "count": 1
            }
        });

        // The URL is served and would download fine under the old hardcoded loopback allowance.
        // The candidate host is chosen entirely by the MCP server, so an undeclared tool has to be
        // refused before the request goes out.
        assert!(normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &crate::network_policy::OutboundPolicy::default(),
        )
        .is_none());

        let downloaded = normalize_mcp_image_result(
            &serde_json::json!({}),
            &value,
            &loopback_mcp_image_policy(),
        )
        .expect("download the loopback candidate once loopback is declared");
        assert_eq!(downloaded["content"][0]["type"], "image");
        assert_eq!(downloaded["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn an_mcp_image_download_policy_comes_from_the_tool_declaration() {
        let mut tool = ToolDefinition::new(
            "fixture-image-search-policy",
            "Fixture Image Search Policy",
            "Report the derived image download policy",
            ToolExecution::Mcp {
                server_id: "fixture".to_owned(),
                tool_name: "brave_image_search".to_owned(),
            },
        );

        let undeclared = mcp_image_download_policy(&tool);
        assert!(!undeclared.allow_http_loopback);
        assert!(!undeclared.allow_private_networks);

        tool.metadata = Some(serde_json::json!({
            "permissionPolicy": {
                "network": {
                    "allowLocalhost": true,
                    "allowPrivateNetworks": true,
                    "domains": ["api.search.brave.com"]
                }
            }
        }));
        let declared = mcp_image_download_policy(&tool);
        assert!(declared.allow_http_loopback);
        assert!(declared.allow_private_networks);
        // The declared domains name the search API, not the image hosts the results point at, so
        // they deliberately do not constrain the image download.
        assert!(declared.allowed_domains.is_empty());
    }

    /// A candidate whose URL cannot become a request at all, so the download loop rejects it without
    /// spending any of its network budget. The cap and deadline tests need many failing candidates
    /// and cannot afford a real connection attempt for each one.
    fn unfetchable_mcp_image_candidate(index: usize) -> McpImageCandidate {
        McpImageCandidate {
            image_url: format!("not-a-url-{index}"),
            alternate_image_url: None,
            title: None,
            thumbnail_url: None,
            source_page_url: None,
            width: None,
            height: None,
        }
    }

    fn fixture_mcp_image_candidate(image_url: String) -> McpImageCandidate {
        McpImageCandidate {
            image_url,
            alternate_image_url: None,
            title: Some("Fixture image".to_owned()),
            thumbnail_url: None,
            source_page_url: None,
            width: Some(1),
            height: Some(1),
        }
    }

    #[test]
    fn mcp_image_candidate_downloads_stop_at_the_attempt_cap() {
        let within_cap = HttpImageFixture::start("image/png", fixture_image_bytes());
        let mut candidates: Vec<McpImageCandidate> = (0..MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS - 1)
            .map(unfetchable_mcp_image_candidate)
            .collect();
        candidates.push(fixture_mcp_image_candidate(within_cap.url("/fixture.png")));

        let (_, selected) = image_response_from_mcp_candidates(
            &candidates,
            0,
            &loopback_mcp_image_policy(),
            McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET),
        )
        .expect("the last candidate inside the attempt cap is still tried");
        assert_eq!(selected, MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS - 1);

        // One candidate further out is never requested: the list length is chosen by the MCP server,
        // and a result full of unfetchable candidates must end rather than walk all of them.
        let past_cap = HttpImageFixture::start("image/png", fixture_image_bytes());
        let mut candidates: Vec<McpImageCandidate> = (0..MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS)
            .map(unfetchable_mcp_image_candidate)
            .collect();
        candidates.push(fixture_mcp_image_candidate(past_cap.url("/fixture.png")));

        assert!(image_response_from_mcp_candidates(
            &candidates,
            0,
            &loopback_mcp_image_policy(),
            McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET),
        )
        .is_none());
    }

    #[test]
    fn an_mcp_image_download_deadline_bounds_each_attempt() {
        let exhausted = McpImageDownloadDeadline::starting_now(Duration::ZERO);
        assert!(exhausted.next_attempt_timeout().is_none());

        // A fresh budget is larger than one request's own timeout, so the first attempt is bounded by
        // the request timeout rather than by the loop budget.
        let fresh = McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET);
        assert_eq!(fresh.next_attempt_timeout(), Some(CLOUD_API_TIMEOUT));

        // Once less than one request timeout is left, the attempt gets only what remains.
        let nearly_spent =
            McpImageDownloadDeadline::starting_now(MIN_MCP_IMAGE_ATTEMPT_TIMEOUT * 2);
        let remaining = nearly_spent
            .next_attempt_timeout()
            .expect("a budget above the attempt minimum still allows one more request");
        assert!(remaining >= MIN_MCP_IMAGE_ATTEMPT_TIMEOUT);
        assert!(remaining <= MIN_MCP_IMAGE_ATTEMPT_TIMEOUT * 2);
    }

    #[test]
    fn an_exhausted_mcp_image_budget_stops_before_the_next_request() {
        let fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
        let candidates = vec![fixture_mcp_image_candidate(fixture.url("/fixture.png"))];

        assert!(image_response_from_mcp_candidates(
            &candidates,
            0,
            &loopback_mcp_image_policy(),
            McpImageDownloadDeadline::starting_now(Duration::ZERO),
        )
        .is_none());

        // The same candidate downloads once there is budget for it, so the refusal above is the
        // deadline and not an unreachable fixture.
        let (response, selected) = image_response_from_mcp_candidates(
            &candidates,
            0,
            &loopback_mcp_image_policy(),
            McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET),
        )
        .expect("download the candidate while budget is left");
        assert_eq!(selected, 0);
        assert_eq!(response["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
    }

    #[test]
    fn execute_cloud_api_tool_posts_json_arguments_to_fixture() {
        let fixture = CloudFixture::start(CloudFixtureMode::Text);
        let mut tool = ToolDefinition::new(
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
        tool.metadata = Some(loopback_cloud_metadata());

        let result = execute_tool(&tool, &[], serde_json::json!({ "prompt": "hello cloud" }))
            .expect("execute cloud API tool");

        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "cloud saw hello cloud");
    }

    #[test]
    fn a_cancelled_cloud_run_sends_no_request() {
        let fixture = CloudFixture::start(CloudFixtureMode::Text);
        let mut tool = ToolDefinition::new(
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
        tool.metadata = Some(loopback_cloud_metadata());
        let cancellation = AtomicBool::new(true);

        let error = execute_tool_with_timeout_and_cancellation(
            &tool,
            &[],
            serde_json::json!({ "prompt": "hello cloud" }),
            Duration::from_secs(5),
            &cancellation,
        )
        .expect_err("a cancelled run does not execute");

        assert!(
            matches!(error, ToolRegistryError::ExecutionCancelled { ref id } if id == "fixture-cloud"),
            "unexpected error: {error}"
        );
        assert!(
            fixture
                .captured_request
                .lock()
                .expect("lock cloud request capture")
                .is_none(),
            "the fixture received a request from a cancelled run"
        );
    }

    #[test]
    fn a_cloud_run_cancels_while_waiting_for_response_headers() {
        assert_delayed_cloud_run_is_cancellable(CloudFixtureMode::DelayedHeaders);
    }

    #[test]
    fn a_cloud_run_cancels_while_waiting_for_response_body() {
        assert_delayed_cloud_run_is_cancellable(CloudFixtureMode::DelayedBody);
    }

    fn assert_delayed_cloud_run_is_cancellable(mode: CloudFixtureMode) {
        let fixture = CloudFixture::start(mode);
        let mut tool = ToolDefinition::new(
            "fixture-cloud-cancel",
            "Fixture Cloud Cancel",
            "Cancel a delayed cloud API request",
            ToolExecution::CloudApi {
                endpoint: fixture.url("/delayed"),
                method: "POST".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );
        tool.metadata = Some(loopback_cloud_metadata());
        let cancellation = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&cancellation);
        let trigger_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            trigger.store(true, Ordering::Release);
        });

        let started = Instant::now();
        let error = execute_tool_with_timeout_and_cancellation(
            &tool,
            &[],
            serde_json::json!({ "prompt": "cancel me" }),
            Duration::from_secs(5),
            cancellation.as_ref(),
        )
        .expect_err("the delayed cloud request must be cancelled");
        let elapsed = started.elapsed();

        trigger_thread
            .join()
            .expect("join cloud cancellation trigger");
        assert!(matches!(
            error,
            ToolRegistryError::ExecutionCancelled { ref id } if id == "fixture-cloud-cancel"
        ));
        assert!(elapsed < Duration::from_secs(1), "cancel took {elapsed:?}");
    }

    #[test]
    fn image_response_accumulator_streams_base64_across_chunk_boundaries() {
        let raw = (0_u8..=250)
            .cycle()
            .take(4 * 1024 * 1024)
            .collect::<Vec<_>>();
        let mut accumulator = CloudBodyAccumulator::new(Some("image/png"), Some(raw.len() as u64));
        for chunk in raw.chunks(65_537) {
            accumulator.push(chunk);
        }

        let CloudResponseBody::ImageDataUrl(data_url) = accumulator
            .finish()
            .expect("finish streamed image response")
        else {
            panic!("image accumulator returned text");
        };
        assert_eq!(
            data_url,
            format!("data:image/png;base64,{}", BASE64.encode(&raw))
        );
    }

    #[test]
    fn maximum_text_response_reuses_its_single_byte_allocation() {
        let mut accumulator =
            CloudBodyAccumulator::new(None, Some(MAX_CLOUD_RESPONSE_BYTES as u64));
        let chunk = [b'x'; 64 * 1024];
        for _ in 0..(MAX_CLOUD_RESPONSE_BYTES / chunk.len()) {
            accumulator.push(&chunk);
        }
        let allocation = match &accumulator {
            CloudBodyAccumulator::Text(bytes) => bytes.as_ptr(),
            CloudBodyAccumulator::Image { .. } => panic!("text accumulator returned image"),
        };

        let CloudResponseBody::Text(text) = accumulator
            .finish()
            .expect("finish maximum valid UTF-8 response")
        else {
            panic!("text accumulator returned image");
        };
        assert_eq!(text.len(), MAX_CLOUD_RESPONSE_BYTES);
        assert_eq!(
            text.as_ptr(),
            allocation,
            "UTF-8 conversion allocated a copy"
        );
    }

    #[test]
    fn invalid_utf8_text_is_rejected_without_a_lossy_full_size_copy() {
        let mut accumulator = CloudBodyAccumulator::new(None, Some(3));
        accumulator.push(&[b'a', 0xff, b'b']);

        assert!(matches!(
            accumulator.finish(),
            Err(CloudTransportError::InvalidUtf8)
        ));
    }

    #[test]
    fn execute_cloud_api_tool_normalizes_image_json_response() {
        let fixture = CloudFixture::start(CloudFixtureMode::Image);
        let mut tool = ToolDefinition::new(
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
        tool.metadata = Some(loopback_cloud_metadata());

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
        let mut tool = ToolDefinition::new(
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
        tool.metadata = Some(loopback_cloud_metadata());

        let error = execute_tool(&tool, &[], serde_json::json!({}))
            .expect_err("cloud API HTTP error fails");

        assert!(error.to_string().contains("cloud API"));
    }

    #[test]
    fn a_cloud_art_without_a_declared_network_policy_cannot_call_loopback() {
        let fixture = CloudFixture::start(CloudFixtureMode::Text);
        let tool = ToolDefinition::new(
            "fixture-cloud-undeclared",
            "Fixture Cloud Undeclared",
            "Call a loopback endpoint without declaring it",
            ToolExecution::CloudApi {
                endpoint: fixture.url("/text"),
                method: "POST".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );

        let error = execute_tool(&tool, &[], serde_json::json!({ "prompt": "hello cloud" }))
            .expect_err("undeclared loopback is refused");

        let message = error.to_string();
        assert!(
            message.contains("loopback") || message.contains("HTTP is only allowed"),
            "unexpected refusal reason: {message}"
        );
    }

    #[test]
    fn a_cloud_art_deadline_can_be_raised_by_the_caller_and_by_the_package() {
        let mut tool = ToolDefinition::new(
            "fixture-cloud-timeout",
            "Fixture Cloud Timeout",
            "Deadline resolution only",
            ToolExecution::CloudApi {
                endpoint: "https://api.example.com/run".to_owned(),
                method: "POST".to_owned(),
                content_type: None,
                headers: None,
                body: None,
            },
        );

        // Nothing declared, nothing requested: the default.
        assert_eq!(cloud_api_timeout(&tool, None), CLOUD_API_TIMEOUT);
        // A caller's deadline is honoured rather than clamped down to the default.
        assert_eq!(
            cloud_api_timeout(&tool, Some(Duration::from_secs(120))),
            Duration::from_secs(120)
        );
        // A package declaration applies when the caller states nothing.
        tool.metadata = Some(serde_json::json!({ "cloudApi": { "timeoutMs": 90_000 } }));
        assert_eq!(cloud_api_timeout(&tool, None), Duration::from_secs(90));
        // An explicit caller deadline still wins over the declaration.
        assert_eq!(
            cloud_api_timeout(&tool, Some(Duration::from_secs(5))),
            Duration::from_secs(5)
        );
        // Both sides are bounded by the host ceiling, and zero never means "no timeout".
        tool.metadata = Some(serde_json::json!({ "cloudApi": { "timeoutMs": 9_000_000 } }));
        assert_eq!(cloud_api_timeout(&tool, None), CLOUD_API_MAX_TIMEOUT);
        assert_eq!(
            cloud_api_timeout(&tool, Some(Duration::from_secs(4_000))),
            CLOUD_API_MAX_TIMEOUT
        );
        assert_eq!(
            cloud_api_timeout(&tool, Some(Duration::ZERO)),
            Duration::from_millis(1)
        );
    }

    #[test]
    fn execute_cloud_api_tool_supports_formal_multipart_template_contract() {
        let root = temp_root("cloud-multipart-template");
        let upload_path = root.join("upload.png");
        fs::write(&upload_path, b"loom-upload").expect("write upload fixture");

        let fixture = CloudFixture::start(CloudFixtureMode::MultipartText);
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "fixture-cloud-multipart",
            "name": "Fixture Cloud Multipart",
            "description": "Call a formal multipart cloud API",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": fixture.url("/upload/{{inputs.route.value}}?mode={{mode}}"),
                "method": "POST",
                "contentType": "multipart/form-data",
                "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\",\"X-Mode\":\"{{mode}}\"}",
                "body": "{\"file\":\"{{inputs.image.path}}\",\"prompt\":\"{{inputs.prompt.value}}\",\"literal\":\"fixed\",\"skipEmpty\":\"{{inputs.empty.value}}\",\"skipDisabled\":\"{{inputs.disabled.value}}\"}"
            },
            "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
        }))
        .expect("formal multipart cloud API execution deserializes");

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
        .expect("execute formal multipart cloud API tool");

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
    fn only_a_templates_own_placeholders_count_as_unresolved() {
        assert_eq!(
            unresolved_cloud_template_placeholder(
                "{{inputs.prompt.value}}",
                "{{inputs.prompt.value}}"
            ),
            Some("{{inputs.prompt.value}}")
        );
        assert_eq!(
            unresolved_cloud_template_placeholder("{{prompt}}", "a filled value"),
            None
        );
        // Braces that arrived inside an argument's value are content, not an unfilled placeholder.
        assert_eq!(
            unresolved_cloud_template_placeholder("{{prompt}}", "render {{this}} literally"),
            None
        );
        // An unterminated `{{` cannot be substituted by anything, so it is not reported either.
        assert_eq!(
            unresolved_cloud_template_placeholder("{{prompt", "{{prompt"),
            None
        );
    }

    #[test]
    fn a_multipart_field_with_an_unfilled_placeholder_is_reported_not_dropped() {
        let tool = ToolDefinition::new(
            "fixture-cloud-multipart-unresolved",
            "Fixture Cloud Multipart Unresolved",
            "Report a multipart field whose binding never resolved",
            ToolExecution::CloudApi {
                endpoint: "https://example.com/upload".to_owned(),
                method: "POST".to_owned(),
                content_type: Some("multipart/form-data".to_owned()),
                headers: None,
                body: Some("{\"prompt\":\"{{inputs.prompt.value}}\"}".to_owned()),
            },
        );

        let arguments = serde_json::json!({ "unrelated": "value" });
        let error = run_cloud_future(build_cloud_multipart_form(
            &tool,
            Some("{\"prompt\":\"{{inputs.prompt.value}}\"}"),
            &arguments,
        ))
        .expect("run multipart builder")
        .expect_err("an unresolved multipart binding is an error");

        let message = error.to_string();
        assert!(message.contains("prompt"));
        assert!(message.contains("{{inputs.prompt.value}}"));
    }

    #[test]
    fn a_body_declared_on_a_method_that_cannot_send_one_is_rejected() {
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "fixture-cloud-get-body",
            "name": "Fixture Cloud GET Body",
            "description": "Declare a body on a method that never sends it",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": "https://example.com/search",
                "method": "GET",
                "body": "{\"query\":\"{{inputs.query.value}}\"}"
            }
        }))
        .expect("GET cloud API tool deserializes");

        let error = execute_tool(&tool, &[], serde_json::json!({ "query": "cat" }))
            .expect_err("a body on GET is an authoring mistake, not a silent drop");

        let message = error.to_string();
        assert!(message.contains("GET"));
        assert!(message.contains("does not send one"));
    }

    #[test]
    fn a_multipart_field_named_file_no_longer_uploads_a_caller_named_path() {
        let root = temp_root("cloud-multipart-field-name");
        let secret_path = root.join("private-key");
        fs::write(&secret_path, b"BEGIN PRIVATE KEY").expect("write secret fixture");

        let fixture = CloudFixture::start(CloudFixtureMode::MultipartText);
        // The author bound an ordinary value, not a path. Before this fix the field *name* alone
        // made the host read the caller's path off disk and upload the bytes.
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "fixture-cloud-multipart-field-name",
            "name": "Fixture Cloud Multipart Field Name",
            "description": "Call a multipart cloud API with a value-bound field named file",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": fixture.url("/upload/plain"),
                "method": "POST",
                "contentType": "multipart/form-data",
                "body": "{\"file\":\"{{inputs.file.value}}\",\"image_file\":\"{{inputs.file.value}}\"}"
            },
            "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
        }))
        .expect("multipart cloud API execution deserializes");

        execute_tool(
            &tool,
            &[],
            serde_json::json!({ "file": secret_path.display().to_string() }),
        )
        .expect("execute multipart cloud API tool");

        let request = fixture.request();
        assert!(request.contains("name=\"file\""));
        assert!(
            !request.contains("filename="),
            "a value-bound field must travel as text: {request}"
        );
        assert!(
            !request.contains("BEGIN PRIVATE KEY"),
            "the named file's contents must never be uploaded: {request}"
        );

        fs::remove_dir_all(root).expect("cleanup multipart field name root");
    }

    #[test]
    fn a_declared_multipart_upload_path_has_to_sit_inside_a_loom_owned_root() {
        let root = temp_root("cloud-multipart-containment");
        let inside = root.join("staged-input.png");
        fs::write(&inside, b"staged").expect("write staged input");

        // A directory the host does not own, next to Loom's own staging directories rather than
        // inside one, standing in for any local file a caller might name.
        let outside_root = std::env::temp_dir().join(format!(
            "cloud-upload-probe-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&outside_root).expect("create probe root");
        let outside = outside_root.join("private-key");
        fs::write(&outside, b"BEGIN PRIVATE KEY").expect("write probe secret");

        let tool = ToolDefinition::new(
            "fixture-cloud-containment",
            "Fixture Cloud Containment",
            "Upload path resolution only",
            ToolExecution::CloudApi {
                endpoint: "https://api.example.com/upload".to_owned(),
                method: "POST".to_owned(),
                content_type: Some("multipart/form-data".to_owned()),
                headers: None,
                body: None,
            },
        );

        assert_eq!(
            cloud_multipart_upload_path(&tool, "file", &inside.display().to_string())
                .expect("a staged input under a Loom temp root is accepted"),
            fs::canonicalize(&inside).expect("canonical staged input")
        );

        let error = cloud_multipart_upload_path(&tool, "file", &outside.display().to_string())
            .expect_err("a file outside every Loom-owned root is refused");
        assert!(
            error.to_string().contains("resolves outside"),
            "unexpected refusal reason: {error}"
        );

        assert!(
            cloud_multipart_upload_path(&tool, "file", &root.display().to_string())
                .expect_err("a directory is not an upload")
                .to_string()
                .contains("is not a file")
        );
        assert!(cloud_multipart_upload_path(
            &tool,
            "file",
            &root.join("absent.png").display().to_string()
        )
        .expect_err("a missing path is refused")
        .to_string()
        .contains("cannot resolve upload path"));

        // A package that ships its own resource declares where it lives, and that directory vouches
        // for its subtree even when it sits nowhere near the host temp directory.
        let mut packaged = tool.clone();
        packaged.metadata = Some(serde_json::json!({
            "artPackage": { "dir": outside_root.display().to_string() }
        }));
        assert!(
            cloud_multipart_upload_path(&packaged, "file", &outside.display().to_string()).is_ok(),
            "a file inside the declared Art package directory is uploadable"
        );

        fs::remove_dir_all(outside_root).expect("cleanup probe root");
        fs::remove_dir_all(root).expect("cleanup multipart containment root");
    }

    #[test]
    fn only_a_declared_path_binding_makes_a_multipart_field_a_file() {
        assert!(is_cloud_multipart_file_field("{{inputs.input.path}}"));
        assert!(is_cloud_multipart_file_field("{{inputs.image}}"));
        // Field names no longer decide this, and neither does a value binding.
        assert!(!is_cloud_multipart_file_field("{{inputs.file.value}}"));
        assert!(!is_cloud_multipart_file_field("{{prompt}}"));
        assert!(!is_cloud_multipart_file_field("fixed"));
    }

    #[test]
    fn an_endpoint_argument_cannot_rewrite_the_request_authority() {
        let arguments =
            serde_json::json!({ "suffix": "@127.0.0.1:8787/steal", "route": "image-v2" });
        let rendered = substitute_cloud_template_with(
            "https://api.example.com{{inputs.suffix}}",
            &arguments,
            percent_encode_cloud_template_value,
        );
        assert_eq!(
            rendered,
            "https://api.example.com%40127.0.0.1%3A8787%2Fsteal"
        );
        assert!(!rendered.contains('@'));

        // Unreserved characters still travel through untouched, so ordinary route and parameter
        // bindings render the way their authors wrote them.
        assert_eq!(
            substitute_cloud_template_with(
                "https://api.example.com/v1/{{inputs.route.value}}?mode={{route}}",
                &arguments,
                percent_encode_cloud_template_value,
            ),
            "https://api.example.com/v1/image-v2?mode=image-v2"
        );

        // The authority guard states the invariant outright, independently of the encoding.
        assert!(validate_rendered_cloud_authority(
            "https://api.example.com/v1/{{inputs.route.value}}",
            "https://api.example.com/v1/image-v2",
        )
        .is_ok());
        assert!(validate_rendered_cloud_authority(
            "https://api.example.com/v1",
            "https://api.example.com@127.0.0.1:8787/v1",
        )
        .expect_err("a moved authority is refused")
        .contains("does not match the declared authority"));
        // An author who templates the host itself is trusted to have meant it; the declared domain
        // list is what constrains that case.
        assert!(validate_rendered_cloud_authority(
            "https://{{inputs.region}}.api.example.com/v1",
            "https://eu.api.example.com/v1",
        )
        .is_ok());
    }

    #[test]
    fn a_json_body_argument_cannot_add_sibling_fields() {
        let fixture = CloudFixture::start(CloudFixtureMode::Text);
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "fixture-cloud-json-injection",
            "name": "Fixture Cloud JSON Injection",
            "description": "Call a JSON cloud API with a quote-carrying argument",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": fixture.url("/text"),
                "method": "POST",
                "contentType": "application/json",
                "body": "{\"prompt\":\"{{inputs.text}}\"}"
            },
            "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
        }))
        .expect("JSON cloud API execution deserializes");

        let injection = "x\",\"stream\":true,\"model\":\"attacker";
        execute_tool(&tool, &[], serde_json::json!({ "text": injection }))
            .expect("execute JSON cloud API tool");

        let request = fixture.request();
        let body = request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_owned())
            .expect("captured request body");
        let sent = serde_json::from_str::<serde_json::Value>(&body).expect("request body is JSON");
        let sent = sent.as_object().expect("request body is an object");
        // The argument stays one string value: it cannot become extra request members.
        assert_eq!(sent.len(), 1);
        assert_eq!(sent["prompt"], serde_json::json!(injection));
        assert!(sent.get("stream").is_none());
        assert!(sent.get("model").is_none());
    }

    #[test]
    fn a_header_argument_stays_one_header_value() {
        let fixture = CloudFixture::start(CloudFixtureMode::Text);
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "fixture-cloud-header-injection",
            "name": "Fixture Cloud Header Injection",
            "description": "Call a JSON cloud API with a quote-carrying header argument",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": fixture.url("/text"),
                "method": "POST",
                "contentType": "application/json",
                "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\"}",
                "body": "{\"prompt\":\"{{inputs.prompt.value}}\"}"
            },
            "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
        }))
        .expect("header template cloud API execution deserializes");

        execute_tool(
            &tool,
            &[],
            serde_json::json!({
                "trace": "trace-42\",\"X-Injected\":\"yes",
                "prompt": "hello"
            }),
        )
        .expect("execute header template cloud API tool");

        let request = fixture.request();
        let request_lower = request.to_ascii_lowercase();
        assert!(!request_lower.contains("x-injected:"));
        assert_eq!(request_lower.matches("x-trace:").count(), 1);
        assert!(request.contains("trace-42\",\"X-Injected\":\"yes"));
    }

    #[test]
    fn a_header_argument_carrying_a_line_break_is_refused() {
        let fixture = CloudFixture::start(CloudFixtureMode::Text);
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "fixture-cloud-header-control",
            "name": "Fixture Cloud Header Control",
            "description": "Call a JSON cloud API with a line break in a header argument",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": fixture.url("/text"),
                "method": "POST",
                "contentType": "application/json",
                "headers": "{\"X-Trace\":\"{{inputs.trace.value}}\"}",
                "body": "{\"prompt\":\"hello\"}"
            },
            "metadata": { "permissionPolicy": { "network": { "allowLocalhost": true } } }
        }))
        .expect("header control cloud API execution deserializes");

        let error = execute_tool(
            &tool,
            &[],
            serde_json::json!({ "trace": "trace-42\r\nX-Injected: yes" }),
        )
        .expect_err("a header value carrying a line break is refused");
        assert!(error.to_string().contains("control character"));
    }

    #[test]
    fn a_json_body_template_that_is_not_json_yet_still_renders() {
        // A placeholder standing in for an unquoted number cannot be parsed before substitution, so
        // that template keeps the original splice-then-parse path.
        let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
            "id": "fixture-cloud-typed-template",
            "name": "Fixture Cloud Typed Template",
            "description": "Render a body template whose placeholder is an unquoted value",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": "https://example.invalid/v1",
                "method": "POST"
            }
        }))
        .expect("typed template cloud API execution deserializes");

        let rendered = render_cloud_json_template(
            &tool,
            "body",
            "{\"steps\": {{inputs.steps.value}}, \"prompt\": \"{{inputs.prompt.value}}\"}",
            &serde_json::json!({ "steps": 12, "prompt": "hello" }),
        )
        .expect("render an unquoted placeholder body");
        assert_eq!(rendered["steps"], serde_json::json!(12));
        assert_eq!(rendered["prompt"], serde_json::json!("hello"));

        // A templated object key still renders on the structural path.
        let keyed = render_cloud_json_template(
            &tool,
            "body",
            "{\"{{inputs.field.value}}\":\"{{inputs.prompt.value}}\"}",
            &serde_json::json!({ "field": "prompt", "prompt": "hello" }),
        )
        .expect("render a templated body key");
        assert_eq!(keyed["prompt"], serde_json::json!("hello"));
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
        if std::env::var("LOOM_TOOL_REGISTRY_MCP_FIXTURE_MODE")
            .ok()
            .as_deref()
            == Some("hang")
        {
            thread::sleep(Duration::from_secs(30));
            return;
        }
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let fixture_image_url = std::env::var("LOOM_MCP_FIXTURE_IMAGE_URL").ok();
        let fixture_image_url_alt = std::env::var("LOOM_MCP_FIXTURE_IMAGE_URL_ALT").ok();
        let mut counter = 0_u64;

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
                                    "name": "counter",
                                    "description": "Count calls in this fixture process",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {},
                                        "additionalProperties": false
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
                        "counter" => {
                            counter += 1;
                            write_fixture_response(
                                &mut stdout,
                                serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": request["id"].clone(),
                                    "result": {
                                        "content": [{ "type": "text", "text": counter.to_string() }]
                                    }
                                }),
                            );
                        }
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

    #[derive(Clone, Copy)]
    enum CloudFixtureMode {
        Text,
        Image,
        Error,
        MultipartText,
        DelayedHeaders,
        DelayedBody,
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
                if matches!(
                    mode,
                    CloudFixtureMode::DelayedHeaders | CloudFixtureMode::DelayedBody
                ) {
                    let response = serde_json::json!({
                        "content": [{ "type": "text", "text": "too late" }]
                    })
                    .to_string();
                    match mode {
                        CloudFixtureMode::DelayedHeaders => {
                            thread::sleep(Duration::from_millis(500));
                            write_http_response(
                                &mut stream,
                                "200 OK",
                                "application/json",
                                &response,
                            );
                        }
                        CloudFixtureMode::DelayedBody => {
                            let _ = write!(
                                stream,
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                response.len()
                            );
                            let split = response.len() / 2;
                            let _ = stream.write_all(&response.as_bytes()[..split]);
                            let _ = stream.flush();
                            thread::sleep(Duration::from_millis(500));
                            let _ = stream.write_all(&response.as_bytes()[split..]);
                            let _ = stream.flush();
                        }
                        _ => unreachable!(),
                    }
                    return;
                }
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
                    CloudFixtureMode::DelayedHeaders | CloudFixtureMode::DelayedBody => {
                        unreachable!()
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

    /// An HTTP image fixture that keeps answering requests, 404ing every path but one.
    ///
    /// The single-connection fixture cannot express a retry, and a retry is the whole point of a test
    /// about a download that asks for the wrong URL first. This one serves until it is dropped, so a
    /// regression that stops after the first attempt fails the assertion instead of hanging the suite.
    struct RetryingExactPathHttpImageFixture {
        port: u16,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }

    impl RetryingExactPathHttpImageFixture {
        fn start(content_type: &'static str, body: Vec<u8>, expected_path: &'static str) -> Self {
            let listener =
                TcpListener::bind(("127.0.0.1", 0)).expect("bind retrying HTTP image fixture");
            let port = listener
                .local_addr()
                .expect("retrying HTTP image fixture address")
                .port();
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let worker_stop = std::sync::Arc::clone(&stop);
            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        break;
                    };
                    if worker_stop.load(Ordering::Acquire) {
                        break;
                    }
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
                        write_http_response(
                            &mut stream,
                            "404 Not Found",
                            "text/plain",
                            "not found",
                        );
                    }
                }
            });
            Self {
                port,
                stop,
                worker: Some(worker),
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }
    }

    impl Drop for RetryingExactPathHttpImageFixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
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
}
