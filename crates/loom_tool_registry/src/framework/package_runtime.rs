//! Framework package self-tests, secure extraction, validation, and errors.
use super::*;

pub(super) fn run_framework_self_test(
    manifest: &FrameworkPackageManifest,
    package_dir: &Path,
) -> Result<(), FrameworkError> {
    let Some(health_check) = &manifest.health_check else {
        return Ok(());
    };
    let command_path = Path::new(&manifest.entry.command);
    let executable =
        loom_process::executable_path_within(package_dir, command_path).map_err(|reason| {
            FrameworkError::RuntimeUnavailable {
                id: manifest.id.clone(),
                reason,
            }
        })?;
    let mut process = ProcessSpec::new(executable);
    process.args = manifest.entry.args.clone();
    process.args.push("--loom-health-check".to_owned());
    process.args.push(health_check.command.clone());
    process.args.extend(health_check.args.clone());
    process.current_dir = Some(package_dir.to_path_buf());
    process.limits.timeout = std::time::Duration::from_secs(health_check.timeout_seconds.max(1));
    process.limits.stdout_bytes = 1024 * 1024;
    process.limits.stderr_bytes = 1024 * 1024;
    process.limits.memory_bytes = manifest
        .resources
        .memory_mib
        .and_then(|value| usize::try_from(value.saturating_mul(1024 * 1024)).ok())
        .or(process.limits.memory_bytes);
    process.limits.max_processes = manifest
        .resources
        .max_processes
        .or(process.limits.max_processes);
    let output = loom_process::run_with_input(&process, b"").map_err(|error| {
        FrameworkError::RuntimeUnavailable {
            id: manifest.id.clone(),
            reason: format!("framework self-test failed: {error}"),
        }
    })?;
    if !output.status.success() {
        return Err(FrameworkError::RuntimeUnavailable {
            id: manifest.id.clone(),
            reason: format!(
                "framework self-test exited with {:?}: {}",
                output.status.code(),
                crate::bounded_error_text(&String::from_utf8_lossy(&output.stderr))
            ),
        });
    }
    let response: FrameworkExecuteResponse =
        serde_json::from_slice(&output.stdout).map_err(|error| {
            FrameworkError::RuntimeUnavailable {
                id: manifest.id.clone(),
                reason: format!("framework self-test returned invalid JSON: {error}"),
            }
        })?;
    if !response_status_is_success(&response.status.to_ascii_lowercase()) {
        return Err(FrameworkError::RuntimeUnavailable {
            id: manifest.id.clone(),
            reason: response
                .error
                .map(|error| format!("{}: {}", error.code, error.message))
                .unwrap_or_else(|| "framework self-test returned failure".to_owned()),
        });
    }
    Ok(())
}

/// Unpack into a newly-created staging directory. Refusing pre-existing paths
/// prevents a predictable staging-name collision from deleting or overwriting
/// filesystem content owned by another process.
pub(super) fn unpack_runtime_zip(
    id: &str,
    zip_bytes: &[u8],
    runtime_dir: &Path,
) -> Result<(), FrameworkError> {
    let fail = |reason: String| FrameworkError::RuntimeUnpackFailed {
        id: id.to_owned(),
        reason,
    };
    let parent = runtime_dir
        .parent()
        .ok_or_else(|| fail("staging directory has no parent".to_owned()))?;
    fs::create_dir_all(parent)?;
    fs::create_dir(runtime_dir)
        .map_err(|error| fail(format!("cannot create fresh staging directory: {error}")))?;
    if let Err(error) = crate::secure_zip::extract_zip_securely(zip_bytes, runtime_dir) {
        let _ = remove_framework_tree(runtime_dir);
        return Err(fail(error.to_string()));
    }
    Ok(())
}

pub(crate) fn is_valid_framework(id: &str) -> bool {
    loom_protocol::is_safe_package_id(id)
}

pub(crate) fn is_valid_framework_reference(reference: &str) -> bool {
    is_valid_framework(reference)
        || reference.split_once('/').is_some_and(|(publisher, id)| {
            !publisher.contains('/')
                && loom_protocol::is_safe_publisher_id(publisher)
                && is_valid_framework(id)
        })
}

#[derive(Debug, thiserror::Error)]
pub enum FrameworkError {
    #[error("unknown framework `{0}`")]
    UnknownFramework(String),
    #[error("framework id `{0}` matches multiple publishers; use a qualified id")]
    AmbiguousFramework(String),
    #[error("framework `{0}` is not installed")]
    FrameworkNotInstalled(String),
    #[error("framework `{id}` has no previous version available for rollback")]
    NoRollback { id: String },
    #[error("invalid framework package `{id}`: {reason}")]
    InvalidPackage { id: String, reason: String },
    #[error("framework `{id}` has no available package source (ship packages/frameworks with Loom, or set LOOM_FRAMEWORK_PACKAGE_CATALOG_DIR, LOOM_ART_STORE_URL, or LOOM_FRAMEWORK_RUNTIME_URL)")]
    RuntimeSourceMissing { id: String },
    #[error("framework `{id}` runtime download failed: {reason}")]
    RuntimeDownloadFailed { id: String, reason: String },
    #[error("framework `{id}` runtime unpack failed: {reason}")]
    RuntimeUnpackFailed { id: String, reason: String },
    #[error("framework `{id}` runtime installed but still not runnable: {reason}")]
    RuntimeUnavailable { id: String, reason: String },
    #[error(
        "frameworks state file `{path}` cannot be read ({reason}); repair or remove it before installing, enabling or uninstalling a framework"
    )]
    CorruptState { path: String, reason: String },
    #[error("frameworks store io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("frameworks store serialize error: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("framework package security error: {0}")]
    Security(#[from] PluginSecurityError),
}
