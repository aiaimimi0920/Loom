//! Generic stdin/stdout bridge for externally packaged Art frameworks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use loom_process::{ProcessError, ProcessSpec};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::framework::{
    enforce_framework_permission_policy, resolve_framework_package_dir, FrameworkPackageManifest,
    FRAMEWORK_PROTOCOL_VERSION,
};
use crate::{ToolDefinition, ToolRegistryError, ToolRegistryResult};

pub use loom_protocol::{
    FrameworkExecuteError, FrameworkExecuteRequest, FrameworkExecuteResponse,
    FrameworkExecutionContext, FrameworkMcpServer,
};

pub const DEFAULT_FRAMEWORK_PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
/// File-size ceiling for an image an Art hands back by path.
///
/// This is checked against the file on disk, and it only became a memory bound when the reader stopped
/// decoding the file: peak is now the file plus its base64 at roughly 1.37×, both linear in the number
/// checked here. Before that a decode sat in the middle whose cost was width × height × 4 and had no
/// relation to the compressed size, so a file well under this limit could still ask for gigabytes.
///
/// The number itself is unchanged, and it is still generous for an image — 256 MiB of file is close to
/// 600 MiB of peak once the base64 and the JSON copy of it are counted. Lowering it would reject outputs
/// that work today, so it is left as a size limit to revisit rather than quietly tightened here.
const MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpArtDependency {
    server_id: String,
    package_id: String,
    version: String,
}

struct TempDirectoryGuard {
    path: PathBuf,
}

impl TempDirectoryGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectoryGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Execute a pluginized Art through its installed framework package.
pub fn execute_framework_art(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
) -> ToolRegistryResult<Value> {
    let packages_root =
        framework_packages_root().ok_or_else(|| ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: "LOOM_CONTROL_PLANE_ROOT/frameworks".to_owned(),
        })?;
    execute_framework_art_in_root_with_timeout(
        tool,
        framework,
        arguments,
        &packages_root,
        DEFAULT_FRAMEWORK_PROCESS_TIMEOUT,
        None,
    )
}

/// Execute a framework Art with a caller-owned upper timeout bound.
///
/// Surface actions use this entry point so their declared deadline also bounds
/// the managed process tree instead of merely timing out the caller.
pub fn execute_framework_art_with_timeout(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
    timeout: Duration,
) -> ToolRegistryResult<Value> {
    let packages_root =
        framework_packages_root().ok_or_else(|| ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: "LOOM_CONTROL_PLANE_ROOT/frameworks".to_owned(),
        })?;
    execute_framework_art_in_root_with_timeout(
        tool,
        framework,
        arguments,
        &packages_root,
        timeout.min(DEFAULT_FRAMEWORK_PROCESS_TIMEOUT),
        None,
    )
}

/// Execute a framework Art with timeout and caller-owned cancellation.
///
/// Cancellation is propagated to `loom_process`, which terminates the managed
/// process tree before returning.
pub fn execute_framework_art_with_timeout_and_cancellation(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> ToolRegistryResult<Value> {
    let packages_root =
        framework_packages_root().ok_or_else(|| ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: "LOOM_CONTROL_PLANE_ROOT/frameworks".to_owned(),
        })?;
    execute_framework_art_in_root_with_timeout(
        tool,
        framework,
        arguments,
        &packages_root,
        timeout.min(DEFAULT_FRAMEWORK_PROCESS_TIMEOUT),
        Some(cancellation),
    )
}

fn execute_framework_art_in_root_with_timeout(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
    packages_root: &Path,
    timeout: Duration,
    cancellation: Option<&AtomicBool>,
) -> ToolRegistryResult<Value> {
    if !crate::framework::is_valid_framework_reference(framework) {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "framework id is not a safe package id".to_owned(),
        });
    }

    let package_dir = resolve_framework_package_dir(packages_root, framework).map_err(|error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            // The resolver distinguishes "no package installed" from "several publishers ship this
            // local id"; carrying its message keeps that distinction visible to the operator.
            path: format!("{} ({error})", packages_root.display()),
        }
    })?;
    let manifest_path = package_dir.join("framework.manifest.json");
    let canonical_packages_root = fs::canonicalize(packages_root).map_err(|_error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: packages_root.display().to_string(),
        }
    })?;
    let package_dir = fs::canonicalize(&package_dir).map_err(|_error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: manifest_path.display().to_string(),
        }
    })?;
    if !package_dir.starts_with(&canonical_packages_root) {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "framework package resolves outside the package root".to_owned(),
        });
    }
    let manifest_path = package_dir.join("framework.manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|_error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: manifest_path.display().to_string(),
        }
    })?;
    let manifest: FrameworkPackageManifest =
        serde_json::from_str(&manifest_text).map_err(|error| {
            ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: format!("invalid framework.manifest.json: {error}"),
            }
        })?;
    let negotiated_protocol =
        loom_protocol::negotiate_framework_protocol(&manifest).map_err(|error| {
            ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: error.to_string(),
            }
        })?;
    if manifest.id != framework && manifest.qualified_id() != framework {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!("manifest identity mismatch: id={}", manifest.id),
        });
    }
    if manifest.entry.kind != "process" || manifest.entry.command.trim().is_empty() {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "manifest entry must be a process with a command".to_owned(),
        });
    }
    enforce_framework_permission_policy(&manifest).map_err(|reason| {
        ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!("permission enforcement unavailable: {reason}"),
        }
    })?;
    let command_path = Path::new(&manifest.entry.command);
    if command_path.is_absolute()
        || command_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "manifest entry command must be relative to the package".to_owned(),
        });
    }
    let command_path = package_dir.join(command_path);
    if !command_path.is_file() {
        return Err(ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: command_path.display().to_string(),
        });
    }

    let art_dir =
        art_directory(tool).ok_or_else(|| ToolRegistryError::FrameworkArtDirectoryNotFound {
            id: tool.id.clone(),
            path: "<metadata.artPackage.dir>".to_owned(),
        })?;
    if !art_dir.is_dir() {
        return Err(ToolRegistryError::FrameworkArtDirectoryNotFound {
            id: tool.id.clone(),
            path: art_dir.display().to_string(),
        });
    }

    let request_id = request_id();
    let cache_dir =
        art_package_path(tool, "cacheDir").unwrap_or_else(|| art_dir.join(".loom-cache"));
    let state_dir = art_package_path(tool, "stateDir");
    let output_dir = art_package_path(tool, "outputDir");
    let temp_dir = std::env::temp_dir()
        .join("loom-framework")
        .join(&request_id);
    fs::create_dir_all(&cache_dir).map_err(|error| framework_io_error(tool, framework, error))?;
    fs::create_dir_all(&temp_dir).map_err(|error| framework_io_error(tool, framework, error))?;
    let temp_dir = TempDirectoryGuard::new(temp_dir);

    let (inputs, params, disabled_params) = split_arguments(tool, &arguments);
    let credential_store = packages_root
        .parent()
        .map(crate::credentials::CredentialStore::new);
    let art_identity = tool.qualified_id();
    let (mut credentials, mcp_server) = if framework == "mcp" {
        let control_plane_root =
            packages_root
                .parent()
                .ok_or_else(|| ToolRegistryError::FrameworkProcessProtocol {
                    id: tool.id.clone(),
                    framework: framework.to_owned(),
                    reason: "framework package root has no control-plane parent".to_owned(),
                })?;
        let (server, grants) =
            resolve_mcp_server(tool, control_plane_root, credential_store.as_ref())?;
        (grants, Some(server))
    } else {
        let grants = credential_store
            .as_ref()
            .map(|store| {
                store.grants_for(
                    framework,
                    &art_identity,
                    &manifest.permission_policy.credentials,
                )
            })
            .transpose()
            .map_err(|error| ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: format!("credential broker failed: {error}"),
            })?
            .unwrap_or_default();
        (grants, None)
    };
    if framework != "mcp" {
        if let (Some(store), bindings) = (credential_store.as_ref(), art_credential_bindings(tool))
        {
            if !bindings.is_empty() {
                let bound = store
                    .grants_for_bindings(framework, &art_identity, &bindings)
                    .map_err(|error| ToolRegistryError::FrameworkProcessProtocol {
                        id: tool.id.clone(),
                        framework: framework.to_owned(),
                        reason: format!("credential binding failed: {error}"),
                    })?;
                for grant in bound {
                    credentials.retain(|existing| existing.name != grant.name);
                    credentials.push(grant);
                }
            }
        }
    }
    let request = FrameworkExecuteRequest {
        protocol_version: negotiated_protocol.to_owned(),
        supported_protocol_versions: vec![FRAMEWORK_PROTOCOL_VERSION.to_owned()],
        framework_id: manifest.id.clone(),
        art_id: tool.id.clone(),
        art_dir: art_dir.clone(),
        inputs,
        params,
        disabled_params,
        context: FrameworkExecutionContext {
            request_id,
            cache_dir: cache_dir.clone(),
            temp_dir: temp_dir.path().to_path_buf(),
            state_dir,
            output_dir: output_dir.clone(),
            host_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            framework_version: Some(manifest.version.clone()),
            art_version: art_package_string(tool, "version"),
            granted_permissions: manifest.permission_policy.clone(),
            credentials,
            mcp_server,
            ..FrameworkExecutionContext::default()
        },
    };
    let payload = serde_json::to_vec(&request).map_err(|error| {
        ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!("cannot serialize request: {error}"),
        }
    })?;

    let mut process = ProcessSpec::new(&command_path);
    process.args = manifest.entry.args.clone();
    process.current_dir = Some(package_dir.clone());
    process.limits.timeout = manifest
        .resources
        .timeout_seconds
        .map(Duration::from_secs)
        .map(|declared| declared.min(timeout))
        .unwrap_or(timeout);
    process.limits.stdout_bytes = manifest
        .resources
        .stdout_mib
        .and_then(|value| usize::try_from(value.saturating_mul(1024 * 1024)).ok())
        .unwrap_or(process.limits.stdout_bytes);
    process.limits.stderr_bytes = manifest
        .resources
        .stderr_mib
        .and_then(|value| usize::try_from(value.saturating_mul(1024 * 1024)).ok())
        .unwrap_or(process.limits.stderr_bytes);
    process.limits.memory_bytes = manifest
        .resources
        .memory_mib
        .and_then(|value| usize::try_from(value.saturating_mul(1024 * 1024)).ok())
        .or(process.limits.memory_bytes);
    process.limits.max_processes = manifest
        .resources
        .max_processes
        .or(process.limits.max_processes);
    let mut stdin_payload = payload;
    stdin_payload.push(b'\n');
    let process_output = match cancellation {
        Some(cancellation) => {
            loom_process::run_with_input_cancellable(&process, &stdin_payload, cancellation)
        }
        None => loom_process::run_with_input(&process, &stdin_payload),
    }
    .map_err(|error| map_process_error(tool, framework, process.limits.timeout, error))?;
    let exit_status = process_output.status;
    let stdout = process_output.stdout;
    let stderr = String::from_utf8_lossy(&process_output.stderr).into_owned();
    let stdout_text = String::from_utf8_lossy(&stdout).trim().to_owned();
    if !exit_status.success() {
        return Err(ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: exit_status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_owned()),
            message: "framework process exited unsuccessfully".to_owned(),
            detail: crate::bounded_error_text(&stderr),
        });
    }
    let mut response: FrameworkExecuteResponse =
        serde_json::from_str(&stdout_text).map_err(|error| {
            ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: format!(
                    "invalid JSON response: {error}; stdout: {}",
                    crate::bounded_error_text(&stdout_text)
                ),
            }
        })?;
    response
        .diagnostics
        .get_or_insert(process_output.diagnostics);
    let status = response.status.trim().to_ascii_lowercase();
    if !loom_protocol::response_status_is_success(&status) {
        let error = response.error.unwrap_or(FrameworkExecuteError {
            code: "framework_failed".to_owned(),
            message: "framework returned a failure status".to_owned(),
            detail: None,
        });
        return Err(ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: error.code,
            message: error.message,
            detail: error.detail.unwrap_or_default(),
        });
    }

    normalize_framework_image_output(
        tool,
        framework,
        &mut response.output,
        &[
            temp_dir.path(),
            cache_dir.as_path(),
            output_dir.as_deref().unwrap_or(temp_dir.path()),
        ],
    )?;

    Ok(response_to_tool_value(tool, response))
}

fn normalize_framework_image_output(
    tool: &ToolDefinition,
    framework: &str,
    output: &mut Value,
    allowed_roots: &[&Path],
) -> ToolRegistryResult<()> {
    if !tool.outputs.iter().any(is_image_output_definition) {
        return Ok(());
    }
    let Some(path) = framework_image_output_path(output) else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(framework_image_output_error(
            tool,
            framework,
            "image output path must be absolute",
        ));
    }
    let canonical_path = fs::canonicalize(&path).map_err(|error| {
        framework_image_output_error(
            tool,
            framework,
            format!("cannot resolve image output path: {error}"),
        )
    })?;
    if !canonical_path.is_file() {
        return Err(framework_image_output_error(
            tool,
            framework,
            "image output path is not a file",
        ));
    }
    let inside_allowed_root = allowed_roots.iter().any(|root| {
        fs::canonicalize(root)
            .ok()
            .is_some_and(|canonical_root| canonical_path.starts_with(canonical_root))
    });
    if !inside_allowed_root {
        return Err(framework_image_output_error(
            tool,
            framework,
            "image output path resolves outside the execution output roots",
        ));
    }
    let bytes = fs::metadata(&canonical_path)
        .map_err(|error| {
            framework_image_output_error(
                tool,
                framework,
                format!("cannot inspect image output: {error}"),
            )
        })?
        .len();
    if bytes > MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES {
        return Err(framework_image_output_error(
            tool,
            framework,
            format!(
                "image output exceeds the {} byte limit",
                MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES
            ),
        ));
    }
    let image =
        loom_image_io::read_image_path_as_web_data_url(&canonical_path).map_err(|error| {
            framework_image_output_error(
                tool,
                framework,
                format!("cannot decode image output: {error}"),
            )
        })?;
    let content = json!([{
        "type": "image",
        "data": image.data_url,
        // The label comes from the bytes rather than from a constant. It used to say `image/png`
        // unconditionally, which was true only because the reader re-encoded everything to PNG; now
        // that a JPEG or WebP output is passed through as itself, a fixed label would be a lie to
        // every consumer that trusts the field over the data URL's own prefix.
        "mimeType": image.mime_type
    }]);
    match output {
        Value::Object(object) => {
            for key in ["output_path", "outputPath", "file_path", "filePath", "path"] {
                object.remove(key);
            }
            // The shared Art runtime emits `output_base64` beside a `content` block of its own, so
            // the data URL the host just built from the validated file is a second full copy of the
            // same image. Drop the self-declared one: it was never checked against the output roots
            // or the size limit, and every reader in the workspace falls back to `content[0].data`
            // when it is absent.
            for key in ["output_base64", "outputBase64"] {
                object.remove(key);
            }
            object.insert("content".to_owned(), content);
        }
        value => {
            *value = json!({ "content": content });
        }
    }
    Ok(())
}

fn is_image_output_definition(output: &Value) -> bool {
    let Some(output) = output.as_object() else {
        return false;
    };
    output
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("image"))
        || output
            .get("executionType")
            .or_else(|| output.get("execution_type"))
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_ascii_lowercase().starts_with("image_"))
}

fn framework_image_output_path(output: &Value) -> Option<&str> {
    match output {
        Value::String(path) => Some(path),
        Value::Object(object) => ["output_path", "outputPath", "file_path", "filePath", "path"]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_str)),
        _ => None,
    }
}

fn framework_image_output_error(
    tool: &ToolDefinition,
    framework: &str,
    reason: impl Into<String>,
) -> ToolRegistryError {
    ToolRegistryError::FrameworkProcessProtocol {
        id: tool.id.clone(),
        framework: framework.to_owned(),
        reason: reason.into(),
    }
}

fn map_process_error(
    tool: &ToolDefinition,
    framework: &str,
    timeout: Duration,
    error: ProcessError,
) -> ToolRegistryError {
    match error {
        ProcessError::Spawn(error) => ToolRegistryError::FrameworkProcessSpawn {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: error.to_string(),
        },
        ProcessError::Timeout { .. } => ToolRegistryError::FrameworkProcessTimeout {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            timeout_ms: timeout.as_millis(),
        },
        ProcessError::Cancelled { .. } => ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: "cancelled".to_owned(),
            message: "framework process was cancelled".to_owned(),
            detail: String::new(),
        },
        ProcessError::OutputLimit {
            stderr,
            diagnostics,
            ..
        } => ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: "resource_limit".to_owned(),
            message: "framework process exceeded bounded output limits".to_owned(),
            detail: format!(
                "stderr={} bytes; stdout={} bytes; {}",
                diagnostics.stderr_bytes,
                diagnostics.stdout_bytes,
                crate::bounded_error_text(&String::from_utf8_lossy(&stderr))
            ),
        },
        other => ToolRegistryError::FrameworkProcessIo {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: other.to_string(),
        },
    }
}

fn framework_io_error(
    tool: &ToolDefinition,
    framework: &str,
    error: std::io::Error,
) -> ToolRegistryError {
    ToolRegistryError::FrameworkProcessIo {
        id: tool.id.clone(),
        framework: framework.to_owned(),
        reason: error.to_string(),
    }
}

fn framework_packages_root() -> Option<PathBuf> {
    std::env::var("LOOM_FRAMEWORK_PACKAGES_DIR")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("LOOM_CONTROL_PLANE_ROOT")
                .ok()
                .map(|value| PathBuf::from(value).join("frameworks"))
        })
}

fn art_directory(tool: &ToolDefinition) -> Option<PathBuf> {
    let metadata = tool.metadata.as_ref()?.as_object()?;
    let package = metadata
        .get("artPackage")
        .and_then(Value::as_object)
        .and_then(|value| value.get("dir"))
        .and_then(Value::as_str);
    package.map(PathBuf::from)
}

fn art_package_path(tool: &ToolDefinition, key: &str) -> Option<PathBuf> {
    art_package_string(tool, key).map(PathBuf::from)
}

fn art_package_string(tool: &ToolDefinition, key: &str) -> Option<String> {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn art_credential_bindings(tool: &ToolDefinition) -> BTreeMap<String, String> {
    let settings = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artUserSettings"))
        .and_then(|settings| settings.get("credentialBindings"));
    let authoring = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("authoring"))
        .and_then(|authoring| authoring.get("credentialBindings"));
    settings
        .or(authoring)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn resolve_mcp_server(
    tool: &ToolDefinition,
    control_plane_root: &Path,
    credential_store: Option<&crate::credentials::CredentialStore>,
) -> ToolRegistryResult<(FrameworkMcpServer, Vec<loom_protocol::CredentialGrant>)> {
    let dependency: McpArtDependency = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("mcp"))
        .cloned()
        .ok_or_else(|| {
            mcp_dependency_error(
                tool,
                "<missing>",
                "mcp_dependency_invalid",
                "MCP Art metadata.mcp is required",
            )
        })
        .and_then(|value| {
            serde_json::from_value(value).map_err(|error| {
                mcp_dependency_error(
                    tool,
                    "<invalid>",
                    "mcp_dependency_invalid",
                    format!("invalid MCP Art dependency: {error}"),
                )
            })
        })?;
    if dependency.server_id.trim().is_empty()
        || dependency.package_id.trim().is_empty()
        || dependency.version.trim().is_empty()
    {
        return Err(mcp_dependency_error(
            tool,
            dependency.server_id.as_str(),
            "mcp_dependency_invalid",
            "metadata.mcp.serverId, packageId, and version are required",
        ));
    }
    let store_path = control_plane_root.join("mcp").join("servers.json");
    let servers = fs::read(&store_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<loom_mcp::McpServerConfig>>(&bytes).ok())
        .unwrap_or_default();
    let server = servers
        .into_iter()
        .find(|server| server.id == dependency.server_id)
        .ok_or_else(|| {
            mcp_dependency_error(
                tool,
                &dependency.server_id,
                "mcp_dependency_missing",
                format!(
                    "independent MCP server `{}` is not installed",
                    dependency.server_id
                ),
            )
        })?;
    if !server.enabled {
        return Err(mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_disabled",
            format!("MCP server `{}` is disabled", dependency.server_id),
        ));
    }
    let package = server.package.as_ref().ok_or_else(|| {
        mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_package_mismatch",
            "the selected MCP server is not installed from a package",
        )
    })?;
    if package.qualified_id != dependency.package_id {
        return Err(mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_package_mismatch",
            format!(
                "Art requires MCP package `{}`, but server `{}` is package `{}`",
                dependency.package_id, dependency.server_id, package.qualified_id
            ),
        ));
    }
    let version_requirement = semver::VersionReq::parse(&dependency.version).map_err(|error| {
        mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_invalid",
            format!(
                "invalid MCP version requirement `{}`: {error}",
                dependency.version
            ),
        )
    })?;
    let package_version = semver::Version::parse(&package.version).map_err(|error| {
        mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_version_mismatch",
            format!("installed MCP package version is invalid: {error}"),
        )
    })?;
    if !version_requirement.matches(&package_version) {
        return Err(mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_version_mismatch",
            format!(
                "Art requires MCP package version `{}`, but `{}` is installed",
                dependency.version, package.version
            ),
        ));
    }
    server.validate().map_err(|error| {
        mcp_dependency_error(
            tool,
            &dependency.server_id,
            "mcp_dependency_invalid",
            error.to_string(),
        )
    })?;
    for requirement in server
        .credential_requirements
        .iter()
        .filter(|requirement| requirement.required)
    {
        if !server.credential_bindings.contains_key(&requirement.id) {
            return Err(mcp_dependency_error(
                tool,
                &dependency.server_id,
                "mcp_credential_missing",
                format!("MCP credential `{}` is not configured", requirement.label),
            ));
        }
    }
    let credentials = credential_store
        .map(|store| store.grants_for_mcp_bindings(&server.id, &server.credential_bindings))
        .transpose()
        .map_err(|error| {
            mcp_dependency_error(
                tool,
                &dependency.server_id,
                "mcp_credential_missing",
                format!("MCP credential resolution failed: {error}"),
            )
        })?
        .unwrap_or_default();
    let optional_credential_ids = server
        .credential_requirements
        .iter()
        .filter(|requirement| !requirement.required)
        .map(|requirement| requirement.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let (credential_env, optional_credential_env) =
        server
            .credential_env
            .into_iter()
            .partition(|(_, credential_name)| {
                !optional_credential_ids.contains(credential_name.as_str())
            });
    let (credential_headers, optional_credential_headers) = server
        .credential_headers
        .into_iter()
        .partition(|(_, credential_name)| {
            !optional_credential_ids.contains(credential_name.as_str())
        });
    let resolved = FrameworkMcpServer {
        id: server.id,
        package_id: package.qualified_id.clone(),
        version: package.version.clone(),
        transport: server.transport.label().to_owned(),
        command: server.command,
        args: server.args,
        env: server.env,
        url: server.url,
        headers: server.headers,
        credential_env,
        credential_headers,
        optional_credential_env,
        optional_credential_headers,
    };
    Ok((resolved, credentials))
}

fn mcp_dependency_error(
    tool: &ToolDefinition,
    server_id: &str,
    code: &str,
    reason: impl Into<String>,
) -> ToolRegistryError {
    ToolRegistryError::McpDependency {
        tool_id: tool.qualified_id(),
        server_id: server_id.to_owned(),
        code: code.to_owned(),
        reason: reason.into(),
    }
}

fn split_arguments(tool: &ToolDefinition, arguments: &Value) -> (Value, Value, Vec<String>) {
    let Some(object) = arguments.as_object() else {
        return (arguments.clone(), Value::Object(Map::new()), Vec::new());
    };
    let disabled = object
        .get("disabledParams")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if object.contains_key("inputs") || object.contains_key("params") {
        let inputs = object.get("inputs").cloned().unwrap_or_else(|| json!({}));
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        return (inputs, params, disabled);
    }

    let parameter_ids = tool
        .params
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|parameter| parameter.get("id").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    let mut inputs = Map::new();
    let mut params = Map::new();
    for (key, value) in object {
        if key == "disabledParams" {
            continue;
        }
        if parameter_ids.contains(key.as_str()) {
            params.insert(key.clone(), value.clone());
        } else {
            inputs.insert(key.clone(), value.clone());
        }
    }
    (Value::Object(inputs), Value::Object(params), disabled)
}

fn selected_image_candidate_index(output: &Map<String, Value>, candidates: &[Value]) -> usize {
    if let Some(index) = output.get("selectedIndex").and_then(Value::as_u64) {
        return usize::try_from(index)
            .unwrap_or(usize::MAX)
            .min(candidates.len().saturating_sub(1));
    }
    let selected_id = output.get("selectedCandidate").and_then(Value::as_str);
    selected_id
        .and_then(|selected_id| {
            candidates.iter().position(|candidate| {
                candidate
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate_id| candidate_id == selected_id)
            })
        })
        .unwrap_or_default()
}

/// Ceilings the host applies to a framework's candidate array.
///
/// `normalize_framework_image_output` bounds the single output image — absolute path, inside an
/// execution output root, under `MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES`, replaced by exactly one data
/// URL — but none of that reaches `response.candidates`, which the framework builds itself and the
/// host previously inserted verbatim. An image candidate normally carries a full data URL, and the
/// finished value is cloned through the store while its mutex is held on the Surface action path, so
/// an Art returning a large grid costs that memory on the interaction hot path. The item count
/// matches `MAX_MCP_IMAGE_CANDIDATES` on the MCP tool path; the byte budget spans the whole array
/// rather than a single item.
const MAX_FRAMEWORK_CANDIDATES: usize = 64;
const MAX_FRAMEWORK_CANDIDATE_BYTES: usize = 32 * 1024 * 1024;

/// Approximate what a candidate occupies, by summing the strings and keys it holds rather than
/// serializing it, so measuring copies nothing.
///
/// Nesting depth needs no guard: the value was deserialized from the framework's stdout, and
/// `serde_json` refuses input deeper than its own recursion limit, so the structure walked here is
/// already bounded.
fn candidate_value_bytes(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        Value::Array(items) => items.iter().map(candidate_value_bytes).sum(),
        Value::Object(entries) => entries
            .iter()
            .map(|(key, value)| key.len() + candidate_value_bytes(value))
            .sum(),
        _ => 0,
    }
}

/// Truncate a candidate array to the host's ceilings, reporting how many items were dropped.
///
/// Truncation keeps the leading items, which is where the selected candidate sits unless the Art
/// says otherwise, and returns the drop count so a consumer can tell a truncated grid from a short
/// one. One item larger than the entire byte budget still survives as the only item: dropping it
/// would leave a grid with no images at all, which is worse than honouring the budget exactly.
fn bound_framework_candidates(mut candidates: Vec<Value>) -> (Vec<Value>, usize) {
    let mut dropped = candidates.len().saturating_sub(MAX_FRAMEWORK_CANDIDATES);
    candidates.truncate(MAX_FRAMEWORK_CANDIDATES);
    let mut budget = MAX_FRAMEWORK_CANDIDATE_BYTES;
    let mut kept = 0;
    for candidate in &candidates {
        let bytes = candidate_value_bytes(candidate);
        if kept > 0 && bytes > budget {
            break;
        }
        budget = budget.saturating_sub(bytes);
        kept += 1;
    }
    dropped += candidates.len() - kept;
    candidates.truncate(kept);
    (candidates, dropped)
}

/// The candidate keys every consumer reads, and the producer keys that may stand in for them.
///
/// The MCP tool path emits `{index, title, imageUrl, thumbnailUrl, sourcePageUrl, width, height}`
/// (`lib.rs`), and both consumers — the Hook canvas result strip and Hook's
/// `artDeliveryCandidates` — key each item on `imageUrl` and drop items without it. Framework Arts
/// author their own candidate objects and reach for the names their own runtime uses, so the host
/// normalizes them here instead of requiring every Art to know the wire shape.
const CANDIDATE_IMAGE_URL_SOURCES: &[&str] = &[
    "imageUrl",
    "image_url",
    "url",
    "src",
    "data",
    "dataUrl",
    "data_url",
    "thumbnailUrl",
    "thumbnail_url",
    "thumbnail",
];
const CANDIDATE_THUMBNAIL_URL_SOURCES: &[&str] =
    &["thumbnailUrl", "thumbnail_url", "thumbnail", "preview"];
const CANDIDATE_SOURCE_PAGE_URL_SOURCES: &[&str] = &[
    "sourcePageUrl",
    "source_page_url",
    "sourceUrl",
    "source_url",
    "pageUrl",
    "page_url",
];

fn first_candidate_string(item: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        item.get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

/// Fill in the canonical candidate keys without discarding what the Art already sent.
///
/// Producer-specific keys are left in place: Hook's candidate strip also renders `thumbnail`, and
/// an Art may attach its own fields. Only the canonical keys are added, and only when they are
/// missing or empty, so an Art that already speaks the wire shape passes through unchanged. An
/// item with no usable image reference at all is left alone rather than given a fabricated one; the
/// consumers drop it exactly as before.
fn normalize_image_candidate_item(index: usize, candidate: Value) -> Value {
    let mut item = match candidate {
        Value::Object(item) => item,
        other => return other,
    };
    if let Some(image_url) = first_candidate_string(&item, CANDIDATE_IMAGE_URL_SOURCES) {
        item.insert("imageUrl".to_owned(), Value::String(image_url));
    }
    if let Some(thumbnail_url) = first_candidate_string(&item, CANDIDATE_THUMBNAIL_URL_SOURCES) {
        item.insert("thumbnailUrl".to_owned(), Value::String(thumbnail_url));
    }
    if let Some(source_page_url) = first_candidate_string(&item, CANDIDATE_SOURCE_PAGE_URL_SOURCES)
    {
        item.insert("sourcePageUrl".to_owned(), Value::String(source_page_url));
    }
    if !item.get("index").is_some_and(Value::is_u64) {
        item.insert("index".to_owned(), json!(index));
    }
    Value::Object(item)
}

fn insert_image_candidate_metadata(
    output: &mut Map<String, Value>,
    candidates: Vec<Value>,
    dropped: usize,
) {
    let selected_index = selected_image_candidate_index(output, &candidates);
    let candidates = candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| normalize_image_candidate_item(index, candidate))
        .collect::<Vec<_>>();
    let metadata = output
        .entry("loomMetadata".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    metadata
        .as_object_mut()
        .expect("loomMetadata was normalized to an object")
        .insert(
            "candidates".to_owned(),
            json!({
                "kind": "image.candidates",
                "selectedIndex": selected_index,
                "droppedItems": dropped,
                "items": candidates,
            }),
        );
}

fn response_to_tool_value(tool: &ToolDefinition, response: FrameworkExecuteResponse) -> Value {
    let (candidates, dropped_candidates) = bound_framework_candidates(response.candidates);
    let has_candidates = !candidates.is_empty();
    let has_image_candidates =
        has_candidates && tool.outputs.iter().any(is_image_output_definition);
    let has_cache = !response.cache.is_null();
    let has_execution_metadata = response.diagnostics.is_some() || !response.events.is_empty();
    if !has_candidates && !has_cache && !has_execution_metadata {
        return response.output;
    }
    let execution_metadata = has_execution_metadata.then(|| {
        json!({
            "diagnostics": response.diagnostics,
            "events": response.events,
        })
    });
    if let Value::Object(mut output) = response.output {
        if has_image_candidates {
            insert_image_candidate_metadata(&mut output, candidates, dropped_candidates);
        } else if has_candidates {
            output.insert("candidates".to_owned(), Value::Array(candidates));
        }
        if has_cache {
            output.insert("cache".to_owned(), response.cache);
        }
        if let Some(execution_metadata) = execution_metadata {
            output.insert("_loomExecution".to_owned(), execution_metadata);
        }
        Value::Object(output)
    } else {
        let mut result = Map::new();
        result.insert("output".to_owned(), response.output);
        if has_image_candidates {
            insert_image_candidate_metadata(&mut result, candidates, dropped_candidates);
        } else if has_candidates {
            result.insert("candidates".to_owned(), Value::Array(candidates));
        }
        if has_cache {
            result.insert("cache".to_owned(), response.cache);
        }
        if let Some(execution_metadata) = execution_metadata {
            result.insert("_loomExecution".to_owned(), execution_metadata);
        }
        Value::Object(result)
    }
}

fn request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("loom-{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("loom-framework-process-{name}-{}", request_id()));
        fs::create_dir_all(&root).expect("create process test root");
        root
    }

    #[test]
    fn framework_image_candidates_use_canonical_loom_metadata() {
        let mut tool = ToolDefinition::new(
            "fixture-image-art",
            "Fixture Image Art",
            "Projects framework candidates for Hook.",
            crate::ToolExecution::FrameworkArt {
                framework: "mcp".to_owned(),
            },
        );
        tool.outputs = vec![json!({
            "name": "output",
            "type": "image",
            "execution_type": "image_buffer",
        })];
        let response: FrameworkExecuteResponse = serde_json::from_value(json!({
            "status": "success",
            "output": {
                "content": [{ "type": "image", "data": "fixture", "mimeType": "image/png" }],
                "selectedCandidate": "candidate-2",
            },
            "candidates": [
                { "id": "candidate-1", "index": 0, "data": "first" },
                { "id": "candidate-2", "index": 1, "data": "second" },
            ],
        }))
        .expect("framework response");

        let result = response_to_tool_value(&tool, response);
        assert!(result.get("candidates").is_none());
        assert_eq!(
            result["loomMetadata"]["candidates"]["kind"],
            "image.candidates"
        );
        assert_eq!(result["loomMetadata"]["candidates"]["selectedIndex"], 1);
        assert_eq!(
            result["loomMetadata"]["candidates"]["items"][1]["id"],
            "candidate-2"
        );
    }

    #[test]
    fn framework_image_candidates_are_normalized_to_the_consumer_wire_shape() {
        let mut tool = ToolDefinition::new(
            "fixture-image-art",
            "Fixture Image Art",
            "Projects framework candidates for Hook.",
            crate::ToolExecution::FrameworkArt {
                framework: "mcp".to_owned(),
            },
        );
        tool.outputs = vec![json!({
            "name": "output",
            "type": "image",
            "execution_type": "image_buffer",
        })];
        let response: FrameworkExecuteResponse = serde_json::from_value(json!({
            "status": "success",
            "output": {
                "content": [{ "type": "image", "data": "fixture", "mimeType": "image/png" }],
            },
            "candidates": [
                // The shape the shipped image-search Art emits: no `imageUrl`, and the source page
                // under `sourceUrl`.
                {
                    "id": "candidate-1",
                    "title": "first",
                    "thumbnail": "data:image/png;base64,AAA",
                    "data": "data:image/png;base64,AAA",
                    "sourceUrl": "https://example.test/one",
                    "width": 10,
                    "height": 20,
                },
                // An Art that already speaks the wire shape must pass through untouched.
                {
                    "id": "candidate-2",
                    "imageUrl": "https://example.test/two.png",
                    "sourcePageUrl": "https://example.test/two",
                    "index": 7,
                },
                // Nothing usable as an image reference: no key is invented.
                { "id": "candidate-3", "title": "third" },
            ],
        }))
        .expect("framework response");

        let result = response_to_tool_value(&tool, response);
        let items = &result["loomMetadata"]["candidates"]["items"];
        assert_eq!(items[0]["imageUrl"], "data:image/png;base64,AAA");
        assert_eq!(items[0]["thumbnailUrl"], "data:image/png;base64,AAA");
        assert_eq!(items[0]["sourcePageUrl"], "https://example.test/one");
        assert_eq!(items[0]["index"], 0);
        assert_eq!(items[0]["thumbnail"], "data:image/png;base64,AAA");
        assert_eq!(items[0]["id"], "candidate-1");
        assert_eq!(items[1]["imageUrl"], "https://example.test/two.png");
        assert_eq!(items[1]["sourcePageUrl"], "https://example.test/two");
        assert_eq!(items[1]["index"], 7);
        assert!(items[1].get("thumbnailUrl").is_none());
        assert!(items[2].get("imageUrl").is_none());
        assert_eq!(items[2]["index"], 2);
    }

    fn image_candidate_tool() -> ToolDefinition {
        let mut tool = ToolDefinition::new(
            "fixture-image-art",
            "Fixture Image Art",
            "Projects framework candidates for Hook.",
            crate::ToolExecution::FrameworkArt {
                framework: "mcp".to_owned(),
            },
        );
        tool.outputs = vec![json!({
            "name": "output",
            "type": "image",
            "execution_type": "image_buffer",
        })];
        tool
    }

    fn image_candidate_response(candidates: Vec<Value>) -> FrameworkExecuteResponse {
        serde_json::from_value(json!({
            "status": "success",
            "output": {
                "content": [{ "type": "image", "data": "fixture", "mimeType": "image/png" }],
            },
            "candidates": candidates,
        }))
        .expect("framework response")
    }

    #[test]
    fn framework_image_candidates_are_capped_by_item_count() {
        let candidates = (0..(MAX_FRAMEWORK_CANDIDATES * 3))
            .map(
                |index| json!({ "id": format!("candidate-{index}"), "imageUrl": "https://a.test" }),
            )
            .collect::<Vec<_>>();
        let result = response_to_tool_value(
            &image_candidate_tool(),
            image_candidate_response(candidates),
        );

        let metadata = &result["loomMetadata"]["candidates"];
        assert_eq!(
            metadata["items"].as_array().expect("items").len(),
            MAX_FRAMEWORK_CANDIDATES
        );
        assert_eq!(metadata["droppedItems"], MAX_FRAMEWORK_CANDIDATES * 2);
        assert_eq!(metadata["items"][0]["id"], "candidate-0");
    }

    #[test]
    fn framework_image_candidates_are_capped_by_total_bytes() {
        let payload = "d".repeat(MAX_FRAMEWORK_CANDIDATE_BYTES / 3);
        let candidates = (0..3)
            .map(|index| json!({ "id": format!("candidate-{index}"), "data": payload.clone() }))
            .collect::<Vec<_>>();
        let result = response_to_tool_value(
            &image_candidate_tool(),
            image_candidate_response(candidates),
        );

        let metadata = &result["loomMetadata"]["candidates"];
        assert_eq!(metadata["items"].as_array().expect("items").len(), 2);
        assert_eq!(metadata["droppedItems"], 1);
        assert_eq!(metadata["items"][1]["id"], "candidate-1");
    }

    #[test]
    fn a_single_oversized_framework_candidate_is_still_delivered() {
        let payload = "d".repeat(MAX_FRAMEWORK_CANDIDATE_BYTES + 1024);
        let result = response_to_tool_value(
            &image_candidate_tool(),
            image_candidate_response(vec![json!({ "id": "only", "data": payload })]),
        );

        let metadata = &result["loomMetadata"]["candidates"];
        assert_eq!(metadata["items"].as_array().expect("items").len(), 1);
        assert_eq!(metadata["droppedItems"], 0);
    }

    #[test]
    fn framework_image_output_drops_the_self_declared_base64_copy() {
        let root = temp_root("image-output-dedupe");
        let data_url = loom_image_io::rgba8_to_png_data_url(1, 1, &[255, 0, 0, 255])
            .expect("encode fixture png");
        let image_path = root.join("output.png");
        fs::write(
            &image_path,
            loom_image_io::decode_data_url_bytes(&data_url).expect("decode fixture png"),
        )
        .expect("write fixture png");

        let tool = image_candidate_tool();
        let mut output = json!({
            "output_base64": data_url,
            "outputBase64": "a stale second copy",
            "output_path": image_path.to_string_lossy(),
            "width": 1,
            "height": 1,
        });
        normalize_framework_image_output(&tool, "mcp", &mut output, &[root.as_path()])
            .expect("normalize image output");

        let output = output.as_object().expect("normalized output object");
        assert!(
            !output.contains_key("output_base64") && !output.contains_key("outputBase64"),
            "the host kept a self-declared base64 copy beside the content it built"
        );
        assert!(!output.contains_key("output_path"));
        assert_eq!(output["content"][0]["type"], "image");
        assert!(output["content"][0]["data"]
            .as_str()
            .expect("content data url")
            .starts_with("data:image/png;base64,"));
        assert_eq!(output["width"], 1);

        fs::remove_dir_all(&root).ok();
    }

    fn powershell_executable() -> PathBuf {
        for candidate in ["powershell.exe", "pwsh.exe"] {
            let output = Command::new(candidate)
                .args([
                    "-NoProfile",
                    "-Command",
                    "(Get-Command powershell.exe).Source",
                ])
                .output();
            if let Ok(output) = output {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                    if !path.is_empty() && Path::new(&path).is_file() {
                        return PathBuf::from(path);
                    }
                }
            }
        }
        panic!("PowerShell is required for the framework process fixture");
    }

    fn write_fixture_package(root: &Path, script: &str) -> PathBuf {
        let package_root = root.join("publisher.test").join("script");
        let package_dir = package_root.join("versions").join("0.1.0-fixture");
        let runtime_dir = package_dir.join("runtime");
        let art_dir = root.join("arts").join("fixture-art");
        fs::create_dir_all(&runtime_dir).expect("create fixture runtime");
        fs::create_dir_all(&art_dir).expect("create fixture art");
        fs::write(
            package_root.join("active.json"),
            serde_json::to_vec_pretty(&json!({ "active": "versions/0.1.0-fixture" })).unwrap(),
        )
        .expect("write fixture activation");
        fs::copy(powershell_executable(), runtime_dir.join("powershell.exe"))
            .expect("copy PowerShell fixture");
        fs::write(runtime_dir.join("fixture.ps1"), script).expect("write fixture script");
        fs::write(
            package_dir.join("framework.manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "id": "script",
                "name": "Fixture Script Framework",
                "description": "test",
                "version": "0.1.0",
                "publisher": { "id": "publisher.test", "name": "Publisher Test" },
                "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
                "platforms": ["windows-x64"],
                "entry": {
                    "kind": "process",
                    "command": "runtime/powershell.exe",
                    "args": ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "runtime/fixture.ps1"],
                    "processModel": "per_execution"
                },
                "permissions": ["process.spawn"],
                "resources": {
                    "stdoutMiB": 16,
                    "stderrMiB": 1
                },
                "artExecution": {
                    "requestSchema": "loom.art.execute.v1",
                    "responseSchema": "loom.art.result.v1"
                }
            }))
            .unwrap(),
        )
        .expect("write fixture manifest");
        art_dir
    }

    fn fixture_tool(art_dir: &Path) -> ToolDefinition {
        ToolDefinition {
            id: "fixture-art".to_owned(),
            name: "Fixture Art".to_owned(),
            description: "External framework fixture".to_owned(),
            enabled: true,
            execution: crate::ToolExecution::FrameworkArt {
                framework: "publisher.test/script".to_owned(),
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: Vec::new(),
            metadata: Some(json!({ "artPackage": { "dir": art_dir } })),
        }
    }

    fn fixture_tool_with_schema(art_dir: &Path) -> ToolDefinition {
        let mut tool = fixture_tool(art_dir);
        tool.inputs = vec![
            json!({ "name": "input", "type": "image" }),
            json!({ "name": "reference", "type": "image" }),
        ];
        tool.params = vec![json!({
            "id": "strength",
            "widget": "slider",
            "default": 100,
            "min": 0,
            "max": 100,
            "step": 1
        })];
        tool
    }

    fn fixture_image_tool(art_dir: &Path) -> ToolDefinition {
        let mut tool = fixture_tool(art_dir);
        tool.outputs = vec![json!({
            "name": "output",
            "type": "image",
            "execution_type": "image_buffer"
        })];
        tool
    }

    const SUCCESS_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$response = [ordered]@{ status = "success"; output = [ordered]@{ request = $request } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 50 -Compress))
"#;
    const ERROR_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$response = [ordered]@{ status = "error"; error = [ordered]@{ code = "quota_exhausted"; message = "quota exhausted"; detail = [string]$request.context.tempDir } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
    const INVALID_SCRIPT: &str = "[Console]::Out.Write('not-json')";
    const TIMEOUT_SCRIPT: &str =
        "Start-Sleep -Milliseconds 300; [Console]::Out.Write('{\"status\":\"success\"}')";
    const LARGE_OUTPUT_SCRIPT: &str = r#"
$large = "x" * (9 * 1024 * 1024)
$response = [ordered]@{ status = "success"; output = [ordered]@{ large = $large } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
    const PATH_IMAGE_OUTPUT_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$path = Join-Path ([string]$request.context.tempDir) "result.png"
$bytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP4DwQACfsD/Wj6HMwAAAAASUVORK5CYII=")
[System.IO.File]::WriteAllBytes($path, $bytes)
$response = [ordered]@{ status = "success"; output = [ordered]@{ output_path = $path; width = 1; height = 1 } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
    const OUTSIDE_PATH_IMAGE_OUTPUT_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$path = Join-Path ([string]$request.artDir) "outside.png"
$bytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP4DwQACfsD/Wj6HMwAAAAASUVORK5CYII=")
[System.IO.File]::WriteAllBytes($path, $bytes)
$response = [ordered]@{ status = "success"; output = [ordered]@{ output_path = $path } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;

    #[test]
    fn process_request_contains_art_inputs_params_and_context() {
        let root = temp_root("success");
        let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
        let result = execute_framework_art_in_root_with_timeout(
            &fixture_tool(&art_dir),
            "publisher.test/script",
            json!({
                "inputs": { "image": "input.png" },
                "params": { "strength": 0.5 },
                "disabledParams": ["unused"]
            }),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect("framework process success");
        assert_eq!(
            result["request"]["protocolVersion"],
            FRAMEWORK_PROTOCOL_VERSION
        );
        assert_eq!(result["request"]["frameworkId"], "script");
        assert_eq!(result["request"]["artId"], "fixture-art");
        assert_eq!(result["request"]["inputs"]["image"], "input.png");
        assert_eq!(result["request"]["params"]["strength"], 0.5);
        assert_eq!(result["request"]["disabledParams"][0], "unused");
        assert_eq!(
            result["request"]["artDir"],
            art_dir.to_string_lossy().to_string()
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn process_request_contains_art_scoped_credential_bindings() {
        let root = temp_root("credentials");
        let packages_root = root.join("frameworks");
        fs::create_dir_all(&packages_root).expect("create framework packages root");
        let art_dir = write_fixture_package(&packages_root, SUCCESS_SCRIPT);
        let mut tool = fixture_tool(&art_dir);
        tool.metadata
            .as_mut()
            .and_then(Value::as_object_mut)
            .expect("fixture metadata")
            .insert(
                "packageSecurity".to_owned(),
                json!({
                    "version": "1.0.0",
                    "publisher": { "id": "publisher.test", "name": "Publisher" }
                }),
            );
        let art_identity = tool.qualified_id();
        crate::art_settings::ArtSettingsStore::new(&root)
            .save(
                &art_identity,
                crate::art_settings::ArtUserSettings {
                    credential_bindings: BTreeMap::from([(
                        "api_key".to_owned(),
                        "stored-secret".to_owned(),
                    )]),
                    ..crate::art_settings::ArtUserSettings::default()
                },
            )
            .expect("persist fixture Art settings");
        crate::credentials::CredentialStore::new(&root)
            .upsert(crate::credentials::CredentialInput {
                name: "stored-secret".to_owned(),
                value: "fixture-value".to_owned(),
                value_type: crate::credentials::CredentialValueType::String,
                scope: crate::credentials::CredentialScope {
                    framework_id: None,
                    art_id: Some(art_identity.clone()),
                    mcp_server_id: None,
                },
                expires_at: None,
            })
            .expect("store fixture credential");
        let tool = crate::ToolRegistry::new(root.join("tools"))
            .save_tool(tool)
            .expect("save fixture tool with persisted settings");
        assert_eq!(
            tool.metadata
                .as_ref()
                .and_then(|metadata| metadata.pointer("/artUserSettings/credentialBindings/api_key"))
                .and_then(Value::as_str),
            Some("stored-secret")
        );

        let result = execute_framework_art_in_root_with_timeout(
            &tool,
            "publisher.test/script",
            json!({}),
            &packages_root,
            Duration::from_secs(10),
            None,
        )
        .expect("framework process credential binding");
        assert_eq!(
            result["request"]["context"]["credentials"][0]["name"],
            "api_key"
        );
        assert_eq!(
            result["request"]["context"]["credentials"][0]["value"],
            "fixture-value"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn mcp_framework_resolves_independent_package_and_server_scoped_credentials() {
        let root = temp_root("independent-mcp");
        fs::create_dir_all(root.join("mcp")).expect("create MCP store root");
        let mut server = loom_mcp::McpServerConfig::new(
            "neuro-image-search",
            "Image Search",
            root.join("mcp/server.ps1").display().to_string(),
        );
        server
            .credential_env
            .insert("BRAVE_API_KEY".to_owned(), "brave_api_key".to_owned());
        server
            .credential_bindings
            .insert("brave_api_key".to_owned(), "stored-image-key".to_owned());
        server
            .credential_requirements
            .push(loom_mcp::McpCredentialRequirement {
                id: "brave_api_key".to_owned(),
                label: "Brave API Key".to_owned(),
                required: true,
            });
        server.package = Some(loom_mcp::McpServerPackageState {
            qualified_id: "neuro.official/neuro-image-search".to_owned(),
            publisher_id: "neuro.official".to_owned(),
            version: "0.1.0".to_owned(),
            digest: "fixture".to_owned(),
            package_dir: root
                .join("mcp/packages/neuro.official/neuro-image-search/versions/0.1.0-fixture"),
            files: std::collections::BTreeMap::new(),
            trust_status: loom_protocol::PackageTrustStatus::Unsigned,
        });
        fs::write(
            root.join("mcp/servers.json"),
            serde_json::to_vec(&vec![server]).expect("serialize MCP store"),
        )
        .expect("write MCP store");
        crate::credentials::CredentialStore::new(&root)
            .upsert(crate::credentials::CredentialInput {
                name: "stored-image-key".to_owned(),
                value: "fixture-value".to_owned(),
                value_type: crate::credentials::CredentialValueType::String,
                scope: crate::credentials::CredentialScope {
                    framework_id: None,
                    art_id: None,
                    mcp_server_id: Some("neuro-image-search".to_owned()),
                },
                expires_at: None,
            })
            .expect("store MCP credential");
        let mut tool = ToolDefinition::new(
            "custom-image-search",
            "Image Search",
            "MCP consumer",
            crate::ToolExecution::FrameworkArt {
                framework: "mcp".to_owned(),
            },
        );
        tool.metadata = Some(json!({
            "packageSecurity": {
                "version": "0.4.0",
                "publisher": { "id": "neuro.official", "name": "Neuro" }
            },
            "mcp": {
                "serverId": "neuro-image-search",
                "packageId": "neuro.official/neuro-image-search",
                "version": "^0.1",
                "toolName": "brave_image_search"
            }
        }));
        let store = crate::credentials::CredentialStore::new(&root);
        let (resolved, credentials) =
            resolve_mcp_server(&tool, &root, Some(&store)).expect("resolve MCP dependency");
        assert_eq!(resolved.package_id, "neuro.official/neuro-image-search");
        assert_eq!(resolved.version, "0.1.0");
        assert_eq!(resolved.credential_env["BRAVE_API_KEY"], "brave_api_key");
        assert_eq!(credentials[0].name, "brave_api_key");
        assert_eq!(credentials[0].value, "fixture-value");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn flat_art_arguments_are_partitioned_by_manifest_schema() {
        let root = temp_root("flat-schema");
        let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
        let result = execute_framework_art_in_root_with_timeout(
            &fixture_tool_with_schema(&art_dir),
            "publisher.test/script",
            json!({
                "input": "source.png",
                "reference": "reference.png",
                "strength": 25
            }),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect("framework process success");

        assert_eq!(result["request"]["inputs"]["input"], "source.png");
        assert_eq!(result["request"]["inputs"]["reference"], "reference.png");
        assert_eq!(result["request"]["params"]["strength"], 25);
        assert!(result["request"]["inputs"].get("strength").is_none());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn execute_tool_routes_framework_art_to_the_external_process() {
        let _guard = ENV_LOCK.lock().expect("framework process env lock");
        let root = temp_root("execute-tool");
        let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
        let previous = std::env::var("LOOM_FRAMEWORK_PACKAGES_DIR").ok();
        std::env::set_var("LOOM_FRAMEWORK_PACKAGES_DIR", &root);
        let result = crate::execute_tool(
            &fixture_tool_with_schema(&art_dir),
            &[],
            json!({
                "input": "source.png",
                "reference": "reference.png",
                "strength": 40
            }),
        )
        .expect("execute_tool external framework route");
        assert_eq!(result["request"]["inputs"]["input"], "source.png");
        assert_eq!(result["request"]["inputs"]["reference"], "reference.png");
        assert_eq!(result["request"]["params"]["strength"], 40);
        match previous {
            Some(value) => std::env::set_var("LOOM_FRAMEWORK_PACKAGES_DIR", value),
            None => std::env::remove_var("LOOM_FRAMEWORK_PACKAGES_DIR"),
        }
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn process_error_preserves_code_message_and_detail() {
        let root = temp_root("error");
        let art_dir = write_fixture_package(&root, ERROR_SCRIPT);
        let error = execute_framework_art_in_root_with_timeout(
            &fixture_tool(&art_dir),
            "publisher.test/script",
            json!({}),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect_err("framework error response");
        let detail = match error {
            ToolRegistryError::FrameworkProcessFailed {
                code,
                message,
                detail,
                ..
            } if code == "quota_exhausted" && message == "quota exhausted" => detail,
            other => panic!("unexpected framework error: {other}"),
        };
        assert!(
            !Path::new(&detail).exists(),
            "framework temp directory leaked after an error: {detail}"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unsafe_framework_id_is_rejected_before_package_resolution() {
        let root = temp_root("unsafe-framework-id");
        let art_dir = root.join("arts").join("fixture-art");
        fs::create_dir_all(&art_dir).expect("create fixture art");
        let error = execute_framework_art_in_root_with_timeout(
            &fixture_tool(&art_dir),
            "../outside",
            json!({}),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect_err("unsafe framework id");
        assert!(matches!(
            error,
            ToolRegistryError::FrameworkProcessProtocol { reason, .. }
                if reason.contains("safe package id")
        ));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn invalid_process_response_is_a_structured_protocol_error() {
        let root = temp_root("invalid");
        let art_dir = write_fixture_package(&root, INVALID_SCRIPT);
        let error = execute_framework_art_in_root_with_timeout(
            &fixture_tool(&art_dir),
            "publisher.test/script",
            json!({}),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect_err("invalid framework response");
        assert!(matches!(
            error,
            ToolRegistryError::FrameworkProcessProtocol { reason, .. }
                if reason.contains("invalid JSON response")
        ));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn process_timeout_kills_the_framework_process() {
        let root = temp_root("timeout");
        let art_dir = write_fixture_package(&root, TIMEOUT_SCRIPT);
        let error = execute_framework_art_in_root_with_timeout(
            &fixture_tool(&art_dir),
            "publisher.test/script",
            json!({}),
            &root,
            Duration::from_millis(50),
            None,
        )
        .expect_err("framework timeout");
        assert!(matches!(
            error,
            ToolRegistryError::FrameworkProcessTimeout { timeout_ms: 50, .. }
        ));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn process_drains_large_stdout_without_deadlocking() {
        let root = temp_root("large-stdout");
        let art_dir = write_fixture_package(&root, LARGE_OUTPUT_SCRIPT);
        let result = execute_framework_art_in_root_with_timeout(
            &fixture_tool(&art_dir),
            "publisher.test/script",
            json!({}),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect("large framework response");
        assert_eq!(
            result["large"].as_str().map(str::len),
            Some(9 * 1024 * 1024)
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn process_normalizes_image_paths_before_the_temp_directory_is_removed() {
        let root = temp_root("path-image-output");
        let art_dir = write_fixture_package(&root, PATH_IMAGE_OUTPUT_SCRIPT);
        let result = execute_framework_art_in_root_with_timeout(
            &fixture_image_tool(&art_dir),
            "publisher.test/script",
            json!({}),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect("path image output");

        assert!(result.get("output_path").is_none());
        assert_eq!(result["width"], 1);
        assert_eq!(result["height"], 1);
        assert!(result["content"][0]["data"]
            .as_str()
            .is_some_and(|value| value.starts_with("data:image/png;base64,")));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn process_rejects_image_paths_outside_execution_output_roots() {
        let root = temp_root("outside-path-image-output");
        let art_dir = write_fixture_package(&root, OUTSIDE_PATH_IMAGE_OUTPUT_SCRIPT);
        let error = execute_framework_art_in_root_with_timeout(
            &fixture_image_tool(&art_dir),
            "publisher.test/script",
            json!({}),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect_err("outside path rejected");

        assert!(matches!(
            error,
            ToolRegistryError::FrameworkProcessProtocol { reason, .. }
                if reason.contains("outside the execution output roots")
        ));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn framework_art_requires_installed_package_directory_metadata() {
        let root = temp_root("missing-art-directory");
        let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
        let mut tool = fixture_tool(&art_dir);
        tool.metadata = Some(json!({}));
        let error = execute_framework_art_in_root_with_timeout(
            &tool,
            "publisher.test/script",
            json!({}),
            &root,
            Duration::from_secs(10),
            None,
        )
        .expect_err("missing artPackage.dir must fail closed");

        assert!(matches!(
            error,
            ToolRegistryError::FrameworkArtDirectoryNotFound { path, .. }
                if path == "<metadata.artPackage.dir>"
        ));
        fs::remove_dir_all(root).ok();
    }

    /// The third budget S9-1 asked for: wall time for one whole art execution. This is the number a
    /// user feels, and every performance finding in the review that touches the framework path ends
    /// up here — resolving the package, building the request, spawning the interpreter, writing
    /// stdin, reading the response back and normalising it.
    ///
    /// The art is a fixture that echoes its request rather than one of the shipped sample packages,
    /// because a sample package has to be built before it can run and this budget has to hold on
    /// every push. What the fixture does keep is everything expensive: a real package on disk, a real
    /// interpreter process, and the real supervisor. The framework's own work is the part the fixture
    /// leaves out, and no budget here could bound that anyway.
    ///
    /// The measured execution is the second one. A framework package is installed once and executed
    /// many times, so the warm case is the representative one; the first execution also pays for the
    /// operating system caching the interpreter this test copied a moment earlier, which is an
    /// artefact of the fixture rather than a cost a deployment pays per execution.
    #[test]
    fn one_art_execution_stays_within_its_wall_time_budget() {
        // Measured at 1,562 ms warm on 2026-08-22. The ceiling is far above that because wall time is
        // the one budget that has to survive a shared CI runner competing with other jobs; it is set
        // to catch an execution that has started spawning twice or waiting on a network round trip,
        // not to track interpreter startup drift.
        const BUDGET_MS: u64 = 10_000;

        let root = temp_root("perf-wall-time");
        let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
        let tool = fixture_tool(&art_dir);
        let execute = || {
            execute_framework_art_in_root_with_timeout(
                &tool,
                "publisher.test/script",
                json!({ "inputs": { "image": "input.png" } }),
                &root,
                Duration::from_secs(60),
                None,
            )
        };

        execute().expect("warm the fixture package");
        let started = std::time::Instant::now();
        execute().expect("measured art execution");
        let elapsed = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        loom_perf::assert_within("art_execution_wall_time_ms", "ms", elapsed, BUDGET_MS);
        fs::remove_dir_all(root).ok();
    }
}
