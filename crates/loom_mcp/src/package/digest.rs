//! Deterministic streaming digests for extracted package trees.

use super::*;

/// Hash every extracted file, keyed by its package-relative path with `/` separators.
///
/// The archive digest says what was downloaded; it says nothing about what is on disk now. These
/// per-file digests are what let a later spawn notice that one file inside an installed package was
/// replaced while the manifest, the version directory name, and the archive digest all still agree.
pub(super) fn digest_tree(root: &Path) -> Result<BTreeMap<String, String>, McpPackageError> {
    let mut files = BTreeMap::new();
    collect_tree_digests(root, root, &mut files)?;
    Ok(files)
}

pub(super) fn collect_tree_digests(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), McpPackageError> {
    let mut entries = fs::read_dir(directory)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        // `symlink_metadata` rather than `metadata`, so a link is seen as a link instead of being
        // followed to whatever it points at. The shared extractor already rejects links, so one
        // here was not produced by it.
        let metadata = fs::symlink_metadata(&path)?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(McpPackageError::UnsafePath(path.display().to_string()));
        }
        if metadata.is_dir() {
            collect_tree_digests(root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(McpPackageError::UnsafePath(path.display().to_string()));
        }
        let key = path
            .strip_prefix(root)
            .ok()
            .and_then(package_key)
            .ok_or_else(|| McpPackageError::UnsafePath(path.display().to_string()))?;
        let digest = file_digest(&path)?;
        files.insert(key, digest);
    }
    Ok(())
}

pub(super) fn verify_tree_digests(
    root: &Path,
    expected: &BTreeMap<String, String>,
) -> Result<(), McpPackageError> {
    let actual = digest_tree(root)?;
    for (key, digest) in expected {
        match actual.get(key) {
            Some(found) if found.eq_ignore_ascii_case(digest) => {}
            Some(_) => {
                return Err(McpPackageError::Integrity(format!(
                    "`{key}` in {} does not match its recorded digest",
                    root.display()
                )))
            }
            None => {
                return Err(McpPackageError::Integrity(format!(
                    "`{key}` is missing from {}",
                    root.display()
                )))
            }
        }
    }
    if let Some(extra) = actual.keys().find(|key| !expected.contains_key(*key)) {
        return Err(McpPackageError::Integrity(format!(
            "`{extra}` in {} was not part of the package",
            root.display()
        )));
    }
    Ok(())
}

/// Turn a package-relative path into its `active.json` key, rejecting anything but plain names.
pub(super) fn package_key(relative: &Path) -> Option<String> {
    let mut key = String::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return None;
        };
        let value = value.to_str()?;
        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(value);
    }
    (!key.is_empty()).then_some(key)
}

pub(super) fn file_digest(path: &Path) -> Result<String, McpPackageError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(McpPackageError::UnsafePath(path.display().to_string()));
    }
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
