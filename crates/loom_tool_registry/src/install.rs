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

use std::ffi::OsStr;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use loom_plugin_security::{
    canonical_package_digest, sign_package, verify_package_signature, SigningKeyDocument,
    TrustStore,
};
use loom_protocol::{
    ArtRuntimeManifest, PackageSignature, PackageTrustStatus, PluginLockfile, PublisherIdentity,
    ResolvedDependency,
};

use crate::framework::{read_dependencies, ArtBinary, FrameworkRegistry};
use crate::{is_obsolete_execution_type, ToolDefinition, ToolRegistry};

const MANIFEST_NAME: &str = "manifest.json";
const ART_LIFECYCLE_FILE: &str = "lifecycle.json";
const ART_UNINSTALL_TOMBSTONE_PREFIX: &str = ".loom-delete-art-";

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
    #[error("remote art binary `{name}` must declare a sha256 digest")]
    RemoteBinaryHashRequired { name: String },
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
    pub trust_status: PackageTrustStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtInstalledVersion {
    pub version: String,
    pub digest: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtPackageSecurityMetadata {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    publisher: Option<PublisherIdentity>,
    #[serde(default)]
    signature: Option<PackageSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtActivationState {
    active: ArtVersionPointer,
    previous: Option<ArtVersionPointer>,
    #[serde(default)]
    local_authoring: bool,
    #[serde(default)]
    bundled_catalog: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtInstallSource {
    ExternalPackage,
    LocalAuthoring,
    BundledCatalog,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtVersionPointer {
    path: String,
    version: String,
    digest: String,
    lockfile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtLifecycleJournal {
    old_activation: Option<ArtActivationState>,
    next_activation: ArtActivationState,
    target: String,
}

fn read_art_package_security(tool: &ToolDefinition) -> ArtPackageSecurityMetadata {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("packageSecurity"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
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

fn is_safe_art_reference(reference: &str) -> bool {
    if let Some((publisher, id)) = reference.split_once('/') {
        !publisher.contains('/')
            && loom_protocol::is_safe_publisher_id(publisher)
            && is_safe_art_id(id)
    } else {
        is_safe_art_id(reference)
    }
}

fn art_root_for_reference(control_plane_root: &Path, reference: &str) -> Option<PathBuf> {
    if !is_safe_art_reference(reference) {
        return None;
    }
    let arts = control_plane_root.join("arts");
    reference
        .split_once('/')
        .map(|(publisher, id)| arts.join(publisher).join(id))
        .or_else(|| Some(arts.join(reference)))
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

struct ArtPackagePaths<'a> {
    qualified_id: &'a str,
    art_dir: &'a Path,
    state_dir: &'a Path,
    cache_dir: &'a Path,
    output_dir: &'a Path,
    lockfile: &'a Path,
    version: &'a str,
    digest: &'a str,
    trust_status: &'a PackageTrustStatus,
}

fn record_art_package_directory(
    metadata: &mut Option<serde_json::Value>,
    paths: ArtPackagePaths<'_>,
) {
    let root = metadata.get_or_insert_with(|| serde_json::json!({}));
    if !root.is_object() {
        *root = serde_json::json!({});
    }
    if let Some(object) = root.as_object_mut() {
        object.insert(
            "artPackage".to_owned(),
            serde_json::json!({
                "qualifiedId": paths.qualified_id,
                "dir": paths.art_dir.to_string_lossy().to_string(),
                "stateDir": paths.state_dir.to_string_lossy().to_string(),
                "cacheDir": paths.cache_dir.to_string_lossy().to_string(),
                "outputDir": paths.output_dir.to_string_lossy().to_string(),
                "lockfile": paths.lockfile.to_string_lossy().to_string(),
                "version": paths.version,
                "digest": paths.digest,
                "trustStatus": paths.trust_status
            }),
        );
        let compat = object
            .entry("artloomCompat".to_owned())
            .or_insert_with(|| serde_json::json!({}));
        if !compat.is_object() {
            *compat = serde_json::json!({});
        }
        if let Some(compat) = compat.as_object_mut() {
            // Package-installed Arts are Loom-owned and Hook-visible. Never let
            // an untrusted package claim the sync-owned `artloom-compat` source.
            compat.insert(
                "source".to_owned(),
                serde_json::Value::String("loom-local".to_owned()),
            );
        }
    }
}

fn qualified_art_id(tool: &ToolDefinition) -> String {
    tool.publisher_identity()
        .map(|publisher| format!("{}/{}", publisher.id, tool.id))
        .unwrap_or_else(|| tool.id.clone())
}

fn art_root_for_tool(control_plane_root: &Path, tool: &ToolDefinition) -> PathBuf {
    let arts_root = control_plane_root.join("arts");
    match tool.publisher_identity() {
        Some(publisher) => arts_root.join(publisher.id).join(&tool.id),
        None => arts_root.join(&tool.id),
    }
}

fn migrate_art_namespace(
    control_plane_root: &Path,
    tool: &ToolDefinition,
    target: &Path,
) -> Result<(), ArtInstallError> {
    if tool.publisher_identity().is_none() || target.exists() {
        return Ok(());
    }
    let legacy = control_plane_root.join("arts").join(&tool.id);
    if !legacy.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(legacy, target)?;
    Ok(())
}

fn migrate_legacy_art_layout(
    control_plane_root: &Path,
    art_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<(), ArtInstallError> {
    if art_root.join("active.json").is_file() || !art_root.join(MANIFEST_NAME).is_file() {
        return Ok(());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let legacy = control_plane_root.join(format!(".loom-art-legacy-{nonce}"));
    std::fs::rename(art_root, &legacy)?;
    let result = (|| {
        let manifest_bytes = std::fs::read(legacy.join(MANIFEST_NAME))?;
        let manifest_value: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
        if manifest_value
            .get("execution")
            .and_then(|execution| execution.get("type"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(is_obsolete_execution_type)
        {
            remove_tree(&legacy)?;
            return Ok(());
        }
        let tool: ToolDefinition = serde_json::from_value(manifest_value)?;
        let security = read_art_package_security(&tool);
        let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let trust_status = verify_package_signature(
            &legacy,
            security.publisher.as_ref(),
            security.signature.as_ref(),
            &trust_store,
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        trust_store
            .effective_policy()
            .enforce(trust_status)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let digest = canonical_package_digest(
            &legacy,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let version = security.version.unwrap_or_else(|| "0.0.0".to_owned());
        let relative = Path::new("versions").join(format!(
            "{}-{}",
            sanitize_version_for_path(&version),
            &digest[..12]
        ));
        let target = art_root.join(&relative);
        std::fs::create_dir_all(target.parent().expect("legacy Art version parent"))?;
        std::fs::rename(&legacy, &target)?;
        let locks_dir = art_root.join("locks");
        for directory in [
            art_root.join("state"),
            art_root.join("cache"),
            art_root.join("outputs"),
            locks_dir.clone(),
        ] {
            std::fs::create_dir_all(directory)?;
        }
        let lockfile = locks_dir.join(format!("{digest}.json"));
        let dependencies = read_dependencies(&tool);
        let framework = dependencies.framework.as_deref().ok_or_else(|| {
            ArtInstallError::InvalidPackage("legacy Art has no framework dependency".to_owned())
        })?;
        let locked_arts = resolve_art_dependency_locks(
            control_plane_root,
            &dependencies.arts,
            framework_registry,
            tool_registry,
        )?;
        write_art_lockfile(
            &lockfile,
            &qualified_art_id(&tool),
            &version,
            framework,
            framework_registry,
            &dependencies.binaries,
            &target,
            &locked_arts,
        )?;
        set_tree_readonly(&target, true)?;
        write_art_activation(
            &art_root.join("active.json"),
            &ArtActivationState {
                active: ArtVersionPointer {
                    path: relative.to_string_lossy().replace('\\', "/"),
                    version,
                    digest,
                    lockfile: lockfile.to_string_lossy().to_string(),
                },
                previous: None,
                local_authoring: false,
                bundled_catalog: false,
            },
        )
    })();
    if result.is_err() {
        let migrated = std::fs::read_dir(art_root.join("versions"))
            .ok()
            .and_then(|mut entries| entries.next())
            .and_then(Result::ok)
            .map(|entry| entry.path());
        if let Some(migrated) = migrated {
            let _ = std::fs::rename(migrated, &legacy);
        }
        let _ = std::fs::remove_dir_all(art_root);
        if legacy.exists() {
            let _ = std::fs::rename(&legacy, art_root);
        }
    }
    result
}

fn set_tree_readonly(path: &Path, readonly: bool) -> Result<(), ArtInstallError> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            set_tree_readonly(&entry?.path(), readonly)?;
        }
    }
    let metadata = std::fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode();
        permissions.set_mode(if readonly {
            mode & !0o222
        } else {
            mode | 0o200
        });
    }
    #[cfg(not(unix))]
    permissions.set_readonly(readonly);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

fn uninstall_tombstone_path(live: &Path, prefix: &str) -> Result<PathBuf, ArtInstallError> {
    let parent = live.parent().ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art package root has no parent".to_owned())
    })?;
    let name = live.file_name().and_then(OsStr::to_str).ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art package root has no UTF-8 name".to_owned())
    })?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(parent.join(format!("{prefix}{name}--{nonce}")))
}

fn uninstall_tombstone_original_name(path: &Path, prefix: &str) -> Option<String> {
    let name = path.file_name()?.to_str()?.strip_prefix(prefix)?;
    let (original, nonce) = name.rsplit_once("--")?;
    (!original.is_empty()
        && nonce.bytes().all(|byte| byte.is_ascii_digit())
        && is_safe_art_id(original))
    .then(|| original.to_owned())
}

fn remove_tree(path: &Path) -> Result<(), ArtInstallError> {
    if path.exists() {
        set_tree_readonly(path, false)?;
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Install an art package into an immutable publisher-scoped version directory.
pub fn install_art_from_zip(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<ArtInstallReport, ArtInstallError> {
    install_art_from_zip_with_source(
        zip_bytes,
        control_plane_root,
        framework_registry,
        tool_registry,
        ArtInstallSource::ExternalPackage,
    )
}

pub fn install_authored_art_from_zip(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<ArtInstallReport, ArtInstallError> {
    install_art_from_zip_with_source(
        zip_bytes,
        control_plane_root,
        framework_registry,
        tool_registry,
        ArtInstallSource::LocalAuthoring,
    )
}

pub fn install_bundled_art_from_zip(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<ArtInstallReport, ArtInstallError> {
    install_art_from_zip_with_source(
        zip_bytes,
        control_plane_root,
        framework_registry,
        tool_registry,
        ArtInstallSource::BundledCatalog,
    )
}

fn install_art_from_zip_with_source(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
    source: ArtInstallSource,
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
    if let Some(requirement) = deps.framework_version.as_deref() {
        let requirement = semver::VersionReq::parse(requirement).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "invalid frameworkVersion requirement `{requirement}`: {error}"
            ))
        })?;
        let installed_version = framework_registry
            .statuses()
            .into_iter()
            .find(|status| status.qualified_id == framework || status.id == framework)
            .and_then(|status| status.version)
            .ok_or_else(|| ArtInstallError::FrameworkNotReady {
                art_id: tool.id.clone(),
                framework: framework.clone(),
                reason: "versioned".to_owned(),
            })?;
        let installed_version = semver::Version::parse(&installed_version).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "installed framework version `{installed_version}` is invalid: {error}"
            ))
        })?;
        if !requirement.matches(&installed_version) {
            return Err(ArtInstallError::FrameworkNotReady {
                art_id: tool.id.clone(),
                framework: framework.clone(),
                reason: format!(
                    "compatible: requires {requirement}, installed {installed_version}"
                ),
            });
        }
    }

    let arts_root = control_plane_root.join("arts");
    std::fs::create_dir_all(&arts_root)?;
    let art_root = art_root_for_tool(control_plane_root, &tool);
    migrate_art_namespace(control_plane_root, &tool, &art_root)?;
    migrate_legacy_art_layout(
        control_plane_root,
        &art_root,
        framework_registry,
        tool_registry,
    )?;
    let qualified_id = qualified_art_id(&tool);
    let locked_arts = resolve_art_dependency_locks(
        control_plane_root,
        &deps.arts,
        framework_registry,
        tool_registry,
    )?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let staging = control_plane_root.join(format!(".loom-art-{}-{nonce}", tool.id));
    let result = (|| {
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir_all(&staging)?;
        let installed_files = crate::secure_zip::extract_zip_securely(zip_bytes, &staging)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;

        let security = read_art_package_security(&tool);
        let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let trust_status = verify_package_signature(
            &staging,
            security.publisher.as_ref(),
            security.signature.as_ref(),
            &trust_store,
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if source == ArtInstallSource::ExternalPackage {
            trust_store
                .effective_policy()
                .enforce(trust_status.clone())
                .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        }

        // Resolve declared third-party binaries before activation. Bundled
        // files are verified in staging; downloads cannot alter the active Art.
        let binaries = resolve_binaries(&deps.binaries, &staging, &installed_files)?;
        let digest = canonical_package_digest(
            &staging,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let version = security.version.as_deref().unwrap_or("0.0.0");
        let active_relative = Path::new("versions").join(format!(
            "{}-{}",
            sanitize_version_for_path(version),
            &digest[..12]
        ));
        let art_dir = art_root.join(&active_relative);
        std::fs::create_dir_all(art_dir.parent().expect("Art version parent"))?;
        let target_created = if art_dir.exists() {
            std::fs::remove_dir_all(&staging)?;
            false
        } else {
            std::fs::rename(&staging, &art_dir)?;
            true
        };

        let state_dir = art_root.join("state");
        let cache_dir = art_root.join("cache");
        let output_dir = art_root.join("outputs");
        let locks_dir = art_root.join("locks");
        for directory in [&state_dir, &cache_dir, &output_dir, &locks_dir] {
            std::fs::create_dir_all(directory)?;
        }
        let lockfile = locks_dir.join(format!("{digest}.json"));
        write_art_lockfile(
            &lockfile,
            &qualified_id,
            version,
            &framework,
            framework_registry,
            &deps.binaries,
            &art_dir,
            &locked_arts,
        )?;
        set_tree_readonly(&art_dir, true)?;

        let active_path = art_root.join("active.json");
        let old_activation = read_art_activation(&active_path);
        let active_text = active_relative.to_string_lossy().replace('\\', "/");
        let active = ArtVersionPointer {
            path: active_text,
            version: version.to_owned(),
            digest: digest.clone(),
            lockfile: lockfile.to_string_lossy().to_string(),
        };
        let previous = old_activation
            .as_ref()
            .and_then(|activation| {
                (activation.active.path != active.path).then(|| activation.active.clone())
            })
            .or_else(|| {
                old_activation
                    .as_ref()
                    .and_then(|activation| activation.previous.clone())
            });
        let activation = ArtActivationState {
            active,
            previous,
            local_authoring: source == ArtInstallSource::LocalAuthoring,
            bundled_catalog: source == ArtInstallSource::BundledCatalog,
        };
        write_art_lifecycle(
            &art_root,
            &ArtLifecycleJournal {
                old_activation: old_activation.clone(),
                next_activation: activation.clone(),
                target: active_relative.to_string_lossy().replace('\\', "/"),
            },
        )?;
        if let Err(error) = write_art_activation(&active_path, &activation) {
            clear_art_lifecycle(&art_root);
            if target_created {
                let _ = remove_tree(&art_dir);
            }
            return Err(error);
        }

        rewrite_artloom_compat_execution_paths(&mut tool.metadata, &art_dir);
        record_art_package_directory(
            &mut tool.metadata,
            ArtPackagePaths {
                qualified_id: &qualified_id,
                art_dir: &art_dir,
                state_dir: &state_dir,
                cache_dir: &cache_dir,
                output_dir: &output_dir,
                lockfile: &lockfile,
                version,
                digest: &digest,
                trust_status: &trust_status,
            },
        );
        let tool_id = tool.id.clone();
        if let Err(error) = tool_registry.save_packaged_tool(tool) {
            if let Some(old_activation) = old_activation {
                let _ = write_art_activation(&active_path, &old_activation);
            } else {
                let _ = std::fs::remove_file(&active_path);
            }
            if target_created {
                let _ = remove_tree(&art_dir);
            }
            clear_art_lifecycle(&art_root);
            return Err(ArtInstallError::Registry(error.to_string()));
        }
        let _ = prune_art_versions(&art_root, &activation);
        clear_art_lifecycle(&art_root);

        Ok(ArtInstallReport {
            tool_id,
            framework,
            art_dir,
            installed_files,
            binaries,
            dependent_arts: deps.arts,
            trust_status,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn art_history_limit() -> usize {
    std::env::var("LOOM_PLUGIN_VERSION_HISTORY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .max(2)
}

fn prune_art_versions(
    art_root: &Path,
    activation: &ArtActivationState,
) -> Result<(), ArtInstallError> {
    let versions_root = art_root.join("versions");
    if !versions_root.is_dir() {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(&versions_root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        std::cmp::Reverse(
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });
    let active = art_root.join(&activation.active.path);
    let previous = activation
        .previous
        .as_ref()
        .map(|pointer| art_root.join(&pointer.path));
    let mut extra_retained = 0usize;
    for entry in entries {
        let path = entry.path();
        let pinned = path == active || previous.as_ref().is_some_and(|previous| *previous == path);
        if pinned || extra_retained < art_history_limit().saturating_sub(2) {
            if !pinned {
                extra_retained += 1;
            }
            continue;
        }
        remove_tree(&path)?;
    }
    Ok(())
}

fn read_art_activation(path: &Path) -> Option<ArtActivationState> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_art_activation(
    path: &Path,
    activation: &ArtActivationState,
) -> Result<(), ArtInstallError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(activation)?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)?;
    crate::replace_registry_file(&temporary, path)?;
    Ok(())
}

fn write_art_lifecycle(
    art_root: &Path,
    journal: &ArtLifecycleJournal,
) -> Result<(), ArtInstallError> {
    let path = art_root.join(ART_LIFECYCLE_FILE);
    let temporary = path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(journal)?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)?;
    crate::replace_registry_file(&temporary, &path)?;
    Ok(())
}

fn clear_art_lifecycle(art_root: &Path) {
    let _ = std::fs::remove_file(art_root.join(ART_LIFECYCLE_FILE));
}

fn is_safe_art_version_path(value: &str) -> bool {
    let path = Path::new(value);
    if path.is_absolute() {
        return false;
    }
    let mut components = path.components();
    matches!(
        (components.next(), components.next(), components.next()),
        (Some(Component::Normal(root)), Some(Component::Normal(_)), None)
            if root == OsStr::new("versions")
    )
}

fn art_activation_is_safe(activation: &ArtActivationState) -> bool {
    is_safe_art_version_path(&activation.active.path)
        && activation
            .previous
            .as_ref()
            .map(|pointer| is_safe_art_version_path(&pointer.path))
            .unwrap_or(true)
}

fn art_lifecycle_journal_is_safe(journal: &ArtLifecycleJournal) -> bool {
    is_safe_art_version_path(&journal.target)
        && art_activation_is_safe(&journal.next_activation)
        && journal
            .old_activation
            .as_ref()
            .map(art_activation_is_safe)
            .unwrap_or(true)
}

pub fn recover_art_lifecycle(control_plane_root: &Path) -> Result<(), ArtInstallError> {
    let arts_root = control_plane_root.join("arts");
    if !arts_root.is_dir() {
        return Ok(());
    }
    let mut roots = Vec::new();
    for first in std::fs::read_dir(&arts_root)? {
        let first = first?.path();
        if !first.is_dir() {
            continue;
        }
        if first.join(ART_LIFECYCLE_FILE).is_file() {
            roots.push(first.clone());
        }
        for second in std::fs::read_dir(&first).into_iter().flatten().flatten() {
            let second = second.path();
            if second.is_dir() && second.join(ART_LIFECYCLE_FILE).is_file() {
                roots.push(second);
            }
        }
    }
    for art_root in roots {
        let journal_path = art_root.join(ART_LIFECYCLE_FILE);
        let journal: ArtLifecycleJournal =
            match serde_json::from_slice(&std::fs::read(&journal_path)?) {
                Ok(journal) => journal,
                Err(_) => {
                    let _ = std::fs::rename(&journal_path, journal_path.with_extension("corrupt"));
                    continue;
                }
            };
        if !art_lifecycle_journal_is_safe(&journal) {
            let _ = std::fs::rename(&journal_path, journal_path.with_extension("corrupt"));
            continue;
        }
        let activation_path = art_root.join("active.json");
        let current = read_art_activation(&activation_path);
        if current.as_ref() != Some(&journal.next_activation) {
            if let Some(old) = &journal.old_activation {
                write_art_activation(&activation_path, old)?;
            } else {
                let _ = std::fs::remove_file(&activation_path);
            }
            let target = art_root.join(&journal.target);
            if target.exists() {
                let _ = remove_tree(&target);
            }
        }
        let _ = std::fs::remove_file(journal_path);
    }
    Ok(())
}

pub fn recover_art_uninstall_tombstones(control_plane_root: &Path) -> Result<(), ArtInstallError> {
    let arts_root = control_plane_root.join("arts");
    if !arts_root.is_dir() {
        return Ok(());
    }
    let mut parents = vec![arts_root.clone()];
    for entry in std::fs::read_dir(&arts_root)? {
        let path = entry?.path();
        if path.is_dir()
            && !path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with(ART_UNINSTALL_TOMBSTONE_PREFIX))
        {
            parents.push(path);
        }
    }
    let registry = ToolRegistry::new(control_plane_root.join("tools"));
    for parent in parents {
        for entry in std::fs::read_dir(&parent)? {
            let tombstone = entry?.path();
            if !tombstone.is_dir() {
                continue;
            }
            let Some(original_name) =
                uninstall_tombstone_original_name(&tombstone, ART_UNINSTALL_TOMBSTONE_PREFIX)
            else {
                continue;
            };
            let reference = if parent == arts_root {
                original_name.clone()
            } else {
                let Some(publisher) = parent.file_name().and_then(OsStr::to_str) else {
                    continue;
                };
                format!("{publisher}/{original_name}")
            };
            if !is_safe_art_reference(&reference) {
                continue;
            }
            let installed = registry
                .get_tool(&reference)
                .map_err(|error| ArtInstallError::Registry(error.to_string()))?
                .is_some();
            let live = parent.join(&original_name);
            if installed && !live.exists() {
                std::fs::rename(&tombstone, &live)?;
            } else {
                remove_tree(&tombstone)?;
            }
        }
    }
    Ok(())
}

pub fn resolve_active_art_package(control_plane_root: &Path, art_id: &str) -> Option<PathBuf> {
    let art_root = art_root_for_reference(control_plane_root, art_id)?;
    let activation = read_art_activation(&art_root.join("active.json"))?;
    if !is_safe_art_version_path(&activation.active.path) {
        return None;
    }
    let relative = Path::new(&activation.active.path);
    let active = art_root.join(relative);
    active.join(MANIFEST_NAME).is_file().then_some(active)
}

/// Resolves and verifies one immutable installed Art package without changing
/// the user's active version. Long-lived Surface instances use this path so an
/// unrelated store update cannot silently move their code or break execution.
pub fn resolve_installed_art_package(
    control_plane_root: &Path,
    art_id: &str,
    version: &str,
    digest: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<ToolDefinition, ArtInstallError> {
    if !is_safe_art_reference(art_id) {
        return Err(ArtInstallError::InvalidArtId(art_id.to_owned()));
    }
    if semver::Version::parse(version).is_err() {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art version `{version}` is not valid SemVer"
        )));
    }
    let digest = digest
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or(digest.trim());
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ArtInstallError::InvalidPackage(
            "Art package digest must be a SHA-256 hex value".to_owned(),
        ));
    }
    let digest = digest.to_ascii_lowercase();
    let current = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{art_id}` is not installed"))
        })?;
    let identity = current.qualified_id();
    let art_root = art_root_for_tool(control_plane_root, &current);
    let activation = read_art_activation(&art_root.join("active.json"))
        .ok_or_else(|| ArtInstallError::InvalidPackage("Art has no activation state".to_owned()))?;
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(
            "Art activation state contains an unsafe version path".to_owned(),
        ));
    }

    let mut matches = Vec::new();
    for entry in std::fs::read_dir(art_root.join("versions"))? {
        let path = entry?.path();
        if !path.is_dir() || !path.join(MANIFEST_NAME).is_file() {
            continue;
        }
        let tool: ToolDefinition =
            serde_json::from_slice(&std::fs::read(path.join(MANIFEST_NAME))?)?;
        let security = read_art_package_security(&tool);
        if tool.qualified_id() != identity
            || security.version.as_deref().unwrap_or("0.0.0") != version
        {
            continue;
        }
        let actual_digest = canonical_package_digest(
            &path,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if actual_digest != digest {
            continue;
        }
        let relative = path
            .strip_prefix(&art_root)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if !relative.ends_with(&actual_digest[..12]) {
            return Err(ArtInstallError::InvalidPackage(
                "Art version directory does not match its digest".to_owned(),
            ));
        }
        matches.push((path, relative, tool, security));
    }
    if matches.len() != 1 {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art `{art_id}` package `{version}` with digest `{digest}` is {}",
            if matches.is_empty() {
                "not installed"
            } else {
                "ambiguous"
            }
        )));
    }

    let (art_dir, relative, mut tool, security) = matches.remove(0);
    let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let trust_status = verify_package_signature(
        &art_dir,
        security.publisher.as_ref(),
        security.signature.as_ref(),
        &trust_store,
    )
    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    if !activation.local_authoring && !activation.bundled_catalog {
        trust_store
            .effective_policy()
            .enforce(trust_status.clone())
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    }
    let lockfile = art_root.join("locks").join(format!("{digest}.json"));
    let mut verifying = std::collections::BTreeSet::from([identity]);
    verify_art_lockfile(
        control_plane_root,
        &lockfile,
        &art_root,
        &art_dir,
        &tool,
        framework_registry,
        &mut verifying,
    )?;
    let state_dir = art_root.join("state");
    let cache_dir = art_root.join("cache");
    let output_dir = art_root.join("outputs");
    let qualified_id = qualified_art_id(&tool);
    rewrite_artloom_compat_execution_paths(&mut tool.metadata, &art_dir);
    record_art_package_directory(
        &mut tool.metadata,
        ArtPackagePaths {
            qualified_id: &qualified_id,
            art_dir: &art_dir,
            state_dir: &state_dir,
            cache_dir: &cache_dir,
            output_dir: &output_dir,
            lockfile: &lockfile,
            version,
            digest: &digest,
            trust_status: &trust_status,
        },
    );
    let lockfile_document: PluginLockfile = serde_json::from_slice(&std::fs::read(&lockfile)?)?;
    let mut locked_arts = serde_json::Map::new();
    for dependency in lockfile_document
        .resolved
        .iter()
        .filter(|dependency| dependency.kind == "art")
    {
        let child = resolve_installed_art_package(
            control_plane_root,
            &dependency.id,
            &dependency.version,
            &dependency.sha256,
            tool_registry,
            framework_registry,
        )?;
        locked_arts.insert(dependency.id.clone(), serde_json::to_value(child)?);
    }
    if !locked_arts.is_empty() {
        if let Some(package) = tool
            .metadata
            .as_mut()
            .and_then(|metadata| metadata.get_mut("artPackage"))
            .and_then(serde_json::Value::as_object_mut)
        {
            package.insert(
                "lockedArts".to_owned(),
                serde_json::Value::Object(locked_arts),
            );
        }
    }
    debug_assert!(relative.ends_with(&digest[..12]));
    Ok(tool)
}

pub fn verify_art_package_integrity(
    control_plane_root: &Path,
    tool: &ToolDefinition,
    framework_registry: &FrameworkRegistry,
) -> Result<(), ArtInstallError> {
    let mut verifying = std::collections::BTreeSet::new();
    verify_art_package_integrity_inner(control_plane_root, tool, framework_registry, &mut verifying)
        .map(|_| ())
}

fn verify_art_package_integrity_inner(
    control_plane_root: &Path,
    tool: &ToolDefinition,
    framework_registry: &FrameworkRegistry,
    verifying: &mut std::collections::BTreeSet<String>,
) -> Result<ArtVersionPointer, ArtInstallError> {
    let identity = tool.qualified_id();
    if !verifying.insert(identity.clone()) {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art dependency cycle detected at `{identity}`"
        )));
    }
    let result = (|| {
        let art_root = art_root_for_tool(control_plane_root, tool);
        let activation = read_art_activation(&art_root.join("active.json")).ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{}` has no activation state", tool.id))
        })?;
        if !art_activation_is_safe(&activation) {
            return Err(ArtInstallError::InvalidPackage(
                "Art activation state contains an unsafe version path".to_owned(),
            ));
        }
        let active_dir = art_root.join(&activation.active.path);
        let package_tool: ToolDefinition =
            serde_json::from_slice(&std::fs::read(active_dir.join(MANIFEST_NAME))?)?;
        if package_tool.qualified_id() != identity {
            return Err(ArtInstallError::InvalidPackage(
                "active Art manifest identity does not match the registry".to_owned(),
            ));
        }
        let security = read_art_package_security(&package_tool);
        let expected_version = security.version.as_deref().unwrap_or("0.0.0");
        if activation.active.version != expected_version {
            return Err(ArtInstallError::InvalidPackage(
                "active Art version does not match its manifest".to_owned(),
            ));
        }
        let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let trust_status = verify_package_signature(
            &active_dir,
            security.publisher.as_ref(),
            security.signature.as_ref(),
            &trust_store,
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if !activation.local_authoring && !activation.bundled_catalog {
            trust_store
                .effective_policy()
                .enforce(trust_status)
                .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        }
        let digest = canonical_package_digest(
            &active_dir,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if digest != activation.active.digest || !activation.active.path.ends_with(&digest[..12]) {
            return Err(ArtInstallError::InvalidPackage(
                "active Art digest does not match its immutable version pointer".to_owned(),
            ));
        }
        verify_art_lockfile(
            control_plane_root,
            Path::new(&activation.active.lockfile),
            &art_root,
            &active_dir,
            &package_tool,
            framework_registry,
            verifying,
        )?;
        Ok(activation.active)
    })();
    verifying.remove(&identity);
    result
}

pub fn list_installed_art_versions(
    control_plane_root: &Path,
    art_id: &str,
    tool_registry: &ToolRegistry,
) -> Result<Vec<ArtInstalledVersion>, ArtInstallError> {
    if !is_safe_art_reference(art_id) {
        return Err(ArtInstallError::InvalidArtId(art_id.to_owned()));
    }
    let current = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{art_id}` is not installed"))
        })?;
    let identity = current.qualified_id();
    let art_root = art_root_for_tool(control_plane_root, &current);
    let activation = read_art_activation(&art_root.join("active.json"))
        .ok_or_else(|| ArtInstallError::InvalidPackage("Art has no activation state".to_owned()))?;
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(
            "Art activation state contains an unsafe version path".to_owned(),
        ));
    }
    let versions_root = art_root.join("versions");
    if !versions_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut versions = Vec::new();
    for entry in std::fs::read_dir(&versions_root)? {
        let path = entry?.path();
        if !path.is_dir() || !path.join(MANIFEST_NAME).is_file() {
            continue;
        }
        let tool: ToolDefinition =
            serde_json::from_slice(&std::fs::read(path.join(MANIFEST_NAME))?)?;
        if tool.qualified_id() != identity {
            continue;
        }
        let security = read_art_package_security(&tool);
        let digest = canonical_package_digest(
            &path,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let relative = path
            .strip_prefix(&art_root)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if !relative.ends_with(&digest[..12]) {
            continue;
        }
        versions.push(ArtInstalledVersion {
            version: security.version.unwrap_or_else(|| "0.0.0".to_owned()),
            digest,
            active: activation.active.path == relative,
        });
    }
    versions.sort_by(|left, right| {
        match (
            semver::Version::parse(&left.version),
            semver::Version::parse(&right.version),
        ) {
            (Ok(left), Ok(right)) => right.cmp(&left),
            _ => right.version.cmp(&left.version),
        }
        .then_with(|| right.active.cmp(&left.active))
        .then_with(|| left.digest.cmp(&right.digest))
    });
    Ok(versions)
}

pub fn activate_art_version(
    control_plane_root: &Path,
    art_id: &str,
    target_version: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<ToolDefinition, ArtInstallError> {
    if semver::Version::parse(target_version).is_err() {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art target version `{target_version}` is not valid SemVer"
        )));
    }
    let current = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{art_id}` is not installed"))
        })?;
    let art_root = art_root_for_tool(control_plane_root, &current);
    let active_path = art_root.join("active.json");
    let activation = read_art_activation(&active_path)
        .ok_or_else(|| ArtInstallError::InvalidPackage("Art has no activation state".to_owned()))?;
    if activation.active.version == target_version {
        return Ok(current);
    }
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(
            "Art activation state contains an unsafe version path".to_owned(),
        ));
    }
    let identity = current.qualified_id();
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(art_root.join("versions"))? {
        let path = entry?.path();
        if !path.is_dir() || !path.join(MANIFEST_NAME).is_file() {
            continue;
        }
        let tool: ToolDefinition =
            serde_json::from_slice(&std::fs::read(path.join(MANIFEST_NAME))?)?;
        let security = read_art_package_security(&tool);
        if tool.qualified_id() != identity
            || security.version.as_deref().unwrap_or("0.0.0") != target_version
        {
            continue;
        }
        let digest = canonical_package_digest(
            &path,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let relative = path
            .strip_prefix(&art_root)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if !relative.ends_with(&digest[..12]) {
            return Err(ArtInstallError::InvalidPackage(
                "Art version directory does not match its digest".to_owned(),
            ));
        }
        matches.push(ArtVersionPointer {
            path: relative,
            version: target_version.to_owned(),
            digest: digest.clone(),
            lockfile: art_root
                .join("locks")
                .join(format!("{digest}.json"))
                .to_string_lossy()
                .to_string(),
        });
    }
    if matches.len() != 1 {
        return Err(ArtInstallError::InvalidPackage(format!(
            "Art `{art_id}` target version `{target_version}` is {}",
            if matches.is_empty() {
                "not installed"
            } else {
                "ambiguous because multiple package digests are installed"
            }
        )));
    }
    activate_art_pointer(
        control_plane_root,
        &art_root,
        &active_path,
        activation,
        matches.remove(0),
        tool_registry,
        framework_registry,
    )
}

pub fn rollback_art_package(
    control_plane_root: &Path,
    art_id: &str,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<ToolDefinition, ArtInstallError> {
    let current = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?
        .ok_or_else(|| {
            ArtInstallError::InvalidPackage(format!("Art `{art_id}` is not installed"))
        })?;
    let art_root = art_root_for_tool(control_plane_root, &current);
    let active_path = art_root.join("active.json");
    let activation = read_art_activation(&active_path)
        .ok_or_else(|| ArtInstallError::InvalidPackage("Art has no activation state".to_owned()))?;
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(
            "Art activation state contains an unsafe version path".to_owned(),
        ));
    }
    let previous = activation.previous.clone().ok_or_else(|| {
        ArtInstallError::InvalidPackage("Art has no previous version to roll back".to_owned())
    })?;
    activate_art_pointer(
        control_plane_root,
        &art_root,
        &active_path,
        activation,
        previous,
        tool_registry,
        framework_registry,
    )
}

fn activate_art_pointer(
    control_plane_root: &Path,
    art_root: &Path,
    active_path: &Path,
    activation: ArtActivationState,
    target: ArtVersionPointer,
    tool_registry: &ToolRegistry,
    framework_registry: &FrameworkRegistry,
) -> Result<ToolDefinition, ArtInstallError> {
    if !is_safe_art_version_path(&target.path) {
        return Err(ArtInstallError::InvalidPackage(
            "target Art package path is unsafe".to_owned(),
        ));
    }
    let target_dir = art_root.join(&target.path);
    if !target_dir.join(MANIFEST_NAME).is_file() {
        return Err(ArtInstallError::InvalidPackage(
            "target Art package is missing".to_owned(),
        ));
    }
    let mut tool: ToolDefinition =
        serde_json::from_slice(&std::fs::read(target_dir.join(MANIFEST_NAME))?)?;
    let security = read_art_package_security(&tool);
    let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let trust_status = verify_package_signature(
        &target_dir,
        security.publisher.as_ref(),
        security.signature.as_ref(),
        &trust_store,
    )
    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    if !activation.local_authoring && !activation.bundled_catalog {
        trust_store
            .effective_policy()
            .enforce(trust_status.clone())
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    }
    let digest = canonical_package_digest(
        &target_dir,
        security
            .signature
            .as_ref()
            .map(|signature| signature.file.as_str()),
    )
    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    if digest != target.digest || !target.path.ends_with(&digest[..12]) {
        return Err(ArtInstallError::InvalidPackage(format!(
            "target Art package digest does not match its immutable version pointer: expected {}, got {digest}",
            target.digest,
        )));
    }
    let mut verifying = std::collections::BTreeSet::from([tool.qualified_id()]);
    verify_art_lockfile(
        control_plane_root,
        Path::new(&target.lockfile),
        art_root,
        &target_dir,
        &tool,
        framework_registry,
        &mut verifying,
    )?;
    let state_dir = art_root.join("state");
    let cache_dir = art_root.join("cache");
    let output_dir = art_root.join("outputs");
    let qualified_id = qualified_art_id(&tool);
    rewrite_artloom_compat_execution_paths(&mut tool.metadata, &target_dir);
    record_art_package_directory(
        &mut tool.metadata,
        ArtPackagePaths {
            qualified_id: &qualified_id,
            art_dir: &target_dir,
            state_dir: &state_dir,
            cache_dir: &cache_dir,
            output_dir: &output_dir,
            lockfile: Path::new(&target.lockfile),
            version: &target.version,
            digest: &target.digest,
            trust_status: &trust_status,
        },
    );
    let next = ArtActivationState {
        active: target,
        previous: Some(activation.active.clone()),
        local_authoring: activation.local_authoring,
        bundled_catalog: activation.bundled_catalog,
    };
    write_art_lifecycle(
        &art_root,
        &ArtLifecycleJournal {
            old_activation: Some(activation.clone()),
            next_activation: next.clone(),
            target: next.active.path.clone(),
        },
    )?;
    write_art_activation(active_path, &next)?;
    if let Err(error) = tool_registry.save_tool(tool.clone()) {
        let _ = write_art_activation(active_path, &activation);
        clear_art_lifecycle(art_root);
        return Err(ArtInstallError::Registry(error.to_string()));
    }
    clear_art_lifecycle(art_root);
    Ok(tool)
}

fn verify_art_lockfile(
    control_plane_root: &Path,
    lockfile_path: &Path,
    art_root: &Path,
    art_dir: &Path,
    tool: &ToolDefinition,
    framework_registry: &FrameworkRegistry,
    verifying: &mut std::collections::BTreeSet<String>,
) -> Result<(), ArtInstallError> {
    let canonical_root = std::fs::canonicalize(art_root)?;
    let canonical_lockfile = std::fs::canonicalize(lockfile_path)?;
    if !canonical_lockfile.starts_with(&canonical_root) {
        return Err(ArtInstallError::InvalidPackage(
            "Art lockfile escapes the Art package root".to_owned(),
        ));
    }
    let lockfile: PluginLockfile = serde_json::from_slice(&std::fs::read(&canonical_lockfile)?)?;
    let security = read_art_package_security(tool);
    let expected_version = security.version.as_deref().unwrap_or("0.0.0");
    let expected_identity = tool.qualified_id();
    let legacy_unpublished_identity =
        tool.publisher_identity().is_none() && lockfile.package_id == tool.id;
    if lockfile.schema_version != loom_protocol::PLUGIN_LOCKFILE_SCHEMA_VERSION
        || (lockfile.package_id != expected_identity && !legacy_unpublished_identity)
        || lockfile.package_version != expected_version
    {
        return Err(ArtInstallError::InvalidPackage(
            "Art lockfile identity, version, or schema version is invalid".to_owned(),
        ));
    }
    let declared_arts = read_dependencies(tool).arts;
    validate_art_dependency_lock_set(&declared_arts, &lockfile.resolved)?;
    for dependency in &lockfile.resolved {
        match dependency.kind.as_str() {
            "framework" => {
                let status = framework_registry
                    .statuses()
                    .into_iter()
                    .find(|status| status.qualified_id == dependency.id)
                    .ok_or_else(|| ArtInstallError::FrameworkNotReady {
                        art_id: tool.id.clone(),
                        framework: dependency.id.clone(),
                        reason: "locked".to_owned(),
                    })?;
                if status.version.as_deref() != Some(dependency.version.as_str()) {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked framework `{}` version is no longer active",
                        dependency.id
                    )));
                }
                let runtime_dir = status.runtime_dir.ok_or_else(|| {
                    ArtInstallError::InvalidPackage(format!(
                        "locked framework `{}` has no runtime directory",
                        dependency.id
                    ))
                })?;
                let actual = canonical_package_digest(&runtime_dir, None)
                    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
                if actual != dependency.sha256 {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked framework `{}` digest mismatch",
                        dependency.id
                    )));
                }
            }
            "binary" => {
                let relative = Path::new(&dependency.id);
                if relative.is_absolute()
                    || relative.components().any(|component| {
                        matches!(
                            component,
                            Component::ParentDir | Component::RootDir | Component::Prefix(_)
                        )
                    })
                {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked binary path `{}` is invalid",
                        dependency.id
                    )));
                }
                let actual = sha256_hex(&std::fs::read(art_dir.join(relative))?);
                if actual != dependency.sha256 {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked binary `{}` digest mismatch",
                        dependency.id
                    )));
                }
            }
            "art" => {
                let locked_digest_is_valid = dependency.sha256.len() == 64
                    && dependency
                        .sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit());
                if !locked_digest_is_valid {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "locked Art dependency `{}` has an invalid digest",
                        dependency.id
                    )));
                }
                let (child_root, child_dir, child_tool) = locate_exact_installed_art_package(
                    control_plane_root,
                    &dependency.id,
                    &dependency.version,
                    &dependency.sha256,
                )?;
                if !verifying.insert(dependency.id.clone()) {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "Art dependency cycle detected at `{}`",
                        dependency.id
                    )));
                }
                let child_lockfile = child_root
                    .join("locks")
                    .join(format!("{}.json", dependency.sha256));
                let verified = verify_art_lockfile(
                    control_plane_root,
                    &child_lockfile,
                    &child_root,
                    &child_dir,
                    &child_tool,
                    framework_registry,
                    verifying,
                );
                verifying.remove(&dependency.id);
                verified?;
            }
            kind => {
                return Err(ArtInstallError::InvalidPackage(format!(
                    "Art lockfile contains unsupported dependency kind `{kind}`"
                )))
            }
        }
    }
    Ok(())
}

fn locate_exact_installed_art_package(
    control_plane_root: &Path,
    art_id: &str,
    version: &str,
    digest: &str,
) -> Result<(PathBuf, PathBuf, ToolDefinition), ArtInstallError> {
    let art_root = art_root_for_reference(control_plane_root, art_id).ok_or_else(|| {
        ArtInstallError::InvalidPackage(format!(
            "locked Art dependency `{art_id}` has an invalid identity"
        ))
    })?;
    let activation = read_art_activation(&art_root.join("active.json")).ok_or_else(|| {
        ArtInstallError::InvalidPackage(format!(
            "locked Art dependency `{art_id}` is not installed"
        ))
    })?;
    if !art_activation_is_safe(&activation) {
        return Err(ArtInstallError::InvalidPackage(format!(
            "locked Art dependency `{art_id}` has unsafe activation state"
        )));
    }
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(art_root.join("versions"))? {
        let path = entry?.path();
        if !path.is_dir() || !path.join(MANIFEST_NAME).is_file() {
            continue;
        }
        let tool: ToolDefinition =
            serde_json::from_slice(&std::fs::read(path.join(MANIFEST_NAME))?)?;
        let security = read_art_package_security(&tool);
        if tool.qualified_id() != art_id
            || security.version.as_deref().unwrap_or("0.0.0") != version
        {
            continue;
        }
        let actual = canonical_package_digest(
            &path,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if actual.eq_ignore_ascii_case(digest)
            && path.ends_with(format!("{version}-{}", &actual[..12]))
        {
            let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
                .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
            let trust_status = verify_package_signature(
                &path,
                security.publisher.as_ref(),
                security.signature.as_ref(),
                &trust_store,
            )
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
            if !activation.local_authoring && !activation.bundled_catalog {
                trust_store
                    .effective_policy()
                    .enforce(trust_status)
                    .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
            }
            matches.push((path, tool));
        }
    }
    if matches.len() != 1 {
        return Err(ArtInstallError::InvalidPackage(format!(
            "locked Art dependency `{art_id}` version `{version}` and digest `{digest}` is {}",
            if matches.is_empty() {
                "unavailable"
            } else {
                "ambiguous"
            }
        )));
    }
    let (art_dir, tool) = matches.remove(0);
    Ok((art_root, art_dir, tool))
}

fn art_reference_matches_qualified(reference: &str, qualified_id: &str) -> bool {
    if reference.contains('/') {
        reference == qualified_id
    } else {
        qualified_id == reference
            || qualified_id
                .rsplit_once('/')
                .is_some_and(|(_, id)| id == reference)
    }
}

fn validate_art_dependency_lock_set(
    declared: &[String],
    resolved: &[ResolvedDependency],
) -> Result<(), ArtInstallError> {
    let locked = resolved
        .iter()
        .filter(|dependency| dependency.kind == "art")
        .collect::<Vec<_>>();
    let mut matched = std::collections::BTreeSet::new();
    for reference in declared {
        if !is_safe_art_reference(reference) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency reference `{reference}` is invalid"
            )));
        }
        let matches = locked
            .iter()
            .filter(|dependency| art_reference_matches_qualified(reference, &dependency.id))
            .collect::<Vec<_>>();
        if matches.len() != 1 || !matched.insert(matches[0].id.clone()) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency `{reference}` is not represented by one exact lock"
            )));
        }
    }
    if matched.len() != locked.len() {
        return Err(ArtInstallError::InvalidPackage(
            "Art lockfile contains an undeclared or duplicate Art dependency".to_owned(),
        ));
    }
    Ok(())
}

fn resolve_art_root_for_uninstall(
    control_plane_root: &Path,
    art_id: &str,
    tool: Option<&ToolDefinition>,
) -> Result<PathBuf, ArtInstallError> {
    if let Some(tool) = tool {
        return Ok(art_root_for_tool(control_plane_root, tool));
    }

    let direct = art_root_for_reference(control_plane_root, art_id)
        .ok_or_else(|| ArtInstallError::InvalidArtId(art_id.to_owned()))?;
    if art_id.contains('/') || direct.exists() {
        return Ok(direct);
    }

    let arts_root = control_plane_root.join("arts");
    if !arts_root.is_dir() {
        return Ok(direct);
    }
    let mut publisher_matches = Vec::new();
    for entry in std::fs::read_dir(&arts_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(publisher) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !loom_protocol::is_safe_publisher_id(&publisher) {
            continue;
        }
        let candidate = entry.path().join(art_id);
        if candidate.is_dir() {
            publisher_matches.push(candidate);
        }
    }
    match publisher_matches.len() {
        0 => Ok(direct),
        1 => Ok(publisher_matches.remove(0)),
        _ => Err(ArtInstallError::InvalidPackage(format!(
            "Art id `{art_id}` is installed by multiple publishers; use a publisher-qualified id"
        ))),
    }
}

pub fn uninstall_art_package(
    control_plane_root: &Path,
    art_id: &str,
    tool_registry: &ToolRegistry,
) -> Result<(), ArtInstallError> {
    if !is_safe_art_reference(art_id) {
        return Err(ArtInstallError::InvalidArtId(art_id.to_owned()));
    }
    let tool = tool_registry
        .get_tool(art_id)
        .map_err(|error| ArtInstallError::Registry(error.to_string()))?;
    let art_root = resolve_art_root_for_uninstall(control_plane_root, art_id, tool.as_ref())?;
    let tombstone = if art_root.exists() {
        let tombstone = uninstall_tombstone_path(&art_root, ART_UNINSTALL_TOMBSTONE_PREFIX)?;
        std::fs::rename(&art_root, &tombstone)?;
        Some(tombstone)
    } else {
        None
    };
    if let Err(error) = tool_registry.delete_tool(art_id) {
        if let Some(tombstone) = &tombstone {
            let _ = std::fs::rename(tombstone, &art_root);
        }
        return Err(ArtInstallError::Registry(error.to_string()));
    }
    if let Some(tombstone) = tombstone {
        remove_tree(&tombstone)?;
    }
    Ok(())
}

fn resolve_art_dependency_locks(
    control_plane_root: &Path,
    art_references: &[String],
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<Vec<ResolvedDependency>, ArtInstallError> {
    let mut resolved = Vec::with_capacity(art_references.len());
    let mut identities = std::collections::BTreeSet::new();
    for reference in art_references {
        if !is_safe_art_reference(reference) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency reference `{reference}` is invalid"
            )));
        }
        let child = tool_registry
            .get_tool(reference)
            .map_err(|error| ArtInstallError::Registry(error.to_string()))?
            .ok_or_else(|| {
                ArtInstallError::InvalidPackage(format!(
                    "Art dependency `{reference}` is not installed"
                ))
            })?;
        let identity = child.qualified_id();
        if !identities.insert(identity.clone()) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency `{reference}` resolves to duplicate `{identity}`"
            )));
        }
        let pointer = {
            let mut verifying = std::collections::BTreeSet::new();
            verify_art_package_integrity_inner(
                control_plane_root,
                &child,
                framework_registry,
                &mut verifying,
            )?
        };
        resolved.push(ResolvedDependency {
            kind: "art".to_owned(),
            id: identity,
            version: pointer.version,
            sha256: pointer.digest,
        });
    }
    Ok(resolved)
}

fn write_art_lockfile(
    path: &Path,
    art_id: &str,
    art_version: &str,
    framework_id: &str,
    framework_registry: &FrameworkRegistry,
    binaries: &[ArtBinary],
    art_dir: &Path,
    art_dependencies: &[ResolvedDependency],
) -> Result<(), ArtInstallError> {
    let framework = framework_registry
        .statuses()
        .into_iter()
        .find(|status| status.qualified_id == framework_id || status.id == framework_id)
        .ok_or_else(|| ArtInstallError::FrameworkNotReady {
            art_id: art_id.to_owned(),
            framework: framework_id.to_owned(),
            reason: "missing status".to_owned(),
        })?;
    let framework_dir =
        framework
            .runtime_dir
            .ok_or_else(|| ArtInstallError::FrameworkNotReady {
                art_id: art_id.to_owned(),
                framework: framework_id.to_owned(),
                reason: "missing runtime directory".to_owned(),
            })?;
    let framework_digest = canonical_package_digest(&framework_dir, None)
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let mut resolved = vec![ResolvedDependency {
        kind: "framework".to_owned(),
        id: framework.qualified_id,
        version: framework.version.unwrap_or_else(|| "0.0.0".to_owned()),
        sha256: framework_digest,
    }];
    for binary in binaries {
        let bytes = std::fs::read(art_dir.join(binary.name.replace('\\', "/")))?;
        resolved.push(ResolvedDependency {
            kind: "binary".to_owned(),
            id: binary.name.clone(),
            version: "pinned".to_owned(),
            sha256: sha256_hex(&bytes),
        });
    }
    resolved.extend_from_slice(art_dependencies);
    let lockfile = PluginLockfile {
        schema_version: loom_protocol::PLUGIN_LOCKFILE_SCHEMA_VERSION,
        package_id: art_id.to_owned(),
        package_version: art_version.to_owned(),
        resolved,
    };
    let mut bytes = serde_json::to_vec_pretty(&lockfile)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    Ok(())
}

fn sanitize_version_for_path(version: &str) -> String {
    let sanitized = version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown".to_owned()
    } else {
        sanitized
    }
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
    if binary
        .sha256
        .as_deref()
        .is_none_or(|digest| digest.trim().is_empty())
    {
        return Err(ArtInstallError::RemoteBinaryHashRequired {
            name: binary.name.clone(),
        });
    }
    let url = binary
        .url
        .as_deref()
        .ok_or_else(|| ArtInstallError::BinaryMissing {
            name: binary.name.clone(),
        })?;
    let policy = crate::network_policy::OutboundPolicy {
        allow_http_loopback: true,
        ..crate::network_policy::OutboundPolicy::default()
    };
    let client = crate::network_policy::secure_client(
        "Loom/0.1 Art Binary Fetch",
        std::time::Duration::from_secs(120),
        policy.clone(),
    )
    .map_err(|error| ArtInstallError::BinaryDownloadFailed {
        name: binary.name.clone(),
        reason: error,
    })?;
    let bytes = crate::network_policy::get_bounded(&client, url, &policy, 128 * 1024 * 1024)
        .map_err(|error| ArtInstallError::BinaryDownloadFailed {
            name: binary.name.clone(),
            reason: error,
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
        let path = Path::new(&rel);
        if rel.trim().is_empty()
            || rel.contains(':')
            || path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ArtInstallError::InvalidPackage(format!(
                "art binary path must stay inside the package: {}",
                binary.name
            )));
        }
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
/// Dependencies are installed before their parent so the parent's lockfile can
/// pin each child to its exact qualified id, version, and digest. Reports remain
/// root-first for API compatibility.
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
    fn install_one<F>(
        zip: &[u8],
        requested_reference: Option<&str>,
        control_plane_root: &Path,
        framework_registry: &FrameworkRegistry,
        tool_registry: &ToolRegistry,
        fetch_dependent: &F,
        visiting: &mut std::collections::BTreeSet<String>,
        newly_installed: &mut Vec<String>,
    ) -> Result<Vec<ArtInstallReport>, ArtInstallError>
    where
        F: Fn(&str) -> Result<Vec<u8>, ArtInstallError>,
    {
        let manifest = read_manifest_from_zip(zip)?;
        if !is_safe_art_id(&manifest.id) {
            return Err(ArtInstallError::InvalidArtId(manifest.id));
        }
        let identity = manifest.qualified_id();
        let was_installed = tool_registry
            .list_tools()
            .map_err(|error| ArtInstallError::Registry(error.to_string()))?
            .iter()
            .any(|tool| tool.qualified_id() == identity);
        if let Some(reference) = requested_reference {
            if !art_reference_matches_qualified(reference, &identity) {
                return Err(ArtInstallError::InvalidPackage(format!(
                    "store dependency `{reference}` resolved to unexpected Art `{identity}`"
                )));
            }
        }
        if !visiting.insert(identity.clone()) {
            return Err(ArtInstallError::InvalidPackage(format!(
                "Art dependency cycle detected at `{identity}`"
            )));
        }

        let result = (|| {
            let dependencies = read_dependencies(&manifest).arts;
            let mut descendants = Vec::new();
            for reference in dependencies {
                if !is_safe_art_reference(&reference) {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "Art dependency reference `{reference}` is invalid"
                    )));
                }
                if visiting
                    .iter()
                    .any(|candidate| art_reference_matches_qualified(&reference, candidate))
                {
                    return Err(ArtInstallError::InvalidPackage(format!(
                        "Art dependency cycle detected through `{reference}`"
                    )));
                }
                if let Some(installed) = tool_registry
                    .get_tool(&reference)
                    .map_err(|error| ArtInstallError::Registry(error.to_string()))?
                {
                    verify_art_package_integrity(
                        control_plane_root,
                        &installed,
                        framework_registry,
                    )?;
                    continue;
                }
                let child_zip = fetch_dependent(&reference)?;
                descendants.extend(install_one(
                    &child_zip,
                    Some(&reference),
                    control_plane_root,
                    framework_registry,
                    tool_registry,
                    fetch_dependent,
                    visiting,
                    newly_installed,
                )?);
            }

            let report =
                install_art_from_zip(zip, control_plane_root, framework_registry, tool_registry)?;
            if !was_installed {
                newly_installed.push(identity.clone());
            }
            let mut reports = Vec::with_capacity(descendants.len() + 1);
            reports.push(report);
            reports.extend(descendants);
            Ok(reports)
        })();
        visiting.remove(&identity);
        result
    }

    let mut newly_installed = Vec::new();
    let result = install_one(
        root_zip,
        None,
        control_plane_root,
        framework_registry,
        tool_registry,
        fetch_dependent,
        &mut std::collections::BTreeSet::new(),
        &mut newly_installed,
    );
    if result.is_err() {
        for identity in newly_installed.into_iter().rev() {
            let _ = uninstall_art_package(control_plane_root, &identity, tool_registry);
        }
    }
    result
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

pub fn package_signed_art_to_zip(
    tool: &ToolDefinition,
    art_dir: &Path,
    publisher_id: &str,
    key: &SigningKeyDocument,
) -> Result<Vec<u8>, ArtInstallError> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging =
        std::env::temp_dir().join(format!("loom-art-sign-{}-{nonce}", std::process::id()));
    let result = (|| {
        std::fs::create_dir_all(&staging)?;
        if art_dir.is_dir() {
            copy_art_resources_for_signing(art_dir, art_dir, &staging)?;
        }
        let mut signed_tool = tool.clone();
        let metadata = signed_tool
            .metadata
            .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !metadata.is_object() {
            *metadata = serde_json::Value::Object(serde_json::Map::new());
        }
        let metadata = metadata
            .as_object_mut()
            .expect("Art metadata was normalized to an object");
        let security = metadata
            .entry("packageSecurity".to_owned())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !security.is_object() {
            *security = serde_json::Value::Object(serde_json::Map::new());
        }
        let security = security
            .as_object_mut()
            .expect("Art package security was normalized to an object");
        security.insert(
            "publisher".to_owned(),
            serde_json::json!({ "id": publisher_id, "keyId": key.key_id }),
        );
        security.insert(
            "signature".to_owned(),
            serde_json::json!({
                "algorithm": "ed25519",
                "keyId": key.key_id,
                "file": "signature.json"
            }),
        );
        std::fs::write(
            staging.join(MANIFEST_NAME),
            serde_json::to_vec_pretty(&signed_tool)?,
        )?;
        sign_package(&staging, "signature.json", key)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        package_art_to_zip(&signed_tool, &staging)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

fn copy_art_resources_for_signing(
    base: &Path,
    directory: &Path,
    staging: &Path,
) -> Result<(), ArtInstallError> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(ArtInstallError::InvalidPackage(format!(
                "signed Art resources cannot contain symbolic links: {}",
                entry.path().display()
            )));
        }
        let path = entry.path();
        let relative = path.strip_prefix(base).map_err(|_| {
            ArtInstallError::InvalidPackage("Art resource path escaped its package root".to_owned())
        })?;
        if relative == Path::new(MANIFEST_NAME) || relative == Path::new("signature.json") {
            continue;
        }
        let target = staging.join(relative);
        if file_type.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_art_resources_for_signing(base, &path, staging)?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, target)?;
        }
    }
    Ok(())
}

/// Build a newly-authored Art package without requiring a pre-existing package
/// directory. The resulting ZIP is consumed by the same secure installer as
/// imported packages, so authoring cannot bypass validation or activation.
pub fn build_authored_art_package_zip(
    tool: &ToolDefinition,
    runtime: Option<&ArtRuntimeManifest>,
    files: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, ArtInstallError> {
    use std::io::Write;
    tool.validate()
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file(MANIFEST_NAME, options)?;
        writer.write_all(&serde_json::to_vec_pretty(tool)?)?;
        if let Some(runtime) = runtime {
            writer.start_file("art.runtime.json", options)?;
            writer.write_all(&serde_json::to_vec_pretty(runtime)?)?;
        }
        let mut written = std::collections::BTreeSet::new();
        for (path, content) in files {
            let normalized = path.replace('\\', "/");
            let candidate = Path::new(&normalized);
            if normalized.is_empty()
                || normalized == MANIFEST_NAME
                || normalized == "art.runtime.json"
                || candidate.is_absolute()
                || candidate.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::ParentDir
                            | std::path::Component::RootDir
                            | std::path::Component::Prefix(_)
                    )
                })
                || !written.insert(normalized.clone())
            {
                return Err(ArtInstallError::InvalidPackage(format!(
                    "invalid authored Art file path: {path}"
                )));
            }
            writer.start_file(normalized, options)?;
            writer.write_all(content)?;
        }
        writer.finish()?;
    }
    Ok(bytes)
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
    use crate::ToolExecution;
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

    fn signed_art_zip(
        id: &str,
        version: &str,
        publisher: &str,
        payload: &[u8],
        key: &loom_plugin_security::SigningKeyDocument,
    ) -> Vec<u8> {
        let package_root = temp_root();
        let package = package_root.join("signed-art");
        std::fs::create_dir_all(package.join("bin")).expect("package directory");
        let manifest = serde_json::json!({
            "id": id,
            "name": "Signed Art",
            "description": "signed rollback fixture",
            "enabled": true,
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": {
                "packageSecurity": {
                    "version": version,
                    "publisher": { "id": publisher, "keyId": key.key_id.clone() },
                    "signature": {
                        "algorithm": "ed25519",
                        "keyId": key.key_id.clone(),
                        "file": "signature.json"
                    }
                }
            }
        });
        std::fs::write(
            package.join(MANIFEST_NAME),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .expect("manifest");
        std::fs::write(package.join("bin/tool.exe"), payload).expect("payload");
        loom_plugin_security::sign_package(&package, "signature.json", key).expect("sign Art");

        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
            let options = SimpleFileOptions::default();
            for relative in [MANIFEST_NAME, "bin/tool.exe", "signature.json"] {
                writer.start_file(relative, options).unwrap();
                writer
                    .write_all(&std::fs::read(package.join(relative)).unwrap())
                    .unwrap();
            }
            writer.finish().unwrap();
        }
        std::fs::remove_dir_all(package_root).ok();
        bytes
    }

    fn install_test_framework(framework: &FrameworkRegistry, id: &str) {
        let command = match id {
            "process" => "runtime/loom-framework-process.exe",
            "cloud_api" => "runtime/loom-framework-cloud-api.exe",
            "mcp" => "runtime/loom-framework-mcp.exe",
            "workflow" => "runtime/loom-framework-workflow.exe",
            other => panic!("unknown test framework: {other}"),
        };
        let manifest = serde_json::json!({
            "id": id,
            "name": format!("{id} test framework"),
            "description": "test framework",
            "version": "0.1.0",
            "protocolVersion": "loom.framework.v1",
            "platforms": ["windows-x64"],
            "entry": { "kind": "process", "command": command, "args": ["--stdio"] },
            "permissions": ["process.spawn"],
            "artExecution": {
                "requestSchema": "loom.art.execute.v1",
                "responseSchema": "loom.art.result.v1"
            }
        });
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
            let opts = SimpleFileOptions::default();
            writer.start_file("framework.manifest.json", opts).unwrap();
            writer
                .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
                .unwrap();
            writer.start_file(command, opts).unwrap();
            writer.write_all(b"MZ-test-framework").unwrap();
            writer.finish().unwrap();
        }
        framework
            .install_framework_package_from_zip(&bytes)
            .expect("install test framework");
    }

    #[test]
    fn reads_manifest_from_zip() {
        let manifest = r#"{"id":"art-x","name":"X","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        let zip = build_zip(manifest, &[]);
        let tool = read_manifest_from_zip(&zip).expect("read manifest");
        assert_eq!(tool.id, "art-x");
    }

    #[test]
    fn authored_package_includes_runtime_and_package_local_files() {
        let tool = ToolDefinition::new(
            "authored-process-art",
            "Authored Process Art",
            "authored package fixture",
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
        );
        let runtime: ArtRuntimeManifest = serde_json::from_value(serde_json::json!({
            "protocolVersion": "loom.art.runtime.v1",
            "entry": {
                "command": "python.exe",
                "args": ["runtime/main.py"]
            }
        }))
        .expect("runtime manifest");
        let zip = build_authored_art_package_zip(
            &tool,
            Some(&runtime),
            &[
                ("runtime/main.py".to_owned(), b"print('ok')\n".to_vec()),
                ("runtime/data/config.json".to_owned(), b"{}\n".to_vec()),
            ],
        )
        .expect("build authored package");

        let mut archive = zip::ZipArchive::new(Cursor::new(zip)).expect("open authored package");
        for expected in [
            "manifest.json",
            "art.runtime.json",
            "runtime/main.py",
            "runtime/data/config.json",
        ] {
            assert!(archive.by_name(expected).is_ok(), "missing {expected}");
        }
        let mut source = String::new();
        archive
            .by_name("runtime/main.py")
            .expect("runtime source")
            .read_to_string(&mut source)
            .expect("read runtime source");
        assert_eq!(source, "print('ok')\n");
    }

    #[test]
    fn authored_package_rejects_reserved_unsafe_and_duplicate_paths() {
        let tool = ToolDefinition::new(
            "invalid-authored-process-art",
            "Invalid Authored Process Art",
            "invalid authored package fixture",
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
        );

        for path in [
            "manifest.json",
            "art.runtime.json",
            "../escape.py",
            "C:/escape.py",
        ] {
            let error =
                build_authored_art_package_zip(&tool, None, &[(path.to_owned(), Vec::new())])
                    .expect_err("unsafe authored path must fail");
            assert!(error.to_string().contains("invalid authored Art file path"));
        }

        let error = build_authored_art_package_zip(
            &tool,
            None,
            &[
                ("runtime/main.py".to_owned(), Vec::new()),
                ("runtime/main.py".to_owned(), Vec::new()),
            ],
        )
        .expect_err("duplicate authored path must fail");
        assert!(error.to_string().contains("invalid authored Art file path"));
    }

    #[test]
    fn installs_package_extracts_files_and_rewrites_paths() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"pingo-art","name":"Pingo","description":"compress","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        let zip = build_zip(manifest, &[("bin/pingo.exe", b"MZ-fake-exe")]);

        let report = install_art_from_zip(&zip, &root, &framework, &registry).expect("install art");
        assert_eq!(report.tool_id, "pingo-art");
        assert_eq!(report.framework, "process");
        // Binary extracted into the art dir.
        assert!(report.art_dir.join("bin/pingo.exe").exists());
        assert!(report
            .installed_files
            .iter()
            .any(|f| f.contains("pingo.exe")));

        // Registered tool keeps the generic process framework identity.
        let saved = registry.get_tool("pingo-art").unwrap().unwrap();
        assert_eq!(
            saved.metadata.as_ref().unwrap()["artloomCompat"]["source"],
            "loom-local"
        );
        assert!(matches!(
            saved.execution,
            crate::ToolExecution::FrameworkArt { ref framework } if framework == "process"
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn strict_trust_policy_allows_local_and_bundled_sources_but_rejects_external_unsigned_packages()
    {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let mut trust = TrustStore::default();
        trust.set_policy(loom_plugin_security::TrustPolicy::RequireSigned);
        trust
            .write_atomic(&root.join("plugin-trust.json"))
            .expect("write strict trust policy");
        let manifest = r#"{"id":"local-draft","name":"Local Draft","description":"draft","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        let zip = build_zip(manifest, &[("runtime/main.txt", b"local")]);

        let external_error = install_art_from_zip(&zip, &root, &framework, &registry)
            .expect_err("external unsigned package must remain rejected");
        assert!(matches!(
            external_error,
            ArtInstallError::InvalidPackage(reason)
                if reason.contains("trust policy rejected package status Unsigned")
        ));

        let report = install_authored_art_from_zip(&zip, &root, &framework, &registry)
            .expect("local authored draft must bypass external install policy");
        assert_eq!(report.trust_status, PackageTrustStatus::Unsigned);
        let saved = registry
            .get_tool("local-draft")
            .expect("read local draft")
            .expect("local draft registered");
        verify_art_package_integrity(&root, &saved, &framework)
            .expect("local draft integrity must remain verifiable");
        let activation = read_art_activation(&root.join("arts/local-draft/active.json"))
            .expect("local draft activation");
        assert!(activation.local_authoring);
        assert!(!activation.bundled_catalog);

        let bundled_manifest = r#"{"id":"bundled-draft","name":"Bundled Draft","description":"catalog","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        let bundled_zip = build_zip(bundled_manifest, &[("runtime/main.txt", b"bundled")]);
        let bundled_report =
            install_bundled_art_from_zip(&bundled_zip, &root, &framework, &registry).expect(
                "checksum-verified bundled catalog package must bypass user install policy",
            );
        assert_eq!(bundled_report.trust_status, PackageTrustStatus::Unsigned);
        let bundled = registry
            .get_tool("bundled-draft")
            .expect("read bundled draft")
            .expect("bundled draft registered");
        verify_art_package_integrity(&root, &bundled, &framework)
            .expect("bundled catalog package integrity must remain verifiable");
        let bundled_activation = read_art_activation(&root.join("arts/bundled-draft/active.json"))
            .expect("bundled draft activation");
        assert!(!bundled_activation.local_authoring);
        assert!(bundled_activation.bundled_catalog);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_replaces_obsolete_legacy_layout_and_registry_entry() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry_root = root.join("tools");
        std::fs::create_dir_all(&registry_root).expect("registry directory");
        let registry = ToolRegistry::new(&registry_root);

        let legacy_manifest = serde_json::json!({
            "id": "legacy-art",
            "name": "Legacy Art",
            "description": "obsolete CLI package",
            "enabled": true,
            "execution": { "type": "cli_wrapper", "command": "bin/legacy.exe" },
            "metadata": { "dependencies": { "framework": "cli_wrapper" } }
        });
        std::fs::write(
            registry_root.join("tools.json"),
            serde_json::to_vec_pretty(&serde_json::json!([legacy_manifest.clone()]))
                .expect("serialize legacy registry"),
        )
        .expect("legacy registry");
        let legacy_root = root.join("arts/legacy-art");
        std::fs::create_dir_all(legacy_root.join("bin")).expect("legacy Art directory");
        std::fs::write(
            legacy_root.join(MANIFEST_NAME),
            serde_json::to_vec_pretty(&legacy_manifest).expect("serialize legacy manifest"),
        )
        .expect("legacy manifest");
        std::fs::write(legacy_root.join("bin/legacy.exe"), b"obsolete").expect("legacy payload");

        let current_manifest = serde_json::json!({
            "id": "legacy-art",
            "name": "Current Art",
            "description": "current process framework package",
            "enabled": true,
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": {
                "packageSecurity": {
                    "version": "0.1.0",
                    "publisher": { "id": "neuro.official", "name": "Neuro" }
                },
                "dependencies": { "framework": "process" }
            }
        });
        let zip = build_zip(
            &serde_json::to_string(&current_manifest).expect("serialize current manifest"),
            &[("runtime/current.txt", b"current")],
        );

        let report =
            install_art_from_zip(&zip, &root, &framework, &registry).expect("replace obsolete Art");
        assert!(report
            .art_dir
            .starts_with(root.join("arts/neuro.official/legacy-art")));
        assert!(report.art_dir.join("runtime/current.txt").is_file());
        assert!(!root.join("arts/legacy-art").exists());
        assert!(!report.art_dir.join("bin/legacy.exe").exists());

        let tools = registry.list_tools().expect("list migrated tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].qualified_id(), "neuro.official/legacy-art");
        assert!(matches!(
            tools[0].execution,
            ToolExecution::FrameworkArt { ref framework } if framework == "process"
        ));
        assert_eq!(
            std::fs::read_dir(&registry_root)
                .expect("registry backups")
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("tools.json.corrupt-"))
                .count(),
            1
        );
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("control-plane root")
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".loom-art-legacy-"))
                .count(),
            0
        );

        remove_tree(&root).ok();
    }

    #[test]
    fn publisher_namespace_keeps_same_art_id_in_separate_roots() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let package = |publisher: &str, marker: &'static [u8]| {
            let manifest = serde_json::json!({
                "id": "shared-art",
                "name": publisher,
                "description": "publisher scoped",
                "enabled": true,
                "execution": { "type": "framework_art", "framework": "process" },
                "metadata": {
                    "packageSecurity": {
                        "version": "0.1.0",
                        "publisher": { "id": publisher, "name": publisher }
                    }
                }
            });
            build_zip(
                &serde_json::to_string(&manifest).expect("serialize Art manifest"),
                &[("bin/tool.exe", marker)],
            )
        };
        let alpha = install_art_from_zip(
            &package("publisher.alpha", b"alpha"),
            &root,
            &framework,
            &registry,
        )
        .expect("install alpha Art");
        let beta = install_art_from_zip(
            &package("publisher.beta", b"beta"),
            &root,
            &framework,
            &registry,
        )
        .expect("install beta Art");

        assert_ne!(alpha.art_dir, beta.art_dir);
        assert!(alpha.art_dir.starts_with(root.join("arts/publisher.alpha")));
        assert!(beta.art_dir.starts_with(root.join("arts/publisher.beta")));
        assert!(matches!(
            registry.get_tool("shared-art"),
            Err(crate::ToolRegistryError::AmbiguousToolId { .. })
        ));
        uninstall_art_package(&root, "publisher.alpha/shared-art", &registry)
            .expect("uninstall alpha");
        assert!(registry
            .get_tool("publisher.beta/shared-art")
            .expect("get beta")
            .is_some());
        remove_tree(&root).ok();
    }

    #[test]
    fn unqualified_uninstall_recovers_a_unique_package_missing_from_the_registry() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = serde_json::json!({
            "id": "orphan-art",
            "name": "Orphan Art",
            "description": "package remains after a registry-only deletion",
            "enabled": true,
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": {
                "packageSecurity": {
                    "version": "0.1.0",
                    "publisher": { "id": "publisher.test", "name": "Publisher Test" }
                }
            }
        });
        install_art_from_zip(
            &build_zip(
                &serde_json::to_string(&manifest).expect("serialize orphan Art manifest"),
                &[],
            ),
            &root,
            &framework,
            &registry,
        )
        .expect("install orphan Art");
        let package_root = root.join("arts/publisher.test/orphan-art");
        assert!(package_root.is_dir());
        registry
            .delete_tool("publisher.test/orphan-art")
            .expect("delete orphan registry entry");

        uninstall_art_package(&root, "orphan-art", &registry)
            .expect("resolve and uninstall orphan package");

        assert!(!package_root.exists());
        remove_tree(&root).ok();
    }

    #[test]
    fn install_preserves_process_framework_identity() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"shell-copy-art","name":"Shell Copy","description":"copy","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        let zip = build_zip(manifest, &[]);

        install_art_from_zip(&zip, &root, &framework, &registry).expect("install shell art");
        let saved = registry.get_tool("shell-copy-art").unwrap().unwrap();
        assert!(matches!(
            saved.execution,
            crate::ToolExecution::FrameworkArt { ref framework } if framework == "process"
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn verifies_bundled_binary_hash_and_reports_it() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));

        let exe = b"MZ-fake-exe";
        let digest = sha256_hex(exe);
        let manifest = format!(
            r#"{{"id":"pingo-hashed","name":"Pingo","description":"c","enabled":true,
            "execution":{{"type":"framework_art","framework":"process"}},
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
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"pingo-badhash","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
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
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"pingo-nobs","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"binaries":[{"name":"bin/missing.exe"}]}}}"#;
        let zip = build_zip(manifest, &[]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(matches!(err, ArtInstallError::BinaryMissing { .. }));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_remote_binary_without_sha256_before_downloading() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"pingo-unpinned","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"binaries":[{"name":"bin/pingo.exe","url":"http://127.0.0.1:1/pingo.exe"}]}}}"#;
        let zip = build_zip(manifest, &[]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(matches!(
            err,
            ArtInstallError::RemoteBinaryHashRequired { .. }
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_binary_path_that_escapes_the_art_package() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));

        let manifest = r#"{"id":"pingo-escape","name":"Pingo","description":"c","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"binaries":[{"name":"../escape.exe","url":"http://127.0.0.1:1/escape.exe"}]}}}"#;
        let zip = build_zip(manifest, &[]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(matches!(
            err,
            ArtInstallError::InvalidPackage(reason)
                if reason.contains("must stay inside the package")
        ));
        assert!(!root.join("escape.exe").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_install_when_framework_not_installed() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        let registry = ToolRegistry::new(root.join("tools"));
        // process is NOT installed by default.
        let manifest = r#"{"id":"py-art","name":"Py","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
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
            "execution":{"type":"framework_art","framework":"process"}}"#;
        let zip = build_zip(manifest, &[]);
        let err = install_art_from_zip(&zip, &root, &framework, &registry).unwrap_err();
        assert!(matches!(err, ArtInstallError::InvalidArtId(_)));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn direct_install_rejects_unlocked_dependent_arts() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "workflow");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"wf-art","name":"WF","description":"d","enabled":true,
            "execution":{"type":"workflow","workflowId":"wf"},
            "metadata":{"dependencies":{"framework":"workflow","arts":["dep-1","dep-2"]}}}"#;
        let zip = build_zip(manifest, &[]);
        let error = install_art_from_zip(&zip, &root, &framework, &registry)
            .expect_err("direct install must not activate an Art with missing dependencies");
        assert!(matches!(
            error,
            ArtInstallError::InvalidPackage(reason)
                if reason.contains("Art dependency `dep-1` is not installed")
        ));
        assert!(registry.get_tool("wf-art").unwrap().is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recursive_install_pulls_dependent_arts() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "workflow");
        install_test_framework(&framework, "cloud_api");
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
        assert_eq!(reports[0].dependent_arts, vec!["dep-1"]);
        assert!(registry.get_tool("dep-2").unwrap().is_some());

        let root_activation =
            read_art_activation(&root.join("arts/root-wf/active.json")).expect("root activation");
        let root_lock: PluginLockfile = serde_json::from_slice(
            &std::fs::read(&root_activation.active.lockfile).expect("root lockfile"),
        )
        .expect("parse root lockfile");
        let locked_dep = root_lock
            .resolved
            .iter()
            .find(|dependency| dependency.kind == "art")
            .expect("root child lock");
        let dep_activation =
            read_art_activation(&root.join("arts/dep-1/active.json")).expect("dep activation");
        assert_eq!(locked_dep.id, "dep-1");
        assert_eq!(locked_dep.version, dep_activation.active.version);
        assert_eq!(locked_dep.sha256, dep_activation.active.digest);

        let dep_tool = registry.get_tool("dep-1").unwrap().unwrap();
        verify_art_package_integrity(&root, &dep_tool, &framework).expect("verify child graph");
        let root_tool = registry.get_tool("root-wf").unwrap().unwrap();
        verify_art_package_integrity(&root, &root_tool, &framework).expect("verify root graph");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn recursive_install_rolls_back_new_children_when_parent_fails() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "cloud_api");
        let registry = ToolRegistry::new(root.join("tools"));
        let parent = serde_json::json!({
            "id": "failing-parent",
            "name": "Failing Parent",
            "description": "missing workflow framework",
            "enabled": true,
            "execution": { "type": "workflow", "workflowId": "wf" },
            "metadata": {
                "dependencies": { "framework": "workflow", "arts": ["new-child"] }
            }
        });
        let child = serde_json::json!({
            "id": "new-child",
            "name": "New Child",
            "description": "child",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": "https://example.invalid/process",
                "method": "POST"
            }
        });
        let child_zip = build_zip(&child.to_string(), &[]);
        let error = install_art_recursive(
            &build_zip(&parent.to_string(), &[]),
            &root,
            &framework,
            &registry,
            &|id| {
                if id == "new-child" {
                    Ok(child_zip.clone())
                } else {
                    Err(ArtInstallError::InvalidPackage(format!("no Art `{id}`")))
                }
            },
        )
        .expect_err("parent framework failure must abort the graph install");
        assert!(matches!(error, ArtInstallError::FrameworkNotReady { .. }));
        assert!(registry.get_tool("new-child").unwrap().is_none());
        assert!(registry.get_tool("failing-parent").unwrap().is_none());
        assert!(!root.join("arts/new-child").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn parent_art_lock_resolves_immutable_child_across_active_upgrade_and_rollback() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "workflow");
        install_test_framework(&framework, "cloud_api");
        let registry = ToolRegistry::new(root.join("tools"));

        let child_zip = |version: &str, payload: &'static [u8]| {
            let manifest = serde_json::json!({
                "id": "locked-child",
                "name": "Locked Child",
                "description": "child",
                "enabled": true,
                "execution": {
                    "type": "cloud_api",
                    "endpoint": "https://example.invalid/process",
                    "method": "POST"
                },
                "metadata": {
                    "packageSecurity": { "version": version }
                }
            });
            build_zip(&manifest.to_string(), &[("payload.bin", payload)])
        };
        let parent_zip = |version: &str| {
            let manifest = serde_json::json!({
                "id": "locked-parent",
                "name": "Locked Parent",
                "description": "parent",
                "enabled": true,
                "execution": { "type": "workflow", "workflowId": "locked-workflow" },
                "metadata": {
                    "packageSecurity": { "version": version },
                    "dependencies": {
                        "framework": "workflow",
                        "arts": ["locked-child"]
                    }
                }
            });
            build_zip(&manifest.to_string(), &[])
        };

        install_art_from_zip(
            &child_zip("1.0.0", b"child-one"),
            &root,
            &framework,
            &registry,
        )
        .expect("install child v1");
        install_art_from_zip(&parent_zip("1.0.0"), &root, &framework, &registry)
            .expect("install parent locked to child v1");
        let parent_v1 = registry.get_tool("locked-parent").unwrap().unwrap();
        let parent_v1_version = parent_v1
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/artPackage/version"))
            .and_then(serde_json::Value::as_str)
            .expect("parent v1 version")
            .to_owned();
        let parent_v1_digest = parent_v1
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/artPackage/digest"))
            .and_then(serde_json::Value::as_str)
            .expect("parent v1 digest")
            .to_owned();
        verify_art_package_integrity(&root, &parent_v1, &framework).expect("verify parent v1");

        install_art_from_zip(
            &child_zip("2.0.0", b"child-two"),
            &root,
            &framework,
            &registry,
        )
        .expect("upgrade child");
        verify_art_package_integrity(&root, &parent_v1, &framework)
            .expect("parent v1 remains bound to installed child v1");
        let resolved_parent_v1 = resolve_installed_art_package(
            &root,
            "locked-parent",
            &parent_v1_version,
            &parent_v1_digest,
            &registry,
            &framework,
        )
        .expect("resolve immutable parent v1 after child upgrade");
        assert_eq!(
            resolved_parent_v1
                .metadata
                .as_ref()
                .and_then(|metadata| {
                    metadata
                        .pointer("/artPackage/lockedArts/locked-child/metadata/artPackage/version")
                })
                .and_then(serde_json::Value::as_str),
            Some("1.0.0")
        );

        install_art_from_zip(&parent_zip("2.0.0"), &root, &framework, &registry)
            .expect("refresh parent lock for child v2");
        let parent_v2 = registry.get_tool("locked-parent").unwrap().unwrap();
        verify_art_package_integrity(&root, &parent_v2, &framework)
            .expect("verify refreshed parent");

        rollback_art_package(&root, "locked-child", &registry, &framework)
            .expect("rollback child to v1");
        verify_art_package_integrity(&root, &parent_v2, &framework)
            .expect("parent v2 remains bound to installed child v2");
        let parent_rolled_back =
            rollback_art_package(&root, "locked-parent", &registry, &framework)
                .expect("rollback parent to lock matching child v1");
        verify_art_package_integrity(&root, &parent_rolled_back, &framework)
            .expect("verify rolled-back parent and child lock");

        let child_root = root.join("arts/locked-child");
        let child_activation =
            read_art_activation(&child_root.join("active.json")).expect("child activation");
        let child_dir = child_root.join(&child_activation.active.path);
        set_tree_readonly(&child_dir, false).expect("unlock child fixture");
        std::fs::write(child_dir.join("payload.bin"), b"tampered").expect("tamper child");
        assert!(verify_art_package_integrity(&root, &parent_rolled_back, &framework).is_err());
        std::fs::write(child_dir.join("payload.bin"), b"child-one").expect("restore child");

        let mut tampered_activation = child_activation.clone();
        tampered_activation.active.version = "9.9.9".to_owned();
        write_art_activation(&child_root.join("active.json"), &tampered_activation)
            .expect("tamper child activation");
        verify_art_package_integrity(&root, &parent_rolled_back, &framework)
            .expect("active pointer metadata does not change an immutable child lock");
        write_art_activation(&child_root.join("active.json"), &child_activation)
            .expect("restore child activation");

        uninstall_art_package(&root, "locked-child", &registry).expect("uninstall child");
        assert!(verify_art_package_integrity(&root, &parent_rolled_back, &framework).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn parent_art_lock_uses_publisher_qualified_child_identity() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "workflow");
        install_test_framework(&framework, "cloud_api");
        let registry = ToolRegistry::new(root.join("tools"));
        let child = serde_json::json!({
            "id": "shared-child",
            "name": "Qualified Child",
            "description": "child",
            "enabled": true,
            "execution": {
                "type": "cloud_api",
                "endpoint": "https://example.invalid/process",
                "method": "POST"
            },
            "metadata": {
                "packageSecurity": {
                    "version": "1.0.0",
                    "publisher": { "id": "publisher.alpha" }
                }
            }
        });
        install_art_from_zip(
            &build_zip(&child.to_string(), &[]),
            &root,
            &framework,
            &registry,
        )
        .expect("install qualified child");
        let parent = serde_json::json!({
            "id": "qualified-parent",
            "name": "Qualified Parent",
            "description": "parent",
            "enabled": true,
            "execution": { "type": "workflow", "workflowId": "qualified" },
            "metadata": {
                "packageSecurity": {
                    "version": "1.0.0",
                    "publisher": { "id": "publisher.parent" }
                },
                "dependencies": {
                    "framework": "workflow",
                    "arts": ["publisher.alpha/shared-child"]
                }
            }
        });
        install_art_from_zip(
            &build_zip(&parent.to_string(), &[]),
            &root,
            &framework,
            &registry,
        )
        .expect("install parent");

        let activation =
            read_art_activation(&root.join("arts/publisher.parent/qualified-parent/active.json"))
                .expect("parent activation");
        let lock: PluginLockfile = serde_json::from_slice(
            &std::fs::read(&activation.active.lockfile).expect("parent lockfile"),
        )
        .expect("parse parent lockfile");
        assert!(lock.resolved.iter().any(|dependency| {
            dependency.kind == "art" && dependency.id == "publisher.alpha/shared-child"
        }));
        let tool = registry
            .get_tool("publisher.parent/qualified-parent")
            .unwrap()
            .unwrap();
        verify_art_package_integrity(&root, &tool, &framework).expect("verify qualified lock");

        let mut bare_parent_lock = lock;
        bare_parent_lock.package_id = "qualified-parent".to_owned();
        std::fs::write(
            &activation.active.lockfile,
            serde_json::to_vec_pretty(&bare_parent_lock).unwrap(),
        )
        .expect("tamper parent lock identity");
        assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn package_roundtrips_installed_art() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"pkg-art","name":"Pkg","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
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
    fn installs_framework_art_and_records_external_package_directory() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{
            "id":"external-script-art","name":"External Script Art","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"dependencies":{"framework":"process"}}
        }"#;
        let zip = build_zip(manifest, &[("resources/input.txt", b"fixture")]);
        let report = install_art_from_zip(&zip, &root, &framework, &registry).expect("install");
        let saved = registry
            .get_tool("external-script-art")
            .expect("get tool")
            .expect("saved tool");
        assert!(matches!(
            saved.execution,
            ToolExecution::FrameworkArt { ref framework } if framework == "process"
        ));
        let expected_dir = report.art_dir.to_string_lossy().to_string();
        assert_eq!(
            saved
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("artPackage"))
                .and_then(|package| package.get("dir"))
                .and_then(serde_json::Value::as_str),
            Some(expected_dir.as_str())
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn art_upgrade_rollback_and_integrity_verification_roundtrip() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"rollback-art","name":"Rollback","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#;
        install_art_from_zip(
            &build_zip(manifest, &[("bin/tool.exe", b"version-one")]),
            &root,
            &framework,
            &registry,
        )
        .expect("install first Art");
        install_art_from_zip(
            &build_zip(manifest, &[("bin/tool.exe", b"version-two")]),
            &root,
            &framework,
            &registry,
        )
        .expect("install second Art");
        let current = registry.get_tool("rollback-art").unwrap().unwrap();
        verify_art_package_integrity(&root, &current, &framework).expect("verify current Art");
        let installed = list_installed_art_versions(&root, "rollback-art", &registry)
            .expect("list immutable versions");
        assert_eq!(installed.len(), 2);
        let payloads = installed
            .iter()
            .map(|version| {
                let pinned = resolve_installed_art_package(
                    &root,
                    "rollback-art",
                    &version.version,
                    &version.digest,
                    &registry,
                    &framework,
                )
                .expect("resolve pinned package");
                let directory = pinned
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("artPackage"))
                    .and_then(|package| package.get("dir"))
                    .and_then(serde_json::Value::as_str)
                    .expect("pinned Art directory");
                std::fs::read(Path::new(directory).join("bin/tool.exe"))
                    .expect("read pinned payload")
            })
            .collect::<Vec<_>>();
        assert!(payloads.iter().any(|payload| payload == b"version-one"));
        assert!(payloads.iter().any(|payload| payload == b"version-two"));

        let rolled_back = rollback_art_package(&root, "rollback-art", &registry, &framework)
            .expect("rollback Art");
        verify_art_package_integrity(&root, &rolled_back, &framework)
            .expect("verify rolled-back Art");
        let active = resolve_active_art_package(&root, "rollback-art").expect("active Art");
        assert_eq!(
            std::fs::read(active.join("bin/tool.exe")).unwrap(),
            b"version-one"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn art_integrity_and_rollback_reject_revoked_publisher_versions() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let key = loom_plugin_security::generate_signing_key("art-release-key");
        framework
            .trust_publisher(loom_protocol::PublisherTrustRecord {
                publisher_id: "publisher.art".to_owned(),
                key_id: key.key_id.clone(),
                public_key: key.public_key.clone(),
                revoked: false,
            })
            .expect("trust publisher");
        for (version, payload) in [("1.0.0", b"one".as_slice()), ("2.0.0", b"two".as_slice())] {
            install_art_from_zip(
                &signed_art_zip("signed-art", version, "publisher.art", payload, &key),
                &root,
                &framework,
                &registry,
            )
            .unwrap_or_else(|error| panic!("install {version}: {error}"));
        }
        framework
            .revoke_publisher("publisher.art", &key.key_id)
            .expect("revoke publisher");
        let tool = registry
            .get_tool("publisher.art/signed-art")
            .expect("read tool")
            .expect("installed tool");
        assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());
        assert!(
            rollback_art_package(&root, "publisher.art/signed-art", &registry, &framework,)
                .is_err()
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn art_integrity_verification_rejects_package_and_lockfile_tampering() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"tamper-art","name":"Tamper","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        let report = install_art_from_zip(
            &build_zip(manifest, &[("bin/tool.exe", b"original")]),
            &root,
            &framework,
            &registry,
        )
        .expect("install Art");
        let tool = registry.get_tool("tamper-art").unwrap().unwrap();
        set_tree_readonly(&report.art_dir, false).expect("unlock test package");
        std::fs::write(report.art_dir.join("bin/tool.exe"), b"tampered").unwrap();
        assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());

        std::fs::write(report.art_dir.join("bin/tool.exe"), b"original").unwrap();
        let art_root = root.join("arts/tamper-art");
        let activation = read_art_activation(&art_root.join("active.json")).unwrap();
        let mut lock: PluginLockfile = serde_json::from_slice(
            &std::fs::read(&activation.active.lockfile).expect("read Art lockfile"),
        )
        .unwrap();
        let original_version = lock.package_version.clone();
        lock.package_version = "9.9.9".to_owned();
        std::fs::write(
            &activation.active.lockfile,
            serde_json::to_vec_pretty(&lock).unwrap(),
        )
        .unwrap();
        assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());

        lock.package_version = original_version;
        lock.schema_version = u32::MAX;
        std::fs::write(
            &activation.active.lockfile,
            serde_json::to_vec_pretty(&lock).unwrap(),
        )
        .unwrap();
        assert!(verify_art_package_integrity(&root, &tool, &framework).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn art_recovery_restores_activation_and_rejects_unsafe_journal_paths() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"recover-art","name":"Recover","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        install_art_from_zip(
            &build_zip(manifest, &[("bin/tool.exe", b"original")]),
            &root,
            &framework,
            &registry,
        )
        .expect("install Art");
        let art_root = root.join("arts/recover-art");
        let active_path = art_root.join("active.json");
        let old = read_art_activation(&active_path).unwrap();
        let orphan_relative = "versions/interrupted-orphan".to_owned();
        let orphan = art_root.join(&orphan_relative);
        std::fs::create_dir_all(&orphan).unwrap();
        std::fs::write(orphan.join("partial.bin"), b"partial").unwrap();
        let mut next_pointer = old.active.clone();
        next_pointer.path = orphan_relative.clone();
        write_art_lifecycle(
            &art_root,
            &ArtLifecycleJournal {
                old_activation: Some(old.clone()),
                next_activation: ArtActivationState {
                    active: next_pointer,
                    previous: Some(old.active.clone()),
                    local_authoring: old.local_authoring,
                    bundled_catalog: old.bundled_catalog,
                },
                target: orphan_relative,
            },
        )
        .unwrap();
        recover_art_lifecycle(&root).expect("recover interrupted Art");
        assert_eq!(read_art_activation(&active_path), Some(old));
        assert!(!orphan.exists());

        let outside = root.join("outside.txt");
        std::fs::write(&outside, b"keep").unwrap();
        let unsafe_journal = serde_json::json!({
            "oldActivation": null,
            "nextActivation": {
                "active": {
                    "path": "../../outside.txt",
                    "version": "0.0.0",
                    "digest": "deadbeef",
                    "lockfile": "outside.json"
                },
                "previous": null
            },
            "target": "../../outside.txt"
        });
        std::fs::write(
            art_root.join(ART_LIFECYCLE_FILE),
            serde_json::to_vec(&unsafe_journal).unwrap(),
        )
        .unwrap();
        recover_art_lifecycle(&root).expect("quarantine unsafe journal");
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
        assert!(art_root.join("lifecycle.corrupt").is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn art_rollback_rejects_unsafe_previous_pointer() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"unsafe-rollback-art","name":"Unsafe","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        install_art_from_zip(
            &build_zip(manifest, &[("bin/tool.exe", b"one")]),
            &root,
            &framework,
            &registry,
        )
        .unwrap();
        install_art_from_zip(
            &build_zip(manifest, &[("bin/tool.exe", b"two")]),
            &root,
            &framework,
            &registry,
        )
        .unwrap();
        let art_root = root.join("arts/unsafe-rollback-art");
        let active_path = art_root.join("active.json");
        let mut activation = read_art_activation(&active_path).unwrap();
        activation.previous.as_mut().unwrap().path = "../../outside".to_owned();
        write_art_activation(&active_path, &activation).unwrap();
        let error = rollback_art_package(&root, "unsafe-rollback-art", &registry, &framework)
            .expect_err("unsafe previous pointer must be rejected");
        assert!(matches!(error, ArtInstallError::InvalidPackage(_)));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn art_version_retention_keeps_active_previous_and_writable_state() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        for (version, payload) in [
            ("1.0.0", b"one".as_slice()),
            ("2.0.0", b"two".as_slice()),
            ("3.0.0", b"three".as_slice()),
            ("4.0.0", b"four".as_slice()),
            ("5.0.0", b"five".as_slice()),
        ] {
            let manifest = serde_json::json!({
                "id": "retained-art",
                "name": "Retained",
                "description": "retention",
                "enabled": true,
                "execution": { "type": "framework_art", "framework": "process" },
                "metadata": { "packageSecurity": { "version": version } }
            });
            install_art_from_zip(
                &build_zip(&manifest.to_string(), &[("bin/tool.exe", payload)]),
                &root,
                &framework,
                &registry,
            )
            .unwrap_or_else(|error| panic!("install {version}: {error}"));
        }
        let art_root = root.join("arts/retained-art");
        let activation = read_art_activation(&art_root.join("active.json")).expect("activation");
        let versions = std::fs::read_dir(art_root.join("versions"))
            .expect("versions")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert!(versions.len() <= art_history_limit());
        assert!(art_root.join(&activation.active.path).is_dir());
        assert!(art_root
            .join(activation.previous.expect("previous version").path)
            .is_dir());
        assert!(
            std::fs::metadata(art_root.join(&activation.active.path).join("bin/tool.exe"))
                .expect("code metadata")
                .permissions()
                .readonly()
        );
        for writable in ["state", "cache", "outputs"] {
            assert!(art_root.join(writable).is_dir());
            assert!(
                !std::fs::metadata(art_root.join(writable))
                    .expect("state metadata")
                    .permissions()
                    .readonly(),
                "{writable} must remain writable"
            );
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn art_uninstall_tombstone_recovery_restores_or_finishes_from_registry_state() {
        let root = temp_root();
        let framework = FrameworkRegistry::new(&root);
        install_test_framework(&framework, "process");
        let registry = ToolRegistry::new(root.join("tools"));
        let manifest = r#"{"id":"recover-uninstall-art","name":"Recover","description":"d","enabled":true,
            "execution":{"type":"framework_art","framework":"process"}}"#;
        install_art_from_zip(
            &build_zip(manifest, &[("bin/tool.exe", b"payload")]),
            &root,
            &framework,
            &registry,
        )
        .expect("install Art");
        let live = root.join("arts/recover-uninstall-art");
        let interrupted = uninstall_tombstone_path(&live, ART_UNINSTALL_TOMBSTONE_PREFIX).unwrap();
        std::fs::rename(&live, &interrupted).expect("simulate pre-registry crash");
        recover_art_uninstall_tombstones(&root).expect("restore tombstone");
        assert!(live.is_dir());
        assert!(!interrupted.exists());

        let committed = uninstall_tombstone_path(&live, ART_UNINSTALL_TOMBSTONE_PREFIX).unwrap();
        std::fs::rename(&live, &committed).expect("simulate committed uninstall");
        registry
            .delete_tool("recover-uninstall-art")
            .expect("commit registry removal");
        recover_art_uninstall_tombstones(&root).expect("finish tombstone deletion");
        assert!(!live.exists());
        assert!(!committed.exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
