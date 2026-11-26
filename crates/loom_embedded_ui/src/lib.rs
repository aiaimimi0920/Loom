//! Immutable embedded Web UI asset lookup and SPA route fallback for `loom-daemon`.

include!(concat!(env!("OUT_DIR"), "/loom_embedded_ui_assets.rs"));

/// One embedded static Web UI asset served under `/ui/*`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddedAsset {
    pub path: &'static str,
    pub content_type: &'static str,
    pub immutable: bool,
    pub data: &'static [u8],
}

/// Whether a compiled React frontend bundle was embedded at build time.
#[must_use]
pub const fn is_prebuilt_bundle() -> bool {
    PREBUILT_BUNDLE
}

/// Total number of embedded files in the Web UI bundle.
#[must_use]
pub fn asset_count() -> usize {
    ASSETS.len()
}

fn normalize_subpath(raw: &str) -> Option<&str> {
    let trimmed = raw.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some("index.html");
    }
    if trimmed.contains('\\')
        || trimmed.contains("..")
        || trimmed.contains('?')
        || trimmed.contains('#')
        || trimmed.bytes().any(|b| b.is_ascii_control())
    {
        return None;
    }
    if trimmed
        .split('/')
        .any(|segment| segment.is_empty() || segment.starts_with('.'))
    {
        return None;
    }
    Some(trimmed)
}

/// Look up an exact asset or fall back to `index.html` for extensionless SPA routes.
#[must_use]
pub fn resolve_ui_asset(subpath: &str) -> Option<EmbeddedAsset> {
    let normalized = normalize_subpath(subpath)?;
    if let Some(&(path, content_type, immutable, data)) = ASSETS
        .iter()
        .find(|&&(candidate, _, _, _)| candidate == normalized)
    {
        return Some(EmbeddedAsset {
            path,
            content_type,
            immutable,
            data,
        });
    }
    let last_segment = normalized.rsplit('/').next().unwrap_or(normalized);
    if !last_segment.contains('.') {
        return ASSETS
            .iter()
            .find(|&&(candidate, _, _, _)| candidate == "index.html")
            .map(|&(path, content_type, immutable, data)| EmbeddedAsset {
                path,
                content_type,
                immutable,
                data,
            });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_index_and_spa_routes_while_rejecting_traversal_and_missing_files() {
        assert!(asset_count() >= 1);
        let root = resolve_ui_asset("").expect("root index");
        assert_eq!(root.path, "index.html");
        assert!(root.content_type.starts_with("text/html"));

        let spa = resolve_ui_asset("devices/overview").expect("spa fallback");
        assert_eq!(spa.path, "index.html");

        assert!(resolve_ui_asset("../secret.txt").is_none());
        assert!(resolve_ui_asset(".env").is_none());
        assert!(resolve_ui_asset("static/js/does-not-exist-9999.js").is_none());
    }
}
