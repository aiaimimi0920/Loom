// Path-component and identifier validation shared by storage and package parsing.
use std::path::Path;

use loom_protocol::is_safe_package_id;

/// Reject ids that are unsafe as a single file stem.
pub fn is_safe_art_id(id: &str) -> bool {
    is_safe_package_id(id)
}

/// Allow nested resources while rejecting absolute paths, traversal and Windows aliases.
pub fn is_safe_resource_name(name: &str) -> bool {
    if name.is_empty() || name.contains('\\') || name.contains(':') {
        return false;
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return false;
    }
    path.components().all(|component| {
        matches!(component, std::path::Component::Normal(part) if part != std::ffi::OsStr::new(".."))
    })
}
