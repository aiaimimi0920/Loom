//! Generic stdin/stdout bridge for externally packaged Art frameworks.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::framework::{FrameworkPackageManifest, FRAMEWORK_PROTOCOL_VERSION};
use crate::{ToolDefinition, ToolRegistryError, ToolRegistryResult};

pub const DEFAULT_FRAMEWORK_PROCESS_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteRequest {
    pub protocol_version: String,
    pub framework_id: String,
    pub art_id: String,
    pub art_dir: PathBuf,
    pub inputs: Value,
    pub params: Value,
    pub disabled_params: Vec<String>,
    pub context: FrameworkExecutionContext,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecutionContext {
    pub request_id: String,
    pub cache_dir: PathBuf,
    pub temp_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteResponse {
    pub status: String,
    #[serde(default)]
    pub output: Value,
    #[serde(default)]
    pub error: Option<FrameworkExecuteError>,
    #[serde(default)]
    pub candidates: Vec<Value>,
    #[serde(default)]
    pub cache: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
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
    )
}

fn execute_framework_art_in_root_with_timeout(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
    packages_root: &Path,
    timeout: Duration,
) -> ToolRegistryResult<Value> {
    let package_dir = packages_root.join(framework);
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
    if manifest.id != framework || manifest.protocol_version != FRAMEWORK_PROTOCOL_VERSION {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!(
                "manifest identity/protocol mismatch: id={}, protocol={}",
                manifest.id, manifest.protocol_version
            ),
        });
    }
    if manifest.entry.kind != "process" || manifest.entry.command.trim().is_empty() {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "manifest entry must be a process with a command".to_owned(),
        });
    }
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
    let cache_dir = art_dir.join(".loom-cache");
    let temp_dir = std::env::temp_dir()
        .join("loom-framework")
        .join(&request_id);
    fs::create_dir_all(&cache_dir).map_err(|error| framework_io_error(tool, framework, error))?;
    fs::create_dir_all(&temp_dir).map_err(|error| framework_io_error(tool, framework, error))?;

    let (inputs, params, disabled_params) = split_arguments(&arguments);
    let request = FrameworkExecuteRequest {
        protocol_version: FRAMEWORK_PROTOCOL_VERSION.to_owned(),
        framework_id: framework.to_owned(),
        art_id: tool.id.clone(),
        art_dir: art_dir.clone(),
        inputs,
        params,
        disabled_params,
        context: FrameworkExecutionContext {
            request_id,
            cache_dir,
            temp_dir: temp_dir.clone(),
        },
    };
    let payload = serde_json::to_vec(&request).map_err(|error| {
        ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!("cannot serialize request: {error}"),
        }
    })?;

    let mut child = Command::new(&command_path)
        .args(&manifest.entry.args)
        .current_dir(&package_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| ToolRegistryError::FrameworkProcessSpawn {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: error.to_string(),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&payload)
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|error| framework_io_error(tool, framework, error))?;
    }

    let deadline = Instant::now() + timeout;
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(ToolRegistryError::FrameworkProcessTimeout {
                    id: tool.id.clone(),
                    framework: framework.to_owned(),
                    timeout_ms: timeout.as_millis(),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(framework_io_error(tool, framework, error));
            }
        }
    };

    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_end(&mut stdout)
            .map_err(|error| framework_io_error(tool, framework, error))?;
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)
            .map_err(|error| framework_io_error(tool, framework, error))?;
    }
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
    let response: FrameworkExecuteResponse =
        serde_json::from_str(&stdout_text).map_err(|error| {
            ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: format!("invalid JSON response: {error}; stdout: {stdout_text}"),
            }
        })?;
    let status = response.status.trim().to_ascii_lowercase();
    if !matches!(status.as_str(), "success" | "ok" | "completed") {
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

    let result = response_to_tool_value(response);
    let _ = fs::remove_dir_all(&temp_dir);
    Ok(result)
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

fn split_arguments(arguments: &Value) -> (Value, Value, Vec<String>) {
    let Some(object) = arguments.as_object() else {
        return (arguments.clone(), Value::Object(Map::new()), Vec::new());
    };
    let inputs = object
        .get("inputs")
        .cloned()
        .unwrap_or_else(|| arguments.clone());
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
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
    (inputs, params, disabled)
}

fn response_to_tool_value(response: FrameworkExecuteResponse) -> Value {
    let has_candidates = !response.candidates.is_empty();
    let has_cache = !response.cache.is_null();
    if !has_candidates && !has_cache {
        return response.output;
    }
    if let Value::Object(mut output) = response.output {
        if has_candidates {
            output.insert("candidates".to_owned(), Value::Array(response.candidates));
        }
        if has_cache {
            output.insert("cache".to_owned(), response.cache);
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

    const SUCCESS_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$response = [ordered]@{ status = "success"; output = [ordered]@{ request = $request } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 50 -Compress))
"#;
    const ERROR_SCRIPT: &str = r#"
$response = [ordered]@{ status = "error"; error = [ordered]@{ code = "quota_exhausted"; message = "quota exhausted"; detail = "fixture detail" } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
    const INVALID_SCRIPT: &str = "[Console]::Out.Write('not-json')";
    const TIMEOUT_SCRIPT: &str =
        "Start-Sleep -Milliseconds 300; [Console]::Out.Write('{\"status\":\"success\"}')";

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
    fn execute_tool_routes_framework_art_to_the_external_process() {
        let _guard = ENV_LOCK.lock().expect("framework process env lock");
        let root = temp_root("execute-tool");
        let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
        let previous = std::env::var("LOOM_FRAMEWORK_PACKAGES_DIR").ok();
        std::env::set_var("LOOM_FRAMEWORK_PACKAGES_DIR", &root);
        let result = crate::execute_tool(
            &fixture_tool(&art_dir),
            &[],
            json!({ "inputs": { "text": "through execute_tool" } }),
        )
        .expect("execute_tool external framework route");
        assert_eq!(result["request"]["inputs"]["text"], "through execute_tool");
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
        )
        .expect_err("framework error response");
        assert!(matches!(
            error,
            ToolRegistryError::FrameworkProcessFailed {
                code,
                message,
                detail,
                ..
            } if code == "quota_exhausted" && message == "quota exhausted" && detail == "fixture detail"
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
        )
        .expect_err("framework timeout");
        assert!(matches!(
            error,
            ToolRegistryError::FrameworkProcessTimeout { timeout_ms: 50, .. }
        ));
        fs::remove_dir_all(root).ok();
    }
}
