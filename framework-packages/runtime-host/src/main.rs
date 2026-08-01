use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

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
    let mut request_text = String::new();
    std::io::stdin()
        .read_to_string(&mut request_text)
        .map_err(|error| format!("failed to read framework request: {error}"))?;
    let request: Value = serde_json::from_str(request_text.trim())
        .map_err(|error| format!("invalid framework request JSON: {error}"))?;
    let framework_id = request
        .get("frameworkId")
        .and_then(Value::as_str)
        .ok_or_else(|| "frameworkId is required".to_owned())?;
    if let Some(expected) = expected_framework.as_deref() {
        if expected != framework_id {
            return Err(format!(
                "framework runtime was built for `{expected}` but request targets `{framework_id}`"
            ));
        }
    }
    let art_dir = request
        .get("artDir")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "artDir is required".to_owned())?;
    if !art_dir.is_dir() {
        return Err(format!("Art directory does not exist: {}", art_dir.display()));
    }

    let runtime_manifest_path = art_dir.join("art.runtime.json");
    let runtime_manifest: Value = serde_json::from_slice(
        &fs::read(&runtime_manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", runtime_manifest_path.display()))?,
    )
    .map_err(|error| format!("invalid {}: {error}", runtime_manifest_path.display()))?;
    let entry = runtime_manifest
        .get("entry")
        .and_then(Value::as_object)
        .ok_or_else(|| "art.runtime.json entry is required".to_owned())?;
    let command = entry
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| "art.runtime.json entry.command is required".to_owned())?;
    let args = entry
        .get("args")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let command_path = resolve_command(&art_dir, command);

    let mut child = Command::new(&command_path)
        .args(args)
        .current_dir(&art_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start Art runtime `{command}`: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request_text.trim().as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|error| format!("failed to send Art runtime request: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for Art runtime: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !output.status.success() {
        write_response(json!({
            "status": "error",
            "error": {
                "code": "art_runtime_failed",
                "message": format!("Art runtime exited with code {:?}", output.status.code()),
                "detail": stderr
            }
        }));
        return Ok(());
    }
    if stdout.is_empty() {
        return Err("Art runtime returned no stdout".to_owned());
    }
    match serde_json::from_str::<Value>(&stdout) {
        Ok(value) if value.get("status").is_some() => write_response(value),
        Ok(value) => write_response(json!({ "status": "success", "output": value })),
        Err(_) => write_response(json!({
            "status": "success",
            "output": {
                "content": [{ "type": "text", "text": stdout }]
            }
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

fn resolve_command(art_dir: &Path, command: &str) -> PathBuf {
    let path = Path::new(command);
    if path.is_absolute() || path.components().count() > 1 {
        let candidate = art_dir.join(path);
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(command)
}

fn write_response(value: Value) {
    let _ = writeln!(std::io::stdout(), "{}", value);
}
