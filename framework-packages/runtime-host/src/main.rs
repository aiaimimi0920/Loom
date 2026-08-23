use std::env;
use std::fs;
use std::io::{BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};

use loom_process::ProcessSpec;
use loom_protocol::{
    ArtRuntimeManifest, FrameworkExecuteRequest, ART_RUNTIME_PROTOCOL_VERSION,
    FRAMEWORK_PROTOCOL_VERSION,
};
use serde_json::{json, Value};

mod mcp;

const MAX_REQUEST_BYTES: u64 = 32 * 1024 * 1024;
// Art runtimes commonly return image payloads as base64 inside a JSON response.
// Keep the outer framework process limit bounded, but allow the nested runtime
// to return a full-size image without being killed at the historical 8 MiB cap.
const MAX_ART_RUNTIME_STDOUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_ART_RUNTIME_STDERR_BYTES: usize = 8 * 1024 * 1024;

fn main() {
    if let Err(error) = run() {
        write_framework_error(error);
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
    if parse_serve_mode() {
        return serve_requests(expected_framework.as_deref());
    }
    let request_text = read_request()?;
    run_request(&request_text, expected_framework.as_deref())
}

fn run_request(request_text: &str, expected_framework: Option<&str>) -> Result<(), String> {
    let request: FrameworkExecuteRequest = serde_json::from_str(request_text.trim())
        .map_err(|error| format!("invalid framework request JSON: {error}"))?;
    if request.protocol_version != FRAMEWORK_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported framework protocol: {}",
            request.protocol_version
        ));
    }
    let framework_id = request.framework_id.as_str();
    if let Some(expected) = expected_framework {
        if expected != framework_id {
            return Err(format!(
                "framework runtime was built for `{expected}` but request targets `{framework_id}`"
            ));
        }
    }
    let art_dir = request.art_dir.clone();
    if !art_dir.is_dir() {
        return Err(format!(
            "Art directory does not exist: {}",
            art_dir.display()
        ));
    }

    let runtime_payload = if framework_id == "mcp" {
        let execution = mcp::execute(&request, &art_dir)?;
        let mut payload = serde_json::to_value(&request)
            .map_err(|error| format!("cannot serialize framework request: {error}"))?;
        if let Some(context) = payload.get_mut("context").and_then(Value::as_object_mut) {
            context.insert("credentials".to_owned(), Value::Array(Vec::new()));
        }
        payload
            .as_object_mut()
            .expect("framework request serializes as an object")
            .insert("frameworkData".to_owned(), json!({ "mcp": execution }));
        payload
    } else {
        serde_json::from_str(request_text.trim())
            .map_err(|error| format!("invalid framework request JSON: {error}"))?
    };

    let runtime_manifest_path = art_dir.join("art.runtime.json");
    let runtime_manifest: ArtRuntimeManifest =
        serde_json::from_slice(&fs::read(&runtime_manifest_path).map_err(|error| {
            format!("cannot read {}: {error}", runtime_manifest_path.display())
        })?)
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
    process.limits.stdout_bytes = MAX_ART_RUNTIME_STDOUT_BYTES;
    process.limits.stderr_bytes = MAX_ART_RUNTIME_STDERR_BYTES;
    configure_packaged_runtimes(&mut process);
    let mut payload = serde_json::to_vec(&runtime_payload)
        .map_err(|error| format!("cannot serialize Art runtime request: {error}"))?;
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
                object.entry("diagnostics".to_owned()).or_insert_with(|| {
                    serde_json::to_value(&output.diagnostics).unwrap_or_default()
                });
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

fn serve_requests(expected_framework: Option<&str>) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("failed to read framework request: {error}"))?;
        if read == 0 {
            return Ok(());
        }
        if line.len() as u64 > MAX_REQUEST_BYTES {
            write_framework_error(format!(
                "framework request exceeds {} bytes",
                MAX_REQUEST_BYTES
            ));
            continue;
        }
        let request = match std::str::from_utf8(&line) {
            Ok(request) if !request.trim().is_empty() => request,
            Ok(_) => continue,
            Err(error) => {
                write_framework_error(format!("framework request is not UTF-8: {error}"));
                continue;
            }
        };
        if let Err(error) = run_request(request, expected_framework) {
            write_framework_error(error);
        }
    }
}

fn configure_packaged_runtimes(process: &mut ProcessSpec) {
    let Some(package_root) = env::current_exe()
        .ok()
        .and_then(|path| path.parent().and_then(Path::parent).map(Path::to_path_buf))
    else {
        return;
    };
    let python_root = package_root.join("python-embed");
    if !python_root.is_dir() {
        return;
    }
    let mut paths = vec![python_root.clone()];
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(path) = env::join_paths(paths) {
        process
            .env
            .insert("PATH".to_owned(), path.to_string_lossy().into_owned());
    }
    process.env.insert(
        "LOOM_PYTHON".to_owned(),
        python_root
            .join("python.exe")
            .to_string_lossy()
            .into_owned(),
    );
    process
        .env
        .insert("PYTHONDONTWRITEBYTECODE".to_owned(), "1".to_owned());
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

fn parse_serve_mode() -> bool {
    env::args().skip(1).any(|arg| arg == "--serve")
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
            format!(
                "cannot resolve Art runtime command {}: {error}",
                candidate.display()
            )
        })?;
        if !canonical_candidate.starts_with(&canonical_art_dir) {
            return Err("Art runtime command resolves outside the Art package".to_owned());
        }
        return Ok(canonical_candidate);
    }
    Ok(PathBuf::from(command))
}

fn write_response(value: Value) {
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{}", value);
    let _ = stdout.flush();
}

fn write_framework_error(error: String) {
    write_response(json!({
        "status": "error",
        "error": {
            "code": "framework_runtime_host_error",
            "message": error
        }
    }));
}
