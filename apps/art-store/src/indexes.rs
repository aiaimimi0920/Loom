// Persistent global Art IDs and platform-owned official certification lookup.
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::filesystem::read_optional_regular_file;
use crate::model::{CatalogEntry, StoreError};
use crate::persistence::write_json_atomic;

pub const GLOBAL_ART_IDS_FILE: &str = "global-art-ids.json";
pub const OFFICIAL_ART_CERTIFICATIONS_FILE: &str = "official-art-certifications.json";
const OFFICIAL_ART_CERTIFICATION_SCHEMA_VERSION: u32 = 1;
const FIRST_GLOBAL_ART_NUMBER: u64 = 40_000_000_000;
const LAST_GLOBAL_ART_NUMBER: u64 = 99_999_999_999;
const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GlobalArtIdIndex {
    #[serde(default = "global_art_id_schema_version")]
    schema_version: u32,
    #[serde(default = "first_global_art_number")]
    next_numeric: u64,
    #[serde(default)]
    assignments: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialArtCertificationIndex {
    #[serde(default = "official_art_certification_schema_version")]
    schema_version: u32,
    #[serde(default)]
    certifications: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

impl Default for OfficialArtCertificationIndex {
    fn default() -> Self {
        Self {
            schema_version: official_art_certification_schema_version(),
            certifications: std::collections::BTreeMap::new(),
        }
    }
}

const fn global_art_id_schema_version() -> u32 {
    1
}

const fn first_global_art_number() -> u64 {
    FIRST_GLOBAL_ART_NUMBER
}

const fn official_art_certification_schema_version() -> u32 {
    OFFICIAL_ART_CERTIFICATION_SCHEMA_VERSION
}

fn load_global_art_id_index(root: &Path) -> Result<GlobalArtIdIndex, StoreError> {
    match read_optional_regular_file(root, &root.join(GLOBAL_ART_IDS_FILE), MAX_INDEX_BYTES)? {
        Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
        None => Ok(GlobalArtIdIndex::default()),
    }
}

fn write_global_art_id_index(root: &Path, index: &GlobalArtIdIndex) -> Result<(), StoreError> {
    write_json_atomic(root, GLOBAL_ART_IDS_FILE, index)
}

fn load_official_art_certifications(
    root: &Path,
) -> Result<OfficialArtCertificationIndex, StoreError> {
    let index = match read_optional_regular_file(
        root,
        &root.join(OFFICIAL_ART_CERTIFICATIONS_FILE),
        MAX_INDEX_BYTES,
    )? {
        Some(bytes) => serde_json::from_slice(&bytes)?,
        None => OfficialArtCertificationIndex::default(),
    };
    if index.schema_version != OFFICIAL_ART_CERTIFICATION_SCHEMA_VERSION {
        return Err(StoreError::UnsupportedOfficialCertificationSchema(
            index.schema_version,
        ));
    }
    Ok(index)
}

fn catalog_entry_is_official(
    entry: &CatalogEntry,
    certifications: &OfficialArtCertificationIndex,
) -> bool {
    let Some(actual_digest) = entry
        .versions
        .iter()
        .find(|version| version.version == entry.latest_version)
        .map(|version| version.sha256.as_str())
    else {
        return false;
    };
    certifications
        .certifications
        .get(&entry.qualified_id)
        .and_then(|versions| versions.get(&entry.latest_version))
        .map(|digest| digest.trim().to_ascii_lowercase())
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .is_some_and(|digest| digest == actual_digest)
}

pub(crate) fn enrich_catalog(
    root: &Path,
    entries: &mut std::collections::BTreeMap<String, CatalogEntry>,
) -> Result<(), StoreError> {
    let global_ids = load_global_art_id_index(root)?.assignments;
    let official_certifications = load_official_art_certifications(root)?;
    for entry in entries.values_mut() {
        entry.global_id = global_ids.get(&entry.qualified_id).cloned();
        entry.official = catalog_entry_is_official(entry, &official_certifications);
    }
    Ok(())
}

pub(crate) fn assign_global_art_id(root: &Path, qualified_id: &str) -> Result<String, StoreError> {
    // The caller holds the store lock across package activation and ID assignment.
    let mut index = load_global_art_id_index(root)?;
    if let Some(existing) = index.assignments.get(qualified_id) {
        return Ok(existing.clone());
    }
    let used = index
        .assignments
        .values()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut numeric = index.next_numeric.max(FIRST_GLOBAL_ART_NUMBER);
    let global_id = loop {
        if numeric > LAST_GLOBAL_ART_NUMBER {
            return Err(StoreError::GlobalIdExhausted);
        }
        let candidate = format!("NA{numeric:011}");
        numeric += 1;
        if !used.contains(&candidate) {
            break candidate;
        }
    };
    index.schema_version = global_art_id_schema_version();
    index.next_numeric = numeric;
    index
        .assignments
        .insert(qualified_id.to_owned(), global_id.clone());
    write_global_art_id_index(root, &index)?;
    Ok(global_id)
}
