//! Art package installer (phase 1 of the art ecosystem).
//!
//! An art package is a zip whose `manifest.json` is a `ToolDefinition` (with an
//! optional `metadata.dependencies` block). Installing it:
//!   1. reads the manifest,
//!   2. checks the art's framework is installed + ready,
//!   3. extracts every zip entry into `<control-plane>/arts/<artId>/`,
//!   4. rewrites bundled binary/script paths to point inside that art dir,
//!   5. registers the `ToolDefinition` in the tool registry.
//! Dependent arts (workflow `uses` / `dependencies.arts`) are returned for the
//! caller to install recursively (wired with the store in phase 2).

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::framework::{read_dependencies, ArtBinary, FrameworkRegistry};
use crate::{ToolDefinition, ToolExecution, ToolRegistry};

const MANIFEST_NAME: &str = "manifest.json";

#[derive(Debug, thiserror::Error)]
pub enum ArtInstallError {
    #[error("invalid art package: {0}")]
    InvalidPackage(String),
    #[error("art package missing {MANIFEST_NAME}")]
    MissingManifest,
    #[error("invalid art id `{0}`")]
    InvalidArtId(String),
    #[error("art `{art_id}` requires framework `{framework}` which is not {reason}")]
    FrameworkNotReady {
        art_id: String,
        framework: String,
        reason: String,
    },
    #[error("art binary `{name}` is not bundled and has no download url")]
    BinaryMissing { name: String },
    #[error("download of art binary `{name}` failed: {reason}")]
    BinaryDownloadFailed { name: String, reason: String },
    #[error("art binary `{name}` sha256 mismatch: expected {expected}, got {actual}")]
    BinaryHashMismatch {
        name: String,
        expected: String,
        actual: String,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("tool registry error: {0}")]
    Registry(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtInstallReport {
    pub tool_id: String,
    pub framework: String,
    pub art_dir: PathBuf,
    pub installed_files: Vec<String>,
    /// Third-party binaries resolved (bundled or downloaded) into the art dir.
    pub binaries: Vec<String>,
    /// Dependent art ids to install next (from the manifest's dependencies).
    pub dependent_arts: Vec<String>,
}

/// Reject ids that aren't safe as a single directory name (mirrors
/// `require_no_path_separator`).
fn is_safe_art_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains(':')
        && id != "."
        && id != ".."
        && !id.contains("..")
}

/// Read the `manifest.json` (a `ToolDefinition`) from an art package zip without
/// extracting anything. Testable in isolation.
pub fn read_manifest_from_zip(zip_bytes: &[u8]) -> Result<ToolDefinition, ArtInstallError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut file = archive
        .by_name(MANIFEST_NAME)
        .map_err(|_| ArtInstallError::MissingManifest)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let tool: ToolDefinition = serde_json::from_str(&text)?;
    Ok(tool)
}

/// If a bundled path is a bare relative path (not absolute), resolve it inside
/// the art dir so the executor finds the extracted binary/script.
fn resolve_bundled_path(raw: &str, art_dir: &Path) -> String {
    let path = Path::new(raw);
    if path.is_absolute() {
        return raw.to_owned();
    }
    // Skip templated tokens (e.g. "{{output}}") — only rewrite real file refs.
    if raw.contains("{{") || raw.contains('}') {
        return raw.to_owned();
    }
    let bundled = art_dir.join(raw);
    if bundled.exists() {
        return bundled.to_string_lossy().to_string();
    }
    raw.to_owned()
}

/// Rewrite an execution's bundled command/script path into the art dir.
fn rewrite_execution_paths(execution: &mut ToolExecution, art_dir: &Path) {
    match execution {
        ToolExecution::CliWrapper { command, .. } => {
            *command = resolve_bundled_path(command, art_dir);
        }
        ToolExecution::Script { path } => {
            *path = resolve_bundled_path(path, art_dir);
        }
        ToolExecution::PythonArt { art_path, .. } => {
            if let Some(path) = art_path {
                *path = resolve_bundled_path(path, art_dir);
            }
        }
        _ => {}
    }
}

fn rewrite_artloom_compat_execution_paths(
    metadata: &mut Option<serde_json::Value>,
    art_dir: &Path,
) {
    let Some(root) = metadata.as_mut().and_then(serde_json::Value::as_object_mut) else {
        return;
    };
    let Some(execution) = root
        .get_mut("artloomCompat")
        .and_then(serde_json::Value::as_object_mut)
        .and_then(|compat| compat.get_mut("execution"))
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };

    for key in ["command", "path", "artPath", "pythonPath"] {
        if let Some(value) = execution.get(key).and_then(serde_json::Value::as_str) {
            let rewritten = resolve_bundled_path(value, art_dir);
            execution.insert(key.to_owned(), serde_json::Value::String(rewritten));
        }
    }
}

/// Install an art package from zip bytes into `<control_plane_root>/arts/<id>/`.
pub fn install_art_from_zip(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<ArtInstallReport, ArtInstallError> {
    let mut tool = read_manifest_from_zip(zip_bytes)?;
    if !is_safe_art_id(&tool.id) {
        return Err(ArtInstallError::InvalidArtId(tool.id.clone()));
    }

    // Framework must be installed + ready before we lay down files.
    let deps = read_dependencies(&tool);
    let framework = deps.framework.clone().unwrap_or_else(|| {
        crate::framework::framework_id_for_execution(&tool.execution).to_owned()
    });
    if !framework_registry.is_installed(&framework) {
        return Err(ArtInstallError::FrameworkNotReady {
            art_id: tool.id.clone(),
            framework,
            reason: "installed".to_owned(),
        });
    }
    let (ready, _) = framework_registry.readiness(&framework);
    if !ready {
        return Err(ArtInstallError::FrameworkNotReady {
            art_id: tool.id.clone(),
            framework,
            reason: "ready".to_owned(),
        });
    }

    // Extract every entry into the art dir (fresh — clear any prior install).
    let art_dir = control_plane_root.join("arts").join(&tool.id);
    if art_dir.exists() {
        std::fs::remove_dir_all(&art_dir)?;
    }
    std::fs::create_dir_all(&art_dir)?;

    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut installed_files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(ArtInstallError::InvalidPackage(format!(
                "unsafe path in zip: {}",
                entry.name()
            )));
        };
        let out_path = art_dir.join(&enclosed);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        std::fs::write(&out_path, &buf)?;
        installed_files.push(enclosed.to_string_lossy().to_string());
    }

    // Resolve declared third-party binaries (portable exes): those already
    // bundled in the zip are verified in place; those with only a download url
    // are fetched into the art dir. Both are sha256-checked when a hash is given.
    let binaries = resolve_binaries(&deps.binaries, &art_dir, &installed_files)?;

    // Rewrite bundled binary/script paths into the art dir, then register.
    rewrite_execution_paths(&mut tool.execution, &art_dir);
    rewrite_artloom_compat_execution_paths(&mut tool.metadata, &art_dir);
    let tool_id = tool.id.clone();
    tool_registry
        .save_tool(tool)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?;

    Ok(ArtInstallReport {
        tool_id,
        framework,
        art_dir,
        installed_files,
        binaries,
        dependent_arts: deps.arts,
    })
}

/// Hex-encode a byte slice (lowercase).
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Compute the sha256 of `bytes` as a lowercase hex string.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

/// Verify (optional) sha256 of already-loaded binary bytes.
fn verify_binary_hash(
    name: &str,
    bytes: &[u8],
    expected: &Option<String>,
) -> Result<(), ArtInstallError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(expected.trim()) {
        return Err(ArtInstallError::BinaryHashMismatch {
            name: name.to_owned(),
            expected: expected.trim().to_owned(),
            actual,
        });
    }
    Ok(())
}

/// Download a portable third-party binary into `dest`, verifying sha256 if given.
fn download_binary(binary: &ArtBinary, dest: &Path) -> Result<(), ArtInstallError> {
    let url = binary
        .url
        .as_deref()
        .ok_or_else(|| ArtInstallError::BinaryMissing {
            name: binary.name.clone(),
        })?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("Loom/0.1 Art Binary Fetch")
        // Bypass any (possibly dead) system proxy; the store is typically local.
        .no_proxy()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|error| ArtInstallError::BinaryDownloadFailed {
            name: binary.name.clone(),
            reason: error.to_string(),
        })?;
    let bytes = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .map_err(|error| ArtInstallError::BinaryDownloadFailed {
            name: binary.name.clone(),
            reason: error.to_string(),
        })?;
    verify_binary_hash(&binary.name, &bytes, &binary.sha256)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, &bytes)?;
    Ok(())
}

/// Resolve each declared binary into the art dir. A binary bundled in the zip
/// (its `name` matches an extracted file) is verified in place; otherwise it is
/// downloaded from its `url`. Returns the relative names resolved.
fn resolve_binaries(
    binaries: &[ArtBinary],
    art_dir: &Path,
    installed_files: &[String],
) -> Result<Vec<String>, ArtInstallError> {
    let mut resolved = Vec::new();
    for binary in binaries {
        let rel = binary.name.replace('\\', "/");
        let bundled = installed_files
            .iter()
            .any(|file| file.replace('\\', "/") == rel);
        let dest = art_dir.join(&rel);
        if bundled {
            // Verify the already-extracted file if a hash was declared.
            if binary.sha256.is_some() {
                let bytes = std::fs::read(&dest)?;
                verify_binary_hash(&binary.name, &bytes, &binary.sha256)?;
            }
        } else if binary.url.is_some() {
            download_binary(binary, &dest)?;
        } else {
            return Err(ArtInstallError::BinaryMissing {
                name: binary.name.clone(),
            });
        }
        resolved.push(rel);
    }
    Ok(resolved)
}

/// Install an art package and, recursively, its dependent arts. `fetch_dependent`
/// returns the zip bytes for a dependent art id (wired to the store over HTTP).
/// Already-installed / already-visited ids are skipped, guarding against cycles.
/// Returns one report per art actually installed (root first).
pub fn install_art_recursive<F>(
    root_zip: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
    fetch_dependent: &F,
) -> Result<Vec<ArtInstallReport>, ArtInstallError>
where
    F: Fn(&str) -> Result<Vec<u8>, ArtInstallError>,
{
    let mut reports = Vec::new();
    let mut visited = std::collections::BTreeSet::new();

    let root = install_art_from_zip(
        root_zip,
        control_plane_root,
        framework_registry,
        tool_registry,
    )?;
    visited.insert(root.tool_id.clone());
    let mut queue: std::collections::VecDeque<String> =
        root.dependent_arts.iter().cloned().collect();
    reports.push(root);

    while let Some(dep_id) = queue.pop_front() {
        if visited.contains(&dep_id) {
            continue;
        }
        visited.insert(dep_id.clone());
        // Skip if already registered.
        if tool_registry
            .get_tool(&dep_id)
            .map(|tool| tool.is_some())
            .unwrap_or(false)
        {
            continue;
        }
        let zip = fetch_dependent(&dep_id)?;
        let report =
            install_art_from_zip(&zip, control_plane_root, framework_registry, tool_registry)?;
        for next in &report.dependent_arts {
            if !visited.contains(next) {
                queue.push_back(next.clone());
            }
        }
        reports.push(report);
    }

    Ok(reports)
}

/// Package an art into a publishable zip: a `manifest.json` (the ToolDefinition)
/// plus every file in the art's resource dir (`<control-plane>/arts/<id>/`).
/// Inverse of `install_art_from_zip`. Returns the zip bytes.
pub fn package_art_to_zip(
    tool: &ToolDefinition,
    art_dir: &Path,
) -> Result<Vec<u8>, ArtInstallError> {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        // manifest.json — the ToolDefinition.
        writer
            .start_file(MANIFEST_NAME, opts)
            .map_err(ArtInstallError::Zip)?;
        let manifest = serde_json::to_vec_pretty(tool)?;
        writer.write_all(&manifest)?;
        // Bundle the art resource dir, if present.
        if art_dir.is_dir() {
            add_dir_to_zip(&mut writer, art_dir, art_dir, opts)?;
        }
        writer.finish().map_err(ArtInstallError::Zip)?;
    }
    Ok(buf)
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    base: &Path,
    dir: &Path,
    opts: zip::write::FileOptions<'_, ()>,
) -> Result<(), ArtInstallError> {
    use std::io::Write;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            add_dir_to_zip(writer, base, &path, opts)?;
        } else {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            let name = rel.to_string_lossy().replace('\\', "/");
            // manifest.json is written explicitly from the ToolDefinition; skip
            // any copy left in the art dir to avoid a duplicate zip entry.
            if name == MANIFEST_NAME {
                continue;
            }
            writer
                .start_file(name, opts)
                .map_err(ArtInstallError::Zip)?;
            let bytes = std::fs::read(&path)?;
            writer.write_all(&bytes)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn temp_root() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "loom-art-install-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    fn build_zip(manifest: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            writer.start_file(MANIFEST_NAME, opts).unwrap();
            writer.write_all(manifest.as_bytes()).unwrap();
            for (name, bytes) in extra {
                writer.start_file(*name, opts).unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    fn build_runtime_zip(extra: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            for (name, bytes) in extra {
                writer.start_file(*name, opts).unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn reads_manifest_from_zip() {
        let manifest = r#"{"id":"art-x","name":"X","description":"d","enabled":true,
            "execution":{"type":"cli_wrapper","command":"bin/pingo.exe","args":["{{input}}"]}}"#;
        let zip = build_zip(manifest, &[]);
        let tool = read_manifest_from_zip(&zip).expect("read manifest");
        assert_eq!(tool.id, "art-x");
    }

    #[test]
    fn installs_package_extracts_files_and_rewrites_paths() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        // cli_wrapper is built-in => installed + ready by default.
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"pingo-art","name":"Pingo","description":"compress","enabled":true,
            "execution":{"type":"cli_wrapper","command":"bin/pingo.exe","args":["-s{{level}}","{{output}}"]}}"#;
        let zip = build_zip(manifest, &[("bin/pingo.exe", b"MZ-fake-exe")]);

        let report = install_art_from_zip(&zip, &root, &framework, &registry).expect("install art");
        assert_eq!(report.tool_id, "pingo-art");
        assert_eq!(report.framework, "cli_wrapper");
        // Binary extracted into the art dir.
        assert!(report.art_dir.join("bin/pingo.exe").exists());
        assert!(report
            .installed_files
            .iter()
            .any(|f| f.contains("pingo.exe")));

        // Registered tool's command now points into the art dir.
        let saved = registry.get_tool("pingo-art").unwrap().unwrap();
        if let ToolExecution::CliWrapper { command, .. } = &saved.execution {
            assert!(
                command.contains("pingo-art"),
                "command rewritten: {command}"
            );
            assert!(command.ends_with("pingo.exe"));
        } else {
            panic!("expected cli_wrapper");
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_preserves_non_bundled_cli_command_names() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"shell-copy-art","name":"Shell Copy","description":"copy","enabled":true,
            "execution":{"type":"cli_wrapper","command":"powershell.exe","args":["-NoProfile","-Command","Copy-Item -LiteralPath '{{input}}' -Destination '{{output}}' -Force"]}}"#;
        let zip = build_zip(manifest, &[]);

        install_art_from_zip(&zip, &root, &framework, &registry).expect("install shell art");
        let saved = registry.get_tool("shell-copy-art").unwrap().unwrap();
        if let ToolExecution::CliWrapper { command, .. } = &saved.execution {
            assert_eq!(command, "powershell.exe");
        } else {
            panic!("expected cli_wrapper");
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn verifies_bundled_binary_hash_and_reports_it() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));

        let exe = b"MZ-fake-exe";
        let digest = sha256_hex(exe);
        let manifest = format!(
            r#"{{"id":"pingo-hashed","name":"Pingo","description":"c","enabled":true,
            "execution":{{"type":"cli_wrapper","command":"bin/pingo.exe","args":["{{{{output}}}}"]}},
            "metadata":{{"dependencies":{{"binaries":[{{"name":"bin/pingo.exe","sha256":"{digest}"}}]}}}}}}"#
        );
        let zip = build_zip(&manifest, &[("bin/pingo.exe", exe)]);
        let report =
            install_art_from_zip(&zip, &root, &framework, &registry).expect("install hashed art");
        assert_eq!(report.binaries, vec!["bin/pingo.exe"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_bundled_binary_hash_mismatch() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"pingo-badhash","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"cli_wrapper","command":"bin/pingo.exe","args":["{{output}}"]},
            "metadata":{"dependencies":{"binaries":[{"name":"bin/pingo.exe","sha256":"deadbeef"}]}}}"#;
        let zip = build_zip(manifest, &[("bin/pingo.exe", b"MZ-fake-exe")]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(matches!(err, ArtInstallError::BinaryHashMismatch { .. }));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_binary_neither_bundled_nor_downloadable() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"pingo-nobs","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"cli_wrapper","command":"bin/pingo.exe","args":["{{output}}"]},
            "metadata":{"dependencies":{"binaries":[{"name":"bin/missing.exe"}]}}}"#;
        let zip = build_zip(manifest, &[]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(matches!(err, ArtInstallError::BinaryMissing { .. }));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_install_when_framework_not_installed() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));
        // python_art is NOT installed by default.
        let manifest = r#"{"id":"py-art","name":"Py","description":"d","enabled":true,
            "execution":{"type":"python_art","artId":"py-art"}}"#;
        let zip = build_zip(manifest, &[]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(matches!(err, ArtInstallError::FrameworkNotReady { .. }));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_unsafe_art_id() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"../evil","name":"E","description":"d","enabled":true,
            "execution":{"type":"cli_wrapper","command":"x","args":[]}}"#;
        let zip = build_zip(manifest, &[]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(matches!(err, ArtInstallError::InvalidArtId(_)));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn returns_dependent_arts_from_manifest() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"wf-art","name":"WF","description":"d","enabled":true,
            "execution":{"type":"workflow","workflowId":"wf"},
            "metadata":{"dependencies":{"framework":"workflow","arts":["dep-1","dep-2"]}}}"#;
        let zip = build_zip(manifest, &[]);
        let report =
            install_art_from_zip(&zip, &root, &framework, &registry).expect("install wf art");
        assert_eq!(report.dependent_arts, vec!["dep-1", "dep-2"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recursive_install_pulls_dependent_arts() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));

        // Root workflow art depends on dep-1; dep-1 depends on dep-2.
        let root_manifest = r#"{"id":"root-wf","name":"Root","description":"d","enabled":true,
            "execution":{"type":"workflow","workflowId":"wf"},
            "metadata":{"dependencies":{"framework":"workflow","arts":["dep-1"]}}}"#;
        let root_zip = build_zip(root_manifest, &[]);

        let dep1 = r#"{"id":"dep-1","name":"D1","description":"d","enabled":true,
            "execution":{"type":"cloud_api","endpoint":"https://x","method":"POST"},
            "metadata":{"dependencies":{"framework":"cloud_api","arts":["dep-2"]}}}"#;
        let dep2 = r#"{"id":"dep-2","name":"D2","description":"d","enabled":true,
            "execution":{"type":"cloud_api","endpoint":"https://y","method":"POST"}}"#;
        let dep1_zip = build_zip(dep1, &[]);
        let dep2_zip = build_zip(dep2, &[]);

        let fetch = |id: &str| -> Result<Vec<u8>, ArtInstallError> {
            match id {
                "dep-1" => Ok(dep1_zip.clone()),
                "dep-2" => Ok(dep2_zip.clone()),
                other => Err(ArtInstallError::InvalidPackage(format!("no art {other}"))),
            }
        };

        let reports = install_art_recursive(&root_zip, &root, &framework, &registry, &fetch)
            .expect("recursive");
        let ids: Vec<&str> = reports.iter().map(|r| r.tool_id.as_str()).collect();
        assert_eq!(ids, vec!["root-wf", "dep-1", "dep-2"]);
        assert!(registry.get_tool("dep-2").unwrap().is_some());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn package_roundtrips_installed_art() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"pkg-art","name":"Pkg","description":"d","enabled":true,
            "execution":{"type":"cli_wrapper","command":"bin/tool.exe","args":["{{input}}"]}}"#;
        let zip = build_zip(manifest, &[("bin/tool.exe", b"binary")]);
        let report = install_art_from_zip(&zip, &root, &framework, &registry).expect("install");

        let saved = registry.get_tool("pkg-art").unwrap().unwrap();
        let packaged = package_art_to_zip(&saved, &report.art_dir).expect("package");
        // The packaged zip is re-readable and carries the bundled binary.
        let manifest_back = read_manifest_from_zip(&packaged).expect("read back");
        assert_eq!(manifest_back.id, "pkg-art");
        let mut archive = zip::ZipArchive::new(Cursor::new(&packaged)).unwrap();
        assert!(archive.by_name("bin/tool.exe").is_ok());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn installs_python_art_package_rewrites_art_paths_for_runtime_and_hook_compat() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        framework
            .install_with_runtime_fetcher("python_art", &|_id| {
                Ok(build_runtime_zip(&[
                    ("python-embed/python.exe", b"fake-python"),
                    ("python/Launcher.py", b"print('launcher')"),
                ]))
            })
            .expect("install python_art runtime");
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{
            "id":"color-transfer-art",
            "name":"Color Transfer",
            "description":"shader python art",
            "enabled":true,
            "execution":{"type":"python_art","artId":"art_color_transfer","artPath":"python/Arts/Art_ColorTransfer"},
            "metadata":{"artloomCompat":{"executionType":"shader","execution":{"artPath":"python/Arts/Art_ColorTransfer"}}}
        }"#;
        let zip = build_zip(
            manifest,
            &[
                (
                    "python/Arts/Art_ColorTransfer/art.json",
                    br#"{"art_id":"art_color_transfer","label":"Color Transfer"}"#,
                ),
                (
                    "python/Arts/Art_ColorTransfer/main.py",
                    b"def main(args):\n    return {'content':[{'type':'text','text':'ok'}]}\n",
                ),
            ],
        );

        let report =
            install_art_from_zip(&zip, &root, &framework, &registry).expect("install python art");
        let saved = registry.get_tool("color-transfer-art").unwrap().unwrap();
        let expected = report
            .art_dir
            .join("python/Arts/Art_ColorTransfer")
            .to_string_lossy()
            .to_string();

        match &saved.execution {
            ToolExecution::PythonArt { art_path, .. } => {
                assert_eq!(art_path.as_deref(), Some(expected.as_str()));
            }
            other => panic!("expected python_art, got {other:?}"),
        }

        let compat_art_path = saved
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("artloomCompat"))
            .and_then(|compat| compat.get("execution"))
            .and_then(|execution| execution.get("artPath"))
            .and_then(serde_json::Value::as_str);
        assert_eq!(compat_art_path, Some(expected.as_str()));

        std::fs::remove_dir_all(&root).ok();
    }
}
