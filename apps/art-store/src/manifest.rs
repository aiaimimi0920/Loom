// Art manifest decoding and catalog metadata projection.
use crate::archive::{open_bounded_archive, read_entry_bounded, MAX_MANIFEST_BYTES};
use crate::model::{CatalogEntry, CatalogVersion, StoreError};

pub(crate) const MANIFEST_NAME: &str = "manifest.json";

pub fn catalog_entry_from_manifest(manifest_bytes: &[u8]) -> Result<CatalogEntry, StoreError> {
    let value: serde_json::Value = serde_json::from_slice(manifest_bytes)?;
    let id = value
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let name = value
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or(&id)
        .to_owned();
    let description = value
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let framework = value
        .get("metadata")
        .and_then(|metadata| metadata.get("dependencies"))
        .and_then(|dependencies| dependencies.get("framework"))
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .filter(|framework| !framework.trim().is_empty())
        .ok_or_else(|| StoreError::MissingFramework(id.clone()))?;
    let version = value
        .get("metadata")
        .and_then(|metadata| metadata.get("packageSecurity"))
        .and_then(|security| security.get("version"))
        .and_then(|version| version.as_str())
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
        .and_then(|publisher| publisher.get("id"))
        .and_then(|publisher| publisher.as_str())
        .map(str::trim)
        .filter(|publisher| !publisher.is_empty())
        .ok_or_else(|| StoreError::MissingPublisher(id.clone()))?;
    Ok(CatalogEntry {
        id: id.clone(),
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
        official: false,
    })
}

pub fn read_manifest_bytes(zip_bytes: &[u8]) -> Result<Vec<u8>, StoreError> {
    let mut archive = open_bounded_archive(zip_bytes)?;
    let mut file = archive
        .by_name(MANIFEST_NAME)
        .map_err(|_| StoreError::MissingManifest)?;
    let size = file.size();
    read_entry_bounded(&mut file, MANIFEST_NAME, size, MAX_MANIFEST_BYTES)
}
