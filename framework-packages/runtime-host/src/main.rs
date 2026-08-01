use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use loom_protocol::{
    ArtRuntimeManifest, FrameworkExecuteRequest, ART_RUNTIME_PROTOCOL_VERSION,
    FRAMEWORK_PROTOCOL_VERSION,
};
use loom_process::ProcessSpec;
use serde_json::{json, Value};

const MAX_REQUEST_BYTES: u64 = 32 * 1024 * 1024;

fn main() {
    if let Err(error) = run() {
        write_response(json!({
            "status": "error",
            "error": {
                "code": "framework_runtime_host_error",
                "message": error
            }
        }));
    }
}

fn run() -> Result<(), String> {
    let expected_framework = parse_framework_id();
    if let Some(command) = parse_health_check_command() {
        write_response(json!({
            "status": "success",
            "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
            "output": {
                "healthy": true,
                "command": command,
                "frameworkId": expected_framework
            }
        }));
        return Ok(());
    }
    let request_text = read_request()?;
    let request: FrameworkExecuteRequest = serde_json::from_str(request_text.trim())
        .map_err(|error| format!("invalid framework request JSON: {error}"))?;
    if request.protocol_version != FRAMEWORK_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported framework protocol: {}",
            request.protocol_version
        ));
    }
    let framework_id = request.framework_id.as_str();
    if let Some(expected) = expected_framework.as_deref() {
        if expected != framework_id {
            return Err(format!(
                "framework runtime was built for `{expected}` but request targets `{framework_id}`"
            ));
        }
    }
    let art_dir = request.art_dir;
    if !art_dir.is_dir() {
        return Err(format!("Art directory does not exist: {}", art_dir.display()));
    }

    let runtime_manifest_path = art_dir.join("art.runtime.json");
    let runtime_manifest: ArtRuntimeManifest = serde_json::from_slice(
        &fs::read(&runtime_manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", runtime_manifest_path.display()))?,
    )
    .map_err(|error| format!("invalid {}: {error}", runtime_manifest_path.display()))?;
    if runtime_manifest.protocol_version != ART_RUNTIME_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported Art runtime protocol: {}",
            runtime_manifest.protocol_version
        ));
    }
    let command = runtime_manifest.entry.command;
    if command.trim().is_empty() {
        return Err("art.runtime.json entry.command is required".to_owned());
    }
    let args = runtime_manifest.entry.args;
    let command_path = resolve_command(&art_dir, &command)?;

    let mut process = ProcessSpec::new(&command_path);
    process.args = args;
    process.current_dir = Some(art_dir.clone());
    let mut payload = request_text.trim().as_bytes().to_vec();
    payload.push(b'\n');
    let output = loom_process::run_with_input(&process, &payload)
        .map_err(|error| format!("Art runtime `{command}` failed: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !output.status.success() {
        write_response(json!({
            "status": "error",
            "error": {
                "code": "art_runtime_failed",
                "message": format!("Art runtime exited with code {:?}", output.status.code()),
                "detail": stderr
            },
            "diagnostics": output.diagnostics
        }));
        return Ok(());
    }
    if stdout.is_empty() {
        return Err("Art runtime returned no stdout".to_owned());
    }
    match serde_json::from_str::<Value>(&stdout) {
        Ok(mut value) if value.get("status").is_some() => {
            if let Some(object) = value.as_object_mut() {
                object
                    .entry("diagnostics".to_owned())
                    .or_insert_with(|| serde_json::to_value(&output.diagnostics).unwrap_or_default());
            }
            write_response(value)
        }
        Ok(value) => write_response(json!({
            "status": "success",
            "output": value,
            "diagnostics": output.diagnostics
        })),
        Err(_) => write_response(json!({
            "status": "success",
            "output": {
                "content": [{ "type": "text", "text": stdout }]
            },
            "diagnostics": output.diagnostics
        })),
    }
    Ok(())
}

fn parse_framework_id() -> Option<String> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--framework-id" {
            return args.next();
        }
    }
    None
}

fn parse_health_check_command() -> Option<String> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--loom-health-check" {
            return Some(args.next().unwrap_or_else(|| "self_test".to_owned()));
        }
    }
    None
}

fn read_request() -> Result<String, String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to read framework request: {error}"))?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err(format!(
            "framework request exceeds {} bytes",
            MAX_REQUEST_BYTES
        ));
    }
    String::from_utf8(bytes).map_err(|error| format!("framework request is not UTF-8: {error}"))
}

fn resolve_command(art_dir: &Path, command: &str) -> Result<PathBuf, String> {
    let path = Path::new(command);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Art runtime command must be a safe relative path or command name".to_owned());
    }
    if path.components().count() > 1 {
        let candidate = art_dir.join(path);
        let canonical_art_dir = fs::canonicalize(art_dir)
            .map_err(|error| format!("cannot resolve Art directory: {error}"))?;
        let canonical_candidate = fs::canonicalize(&candidate).map_err(|error| {
            format!("cannot resolve Art runtime command {}: {error}", candidate.display())
        })?;
        if !canonical_candidate.starts_with(&canonical_art_dir) {
            return Err("Art runtime command resolves outside the Art package".to_owned());
        }
        return Ok(canonical_candidate);
    }
    Ok(PathBuf::from(command))
}

fn write_response(value: Value) {
    let _ = writeln!(std::io::stdout(), "{}", value);
}
