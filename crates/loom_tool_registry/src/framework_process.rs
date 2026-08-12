//! Generic stdin/stdout bridge for externally packaged Art frameworks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use loom_process::{ProcessError, ProcessSpec};
use serde_json::{json, Map, Value};

use crate::framework::{
    enforce_framework_permission_policy, is_valid_framework, resolve_framework_package_dir,
    FrameworkPackageManifest, FRAMEWORK_PROTOCOL_VERSION,
};
use crate::{ToolDefinition, ToolRegistryError, ToolRegistryResult};

pub use loom_protocol::{
    FrameworkExecuteError, FrameworkExecuteRequest, FrameworkExecuteResponse,
    FrameworkExecutionContext,
};

pub const DEFAULT_FRAMEWORK_PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES: u64 = 256 * 1024 * 1024;

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
    if !is_valid_framework(framework) {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "framework id is not a safe package id".to_owned(),
        });
    }

    let package_dir = resolve_framework_package_dir(packages_root, framework)
        .unwrap_or_else(|| packages_root.join(framework));
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
    if manifest.id != framework {
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
            path: format!("<control-plane>/arts/{}", tool.id),
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
    let mut credentials = credential_store
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
    if let (Some(store), bindings) = (credential_store.as_ref(), art_credential_bindings(tool)) {
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
    let request = FrameworkExecuteRequest {
        protocol_version: negotiated_protocol.to_owned(),
        supported_protocol_versions: vec![FRAMEWORK_PROTOCOL_VERSION.to_owned()],
        framework_id: framework.to_owned(),
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
            detail: stderr.trim().to_owned(),
        });
    }
    let mut response: FrameworkExecuteResponse =
        serde_json::from_str(&stdout_text).map_err(|error| {
            ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: format!("invalid JSON response: {error}; stdout: {stdout_text}"),
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

    Ok(response_to_tool_value(response))
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
    let data_url =
        loom_image_io::read_image_path_as_data_url(&canonical_path).map_err(|error| {
            framework_image_output_error(
                tool,
                framework,
                format!("cannot decode image output: {error}"),
            )
        })?;
    let content = json!([{
        "type": "image",
        "data": data_url,
        "mimeType": "image/png"
    }]);
    match output {
        Value::Object(object) => {
            for key in ["output_path", "outputPath", "file_path", "filePath", "path"] {
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
                String::from_utf8_lossy(&stderr)
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
    ["LOOM_FRAMEWORK_PACKAGES_DIR", "LOOM_FRAMEWORK_RUNTIMES_DIR"]
        .into_iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
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
        .and_then(Value::as_str)
        .or_else(|| metadata.get("artDir").and_then(Value::as_str));
    package.map(PathBuf::from).or_else(|| {
        std::env::var("LOOM_CONTROL_PLANE_ROOT")
            .ok()
            .map(|root| PathBuf::from(root).join("arts").join(&tool.id))
    })
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

fn split_arguments(tool: &ToolDefinition, arguments: &Value) -> (Value, Value, Vec<String>) {
    let Some(object) = arguments.as_object() else {
        return (arguments.clone(), Value::Object(Map::new()), Vec::new());
    };
    let disabled = object
        .get("disabledParams")
        .or_else(|| object.get("disabled_params"))
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
        .filter_map(|parameter| {
            parameter
                .get("id")
                .or_else(|| parameter.get("name"))
                .and_then(Value::as_str)
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut inputs = Map::new();
    let mut params = Map::new();
    for (key, value) in object {
        if matches!(key.as_str(), "disabledParams" | "disabled_params") {
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

fn response_to_tool_value(response: FrameworkExecuteResponse) -> Value {
    let has_candidates = !response.candidates.is_empty();
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
        if has_candidates {
            output.insert("candidates".to_owned(), Value::Array(response.candidates));
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
        if has_candidates {
            result.insert("candidates".to_owned(), Value::Array(response.candidates));
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
        let package_dir = root.join("script");
        let runtime_dir = package_dir.join("runtime");
        let art_dir = root.join("arts").join("fixture-art");
        fs::create_dir_all(&runtime_dir).expect("create fixture runtime");
        fs::create_dir_all(&art_dir).expect("create fixture art");
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
                "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
                "platforms": ["windows-x64"],
                "entry": {
                    "kind": "process",
                    "command": "runtime/powershell.exe",
                    "args": ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "runtime/fixture.ps1"]
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
                framework: "script".to_owned(),
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
            "script",
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
                "artUserSettings".to_owned(),
                json!({ "credentialBindings": { "api_key": "stored-secret" } }),
            );
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
        crate::credentials::CredentialStore::new(&root)
            .upsert(crate::credentials::CredentialInput {
                name: "stored-secret".to_owned(),
                value: "fixture-value".to_owned(),
                value_type: crate::credentials::CredentialValueType::String,
                scope: crate::credentials::CredentialScope {
                    framework_id: None,
                    art_id: Some(tool.qualified_id()),
                },
                expires_at: None,
            })
            .expect("store fixture credential");

        let result = execute_framework_art_in_root_with_timeout(
            &tool,
            "script",
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
    fn flat_art_arguments_are_partitioned_by_manifest_schema() {
        let root = temp_root("flat-schema");
        let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
        let result = execute_framework_art_in_root_with_timeout(
            &fixture_tool_with_schema(&art_dir),
            "script",
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
            "script",
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
            "script",
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
            "script",
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
            "script",
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
            "script",
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
            "script",
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
}
