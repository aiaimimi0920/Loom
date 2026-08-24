use super::super::*;
use std::fs;
use std::path::PathBuf;
#[cfg(windows)]
use std::process::Command;
#[cfg(windows)]
use std::sync::Mutex;

// Execution fixtures intentionally model a windows-x64 framework package and copy PowerShell into
// that package. Tests that launch the fixture are Windows-only; pure validation stays cross-platform.
#[cfg(windows)]
pub(super) static ENV_LOCK: Mutex<()> = Mutex::new(());

#[cfg(windows)]
pub(super) struct EnvVarGuard {
    name: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[cfg(windows)]
impl EnvVarGuard {
    pub(super) fn set(name: &'static str, value: &Path) -> Self {
        let previous = std::env::var_os(name);
        std::env::set_var(name, value);
        Self { name, previous }
    }
}

#[cfg(windows)]
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(self.name, value),
            None => std::env::remove_var(self.name),
        }
    }
}

pub(super) fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("loom-framework-process-{name}-{}", request_id()));
    fs::create_dir_all(&root).expect("create process test root");
    root
}

#[cfg(windows)]
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

#[cfg(windows)]
pub(super) fn write_fixture_package(root: &Path, script: &str) -> PathBuf {
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

pub(super) fn fixture_tool(art_dir: &Path) -> ToolDefinition {
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

#[cfg(windows)]
pub(super) fn fixture_tool_with_schema(art_dir: &Path) -> ToolDefinition {
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

#[cfg(windows)]
pub(super) fn fixture_image_tool(art_dir: &Path) -> ToolDefinition {
    let mut tool = fixture_tool(art_dir);
    tool.outputs = vec![json!({
        "name": "output",
        "type": "image",
        "execution_type": "image_buffer"
    })];
    tool
}

#[cfg(windows)]
pub(super) const SUCCESS_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$response = [ordered]@{ status = "success"; output = [ordered]@{ request = $request } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 50 -Compress))
"#;
#[cfg(windows)]
pub(super) const ERROR_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$response = [ordered]@{ status = "error"; error = [ordered]@{ code = "quota_exhausted"; message = "quota exhausted"; detail = [string]$request.context.tempDir } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
#[cfg(windows)]
pub(super) const INVALID_SCRIPT: &str = "[Console]::Out.Write('not-json')";
#[cfg(windows)]
pub(super) const TIMEOUT_SCRIPT: &str =
    "Start-Sleep -Milliseconds 300; [Console]::Out.Write('{\"status\":\"success\"}')";
#[cfg(windows)]
pub(super) const LARGE_OUTPUT_SCRIPT: &str = r#"
$large = "x" * (9 * 1024 * 1024)
$response = [ordered]@{ status = "success"; output = [ordered]@{ large = $large } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
#[cfg(windows)]
pub(super) const PATH_IMAGE_OUTPUT_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$path = Join-Path ([string]$request.context.tempDir) "result.png"
$bytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP4DwQACfsD/Wj6HMwAAAAASUVORK5CYII=")
[System.IO.File]::WriteAllBytes($path, $bytes)
$response = [ordered]@{ status = "success"; output = [ordered]@{ output_path = $path; width = 1; height = 1 } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
#[cfg(windows)]
pub(super) const OUTSIDE_PATH_IMAGE_OUTPUT_SCRIPT: &str = r#"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$path = Join-Path ([string]$request.artDir) "outside.png"
$bytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP4DwQACfsD/Wj6HMwAAAAASUVORK5CYII=")
[System.IO.File]::WriteAllBytes($path, $bytes)
$response = [ordered]@{ status = "success"; output = [ordered]@{ output_path = $path } }
[Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
"#;
