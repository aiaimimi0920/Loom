// Art, Capability, and MCP Server manifest decoding and multi-platform metadata projection.
use std::collections::BTreeMap;

use crate::archive::{open_bounded_archive, read_entry_bounded, MAX_MANIFEST_BYTES};
use crate::model::{
    CatalogEntry, CatalogVersion, PlatformArtifactSummary, StoreError, StorePackageKind,
};
use crate::validation::is_safe_resource_name;

pub(crate) const MANIFEST_NAME: &str = "manifest.json";
pub(crate) const CAPABILITY_MANIFEST_NAME: &str = "capability.manifest.json";
pub(crate) const MCP_MANIFEST_NAME: &str = "mcp.server.json";

const ALLOWED_PLATFORM_KEYS: &[&str] = &[
    "windows",
    "windows-x64",
    "windows-arm64",
    "linux",
    "linux-x64",
    "linux-arm64",
    "macos",
    "macos-x64",
    "macos-arm64",
    "universal",
];

fn extract_platform_matrix(value: &serde_json::Value) -> BTreeMap<String, PlatformArtifactSummary> {
    let mut matrix = BTreeMap::new();
    let candidates = [
        value.get("platforms"),
        value.get("metadata").and_then(|m| m.get("platforms")),
        value.get("entry").and_then(|e| e.get("platforms")),
    ];
    for candidate in candidates.into_iter().flatten() {
        let Some(object) = candidate.as_object() else {
            continue;
        };
        for (platform, spec) in object {
            if !ALLOWED_PLATFORM_KEYS.contains(&platform.as_str()) {
                continue;
            }
            let summary = if let Some(spec_obj) = spec.as_object() {
                let binary = spec_obj
                    .get("binary")
                    .and_then(|v| v.as_str())
                    .filter(|name| is_safe_resource_name(name))
                    .map(str::to_owned);
                let entry = spec_obj
                    .get("command")
                    .or_else(|| spec_obj.get("entry"))
                    .and_then(|v| v.as_str())
                    .filter(|name| is_safe_resource_name(name))
                    .map(str::to_owned);
                let sha256 = spec_obj
                    .get("sha256")
                    .and_then(|v| v.as_str())
                    .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
                    .map(|d| d.to_ascii_lowercase());
                PlatformArtifactSummary {
                    sha256,
                    binary,
                    entry,
                }
            } else {
                continue;
            };
            matrix.entry(platform.clone()).or_insert(summary);
        }
    }
    matrix
}

fn extract_single_binary(value: &serde_json::Value) -> Option<String> {
    value
        .get("binary")
        .or_else(|| value.get("metadata").and_then(|m| m.get("binary")))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty() && is_safe_resource_name(name))
        .map(str::to_owned)
}

pub fn catalog_entry_from_manifest(manifest_bytes: &[u8]) -> Result<CatalogEntry, StoreError> {
    let value: serde_json::Value = serde_json::from_slice(manifest_bytes)?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_owned();
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();

    let is_mcp_server = value.get("transport").is_some()
        && value.get("entry").is_some()
        && value.get("execution").is_none();
    let explicit_kind = value
        .get("kind")
        .and_then(|v| v.as_str())
        .and_then(StorePackageKind::parse_segment);
    let is_capability = explicit_kind == Some(StorePackageKind::Capability)
        || (value.get("capabilities").is_some() && value.get("execution").is_none());

    let kind = if is_mcp_server || explicit_kind == Some(StorePackageKind::McpServer) {
        StorePackageKind::McpServer
    } else if is_capability {
        StorePackageKind::Capability
    } else {
        StorePackageKind::Art
    };

    let framework = match kind {
        StorePackageKind::McpServer => "mcp".to_owned(),
        StorePackageKind::Capability => value
            .get("framework")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("capability")
            .to_owned(),
        StorePackageKind::Art => value
            .get("metadata")
            .and_then(|metadata| metadata.get("dependencies"))
            .and_then(|dependencies| dependencies.get("framework"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .filter(|framework| !framework.trim().is_empty())
            .ok_or_else(|| StoreError::MissingFramework(id.clone()))?,
    };

    let version = value
        .get("metadata")
        .and_then(|metadata| metadata.get("packageSecurity"))
        .and_then(|security| security.get("version"))
        .or_else(|| value.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    if semver::Version::parse(&version).is_err() {
        return Err(StoreError::InvalidVersion {
            id: id.clone(),
            version,
        });
    }

    let publisher = value
        .get("metadata")
        .and_then(|metadata| metadata.get("packageSecurity"))
        .and_then(|security| security.get("publisher"))
        .and_then(|p| p.get("id"))
        .or_else(|| {
            value
                .get("packageSecurity")
                .and_then(|s| s.get("publisher"))
                .and_then(|p| p.get("id"))
        })
        .or_else(|| value.get("publisher").and_then(|p| p.get("id")))
        .and_then(|p| p.as_str())
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| StoreError::MissingPublisher(id.clone()))?;

    let platforms = extract_platform_matrix(&value);
    let binary = extract_single_binary(&value);

    Ok(CatalogEntry {
        id: id.clone(),
        kind,
        qualified_id: format!("{publisher}/{id}"),
        global_id: None,
        name,
        description,
        framework,
        latest_version: version.clone(),
        versions: vec![CatalogVersion {
            version,
            sha256: String::new(),
        }],
        platforms,
        binary,
        official: false,
    })
}

pub fn read_manifest_bytes(zip_bytes: &[u8]) -> Result<Vec<u8>, StoreError> {
    let mut archive = open_bounded_archive(zip_bytes)?;
    for candidate in [MANIFEST_NAME, CAPABILITY_MANIFEST_NAME, MCP_MANIFEST_NAME] {
        if let Ok(mut file) = archive.by_name(candidate) {
            let size = file.size();
            return read_entry_bounded(&mut file, candidate, size, MAX_MANIFEST_BYTES);
        }
    }
    Err(StoreError::MissingManifest)
}
