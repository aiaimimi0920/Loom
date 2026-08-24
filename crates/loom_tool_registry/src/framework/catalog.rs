//! Framework package catalog discovery and bounded package acquisition.
use super::*;

/// Resolve the runtime download URL for a framework. Uses the art store base
/// (`LOOM_ART_STORE_URL`, overridable per-framework by
/// `LOOM_FRAMEWORK_RUNTIME_URL`), fetching `<store>/frameworks/<id>.zip`.
pub(super) fn framework_runtime_url(id: &str) -> Option<String> {
    if let Ok(explicit) = std::env::var("LOOM_FRAMEWORK_RUNTIME_URL") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    let store = std::env::var("LOOM_ART_STORE_URL").ok()?;
    let store = store.trim().trim_end_matches('/');
    if store.is_empty() {
        return None;
    }
    Some(format!("{store}/frameworks/{id}.zip"))
}

pub(super) fn configured_framework_package_path(id: &str) -> Option<PathBuf> {
    let root = std::env::var_os(FRAMEWORK_PACKAGE_CATALOG_ENV)?;
    let root = PathBuf::from(root);
    (!root.as_os_str().is_empty()).then(|| root.join(format!("{id}.zip")))
}

pub(super) fn packaged_framework_catalog_roots(executable: &Path) -> Vec<PathBuf> {
    let Some(executable_dir) = executable.parent() else {
        return Vec::new();
    };
    let mut roots = vec![executable_dir.join("packages").join("frameworks")];
    if let Some(release_root) = executable_dir.parent() {
        roots.push(release_root.join("packages").join("frameworks"));
    }
    roots
}

pub(super) fn packaged_framework_package_path(id: &str) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    packaged_framework_catalog_roots(&executable)
        .into_iter()
        .map(|root| root.join(format!("{id}.zip")))
        .find(|path| path.is_file())
}

pub(super) fn read_framework_package_from_catalog(
    id: &str,
    package_path: &Path,
) -> Result<Vec<u8>, FrameworkError> {
    let metadata = fs::symlink_metadata(package_path).map_err(|error| {
        FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "cannot read local package `{}`: {error}",
                package_path.display()
            ),
        }
    })?;
    if metadata_has_link_semantics(&metadata) || !metadata.is_file() {
        return Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "local package is linked or not a file: {}",
                package_path.display()
            ),
        });
    }
    if metadata.len() > FRAMEWORK_PACKAGE_MAX_BYTES {
        return Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "local package exceeds {FRAMEWORK_PACKAGE_MAX_BYTES} bytes: {}",
                package_path.display()
            ),
        });
    }
    let bytes = read_bounded_file(package_path, FRAMEWORK_PACKAGE_MAX_BYTES).map_err(|error| {
        FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "cannot read local package `{}`: {error}",
                package_path.display()
            ),
        }
    })?;
    let checksum_path = package_path.with_extension("zip.sha256");
    let checksum = String::from_utf8(
        read_bounded_file(&checksum_path, FRAMEWORK_METADATA_MAX_BYTES).map_err(|error| {
            FrameworkError::RuntimeDownloadFailed {
                id: id.to_owned(),
                reason: format!(
                    "cannot read local package checksum `{}`: {error}",
                    checksum_path.display()
                ),
            }
        })?,
    )
    .map_err(|error| FrameworkError::RuntimeDownloadFailed {
        id: id.to_owned(),
        reason: format!("local package checksum is not UTF-8: {error}"),
    })?;
    let mut fields = checksum.split_whitespace();
    let expected_hash = fields
        .next()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let expected_name = fields.next();
    let package_name = package_path.file_name().and_then(OsStr::to_str);
    if expected_hash.is_none() || expected_name != package_name || fields.next().is_some() {
        return Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "invalid local package checksum: {}",
                checksum_path.display()
            ),
        });
    }
    let actual_hash = format!("{:x}", Sha256::digest(&bytes));
    if !actual_hash.eq_ignore_ascii_case(expected_hash.expect("validated checksum hash")) {
        return Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "local package checksum mismatch: {}",
                package_path.display()
            ),
        });
    }
    Ok(bytes)
}

/// Load a framework package from an explicit local catalog, a configured
/// network store, or the package catalog next to a formal Loom release.
pub(super) fn default_runtime_fetcher(id: &str) -> Result<Vec<u8>, FrameworkError> {
    if let Some(package_path) = configured_framework_package_path(id) {
        return read_framework_package_from_catalog(id, &package_path);
    }
    let Some(url) = framework_runtime_url(id) else {
        if let Some(package_path) = packaged_framework_package_path(id) {
            return read_framework_package_from_catalog(id, &package_path);
        }
        return Err(FrameworkError::RuntimeSourceMissing { id: id.to_owned() });
    };
    let policy = crate::network_policy::OutboundPolicy {
        allow_http_loopback: true,
        ..crate::network_policy::OutboundPolicy::default()
    };
    let client = crate::network_policy::secure_client(
        "Loom/0.1 Framework Runtime Fetch",
        std::time::Duration::from_secs(600),
        policy.clone(),
    )
    .map_err(|error| FrameworkError::RuntimeDownloadFailed {
        id: id.to_owned(),
        reason: error,
    })?;
    crate::network_policy::get_bounded(&client, &url, &policy, FRAMEWORK_PACKAGE_MAX_BYTES as usize)
        .map_err(|error| FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: error,
        })
}
