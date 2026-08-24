//! Shared framework registry test fixtures.
use super::*;

use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
pub(super) fn create_directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("create directory symlink");
}

#[cfg(windows)]
pub(super) fn create_directory_link(target: &Path, link: &Path) {
    // `cmd.exe` treats forward slashes embedded in an argument as switches;
    // activation paths are deliberately normalized with `/`, so normalize the
    // fixture paths back to native separators before invoking `mklink`.
    let target = target.to_string_lossy().replace('/', "\\");
    let link = link.to_string_lossy().replace('/', "\\");
    let output = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .output()
        .expect("run mklink");
    assert!(
        output.status.success(),
        "create directory junction: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn temp_root() -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "loom-frameworks-test-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        TEMP_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&base).unwrap();
    base
}
pub(super) fn fake_framework_package_zip(id: &str) -> Vec<u8> {
    fake_framework_package_zip_with_version(id, "0.1.0")
}

pub(super) fn fake_framework_package_zip_with_version(id: &str, version: &str) -> Vec<u8> {
    fake_framework_package_zip_with_identity(id, version, Some("publisher.test"))
}

pub(super) fn fake_framework_package_zip_with_identity(
    id: &str,
    version: &str,
    publisher: Option<&str>,
) -> Vec<u8> {
    use std::io::Write;
    let command = match id {
        "process" => "runtime/loom-framework-process.exe",
        "cloud_api" => "runtime/loom-framework-cloud-api.exe",
        "mcp" => "runtime/loom-framework-mcp.exe",
        "workflow" => "runtime/loom-framework-workflow.exe",
        _ => "runtime/loom-framework-third-party.exe",
    };
    let mut manifest = serde_json::json!({
        "id": id,
        "name": format!("{id} test framework"),
        "description": "test framework",
        "version": version,
        "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
        "platforms": [WINDOWS_X64_PLATFORM],
        "entry": {
            "kind": "process",
            "command": command,
            "args": ["--stdio"],
            "processModel": "per_execution"
        },
        "permissions": ["process.spawn"],
        "artExecution": {
            "requestSchema": "loom.art.execute.v1",
            "responseSchema": "loom.art.result.v1"
        }
    });
    if let Some(publisher) = publisher {
        manifest.as_object_mut().expect("manifest object").insert(
            "publisher".to_owned(),
            serde_json::json!({ "id": publisher, "name": publisher }),
        );
    }
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file(FRAMEWORK_MANIFEST_FILE, opts).unwrap();
        writer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        writer.start_file(command, opts).unwrap();
        writer.write_all(b"MZ-fake-framework").unwrap();
        if id == "process" {
            writer.start_file("python-embed/python.exe", opts).unwrap();
            writer.write_all(b"MZ-fake-python").unwrap();
        }
        writer.finish().unwrap();
    }
    buf
}

pub(super) fn signed_framework_package_zip(
    id: &str,
    version: &str,
    publisher: &str,
    key: &loom_plugin_security::SigningKeyDocument,
) -> Vec<u8> {
    use std::io::Write;
    let package = temp_root().join("signed-package");
    let command = "runtime/loom-framework-third-party.exe";
    std::fs::create_dir_all(package.join("runtime")).expect("runtime directory");
    let manifest = serde_json::json!({
        "id": id,
        "name": format!("{id} signed test framework"),
        "description": "signed test framework",
        "version": version,
        "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
        "platforms": [WINDOWS_X64_PLATFORM],
        "entry": {
            "kind": "process",
            "command": command,
            "args": ["--stdio"],
            "processModel": "per_execution"
        },
        "permissions": ["process.spawn"],
        "artExecution": {
            "requestSchema": "loom.art.execute.v1",
            "responseSchema": "loom.art.result.v1"
        },
        "publisher": { "id": publisher, "keyId": key.key_id.clone() },
        "signature": {
            "algorithm": "ed25519",
            "keyId": key.key_id.clone(),
            "file": "signature.json"
        }
    });
    std::fs::write(
        package.join(FRAMEWORK_MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .expect("manifest");
    std::fs::write(package.join(command), b"MZ-signed-framework").expect("runtime");
    loom_plugin_security::sign_package(&package, "signature.json", key).expect("sign package");

    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for relative in [FRAMEWORK_MANIFEST_FILE, command, "signature.json"] {
            writer.start_file(relative, options).unwrap();
            writer
                .write_all(&std::fs::read(package.join(relative)).unwrap())
                .unwrap();
        }
        writer.finish().unwrap();
    }
    std::fs::remove_dir_all(package.parent().expect("package parent")).ok();
    bytes
}
