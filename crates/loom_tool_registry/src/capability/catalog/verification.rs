use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use loom_plugin_security::{verify_message, TrustStore};
use loom_protocol::{
    is_safe_package_id, is_safe_publisher_id, is_valid_capability_permission,
    CapabilityApiRequirement,
};
use reqwest::Url;
use semver::Version;
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use super::model::{
    CapabilityCatalogArtifact, CapabilityCatalogDocument, CapabilityCatalogEntry,
    CapabilityHostSupport, CAPABILITY_CATALOG_SCHEMA_VERSION, MAX_CAPABILITY_CATALOG_BYTES,
    MAX_CAPABILITY_PACKAGE_DOWNLOAD_BYTES, OFFICIAL_CAPABILITY_CATALOG_PUBLISHER_ID,
};

const MAX_CATALOG_PACKAGES: usize = 256;
const MAX_CATALOG_TEXT_CHARS: usize = 4_096;
const MAX_EVIDENCE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum CapabilityCatalogError {
    #[error("capability catalog is invalid: {0}")]
    Invalid(String),
    #[error("capability catalog trust failed: {0}")]
    Trust(String),
    #[error("capability catalog network failed: {0}")]
    Network(String),
    #[error("capability catalog package was not found: {0}")]
    NotFound(String),
    #[error("capability catalog package is incompatible: {0}")]
    Incompatible(String),
}

pub fn parse_and_verify_capability_catalog(
    bytes: &[u8],
    trust_store: &TrustStore,
    now: DateTime<Utc>,
) -> Result<CapabilityCatalogDocument, CapabilityCatalogError> {
    if bytes.len() > MAX_CAPABILITY_CATALOG_BYTES {
        return Err(CapabilityCatalogError::Invalid(
            "metadata exceeds its byte limit".to_owned(),
        ));
    }
    let document: CapabilityCatalogDocument = serde_json::from_slice(bytes)
        .map_err(|error| CapabilityCatalogError::Invalid(error.to_string()))?;
    validate_document(&document, now)?;
    let signer = trust_store
        .publishers
        .iter()
        .find(|record| {
            record.publisher_id == document.signed.publisher.id
                && record.key_id == document.signature.key_id
        })
        .ok_or_else(|| CapabilityCatalogError::Trust("catalog signer is not trusted".to_owned()))?;
    if signer.revoked {
        return Err(CapabilityCatalogError::Trust(
            "catalog signer is revoked".to_owned(),
        ));
    }
    let payload = serde_json::to_vec(&document.signed)
        .map_err(|error| CapabilityCatalogError::Invalid(error.to_string()))?;
    verify_message(&signer.public_key, &payload, &document.signature.value)
        .map_err(|error| CapabilityCatalogError::Trust(error.to_string()))?;
    Ok(document)
}

fn validate_document(
    document: &CapabilityCatalogDocument,
    now: DateTime<Utc>,
) -> Result<(), CapabilityCatalogError> {
    let payload = &document.signed;
    if payload.schema_version != CAPABILITY_CATALOG_SCHEMA_VERSION
        || document.signature.algorithm != "ed25519"
        || document.signature.key_id != payload.publisher.key_id
        || payload.publisher.id != OFFICIAL_CAPABILITY_CATALOG_PUBLISHER_ID
        || !is_safe_publisher_id(&payload.publisher.id)
        || payload.packages.len() > MAX_CATALOG_PACKAGES
    {
        return Err(CapabilityCatalogError::Invalid(
            "unsupported catalog envelope".to_owned(),
        ));
    }
    let generated = parse_time(&payload.generated_at)?;
    let expires = parse_time(&payload.expires_at)?;
    if generated > now + Duration::minutes(5) || expires <= now || expires <= generated {
        return Err(CapabilityCatalogError::Invalid(
            "catalog validity window is stale".to_owned(),
        ));
    }
    let mut ids = HashSet::new();
    for entry in &payload.packages {
        validate_entry(entry)?;
        if !ids.insert(entry.qualified_id.to_ascii_lowercase()) {
            return Err(CapabilityCatalogError::Invalid(format!(
                "duplicate package {}",
                entry.qualified_id
            )));
        }
    }
    Ok(())
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, CapabilityCatalogError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| CapabilityCatalogError::Invalid("catalog timestamp is invalid".to_owned()))
}

fn validate_entry(entry: &CapabilityCatalogEntry) -> Result<(), CapabilityCatalogError> {
    let Some((publisher, package)) = entry.qualified_id.split_once('/') else {
        return Err(CapabilityCatalogError::Invalid(
            "package identity is unqualified".to_owned(),
        ));
    };
    if !is_safe_publisher_id(publisher)
        || !is_safe_package_id(package)
        || publisher != entry.publisher.id
        || entry.publisher.key_id != entry.package.signature.key_id
        || entry.package.signature.algorithm != "ed25519"
        || Version::parse(&entry.version).is_err()
        || entry.name.is_empty()
        || entry.name.len() > MAX_CATALOG_TEXT_CHARS
        || entry.description.len() > MAX_CATALOG_TEXT_CHARS
        || entry.package.bytes == 0
        || entry.package.bytes > MAX_CAPABILITY_PACKAGE_DOWNLOAD_BYTES as u64
        || entry.sbom.bytes == 0
        || entry.sbom.bytes > MAX_EVIDENCE_BYTES
        || entry.provenance.bytes == 0
        || entry.provenance.bytes > MAX_EVIDENCE_BYTES
        || entry.permissions.len() > 64
        || entry
            .permissions
            .iter()
            .any(|permission| !is_valid_capability_permission(permission))
    {
        return Err(CapabilityCatalogError::Invalid(format!(
            "package {} has invalid metadata",
            entry.qualified_id
        )));
    }
    for digest in [
        &entry.package.sha256,
        &entry.sbom.sha256,
        &entry.provenance.sha256,
    ] {
        validate_sha256(digest)?;
    }
    for url in [&entry.package.url, &entry.sbom.url, &entry.provenance.url] {
        validate_artifact_url(url)?;
    }
    validate_requirement(&entry.host_compatibility.loom_capability_api)?;
    validate_requirement(&entry.host_compatibility.hook_extension_api)?;
    if let Some(requirement) = &entry.host_compatibility.surface_api {
        validate_requirement(requirement)?;
    }
    Ok(())
}

fn validate_artifact_url(value: &str) -> Result<(), CapabilityCatalogError> {
    let url = Url::parse(value)
        .map_err(|_| CapabilityCatalogError::Invalid("artifact URL is invalid".to_owned()))?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(CapabilityCatalogError::Invalid(
            "artifact URL is unsafe".to_owned(),
        ));
    }
    Ok(())
}

fn validate_requirement(
    requirement: &CapabilityApiRequirement,
) -> Result<(), CapabilityCatalogError> {
    if !api_version_matches(
        &requirement.minimum,
        &requirement.minimum,
        requirement.maximum.as_deref(),
    ) || requirement.required_features.len() > 128
        || requirement.optional_features.len() > 128
        || requirement
            .required_features
            .iter()
            .chain(&requirement.optional_features)
            .any(|feature| feature.is_empty() || feature.len() > 128)
    {
        return Err(CapabilityCatalogError::Invalid(
            "host compatibility requirement is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), CapabilityCatalogError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(CapabilityCatalogError::Invalid(
            "artifact SHA-256 is invalid".to_owned(),
        ))
    }
}

pub(super) fn verify_artifact(
    bytes: &[u8],
    artifact: &CapabilityCatalogArtifact,
) -> Result<(), CapabilityCatalogError> {
    if bytes.len() as u64 != artifact.bytes {
        return Err(CapabilityCatalogError::Invalid(
            "artifact byte length changed".to_owned(),
        ));
    }
    verify_sha256(bytes, &artifact.sha256)
}

pub(super) fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), CapabilityCatalogError> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(CapabilityCatalogError::Invalid(
            "artifact digest changed".to_owned(),
        ))
    }
}

pub fn capability_host_compatibility_error(
    entry: &CapabilityCatalogEntry,
    host: &CapabilityHostSupport,
) -> Option<String> {
    let compatibility = &entry.host_compatibility;
    requirement_error(
        "Loom",
        &compatibility.loom_capability_api,
        &host.loom_api_version,
        &host.loom_features,
    )
    .or_else(|| {
        if !host.hook_connected {
            Some("需要连接支持能力扩展的 Hook".to_owned())
        } else {
            requirement_error(
                "Hook",
                &compatibility.hook_extension_api,
                &host.hook_api_version,
                &host.hook_features,
            )
        }
    })
    .or_else(|| {
        compatibility.surface_api.as_ref().and_then(|requirement| {
            requirement_error(
                "Surface",
                requirement,
                &host.surface_api_version,
                &host.surface_features,
            )
        })
    })
}

fn requirement_error(
    host_name: &str,
    requirement: &CapabilityApiRequirement,
    actual_version: &str,
    actual_features: &[String],
) -> Option<String> {
    if !api_version_matches(
        actual_version,
        &requirement.minimum,
        requirement.maximum.as_deref(),
    ) {
        return Some(format!("{host_name} Host API 版本不兼容"));
    }
    requirement
        .required_features
        .iter()
        .find(|feature| !actual_features.contains(feature))
        .map(|feature| format!("{host_name} 缺少功能 {feature}"))
}

fn api_version_matches(actual: &str, minimum: &str, maximum: Option<&str>) -> bool {
    fn parts(value: &str) -> Option<(u32, u32)> {
        let (major, minor) = value.split_once('.')?;
        Some((major.parse().ok()?, minor.parse().ok()?))
    }
    let (Some(actual), Some(minimum)) = (parts(actual), parts(minimum)) else {
        return false;
    };
    actual >= minimum
        && maximum
            .and_then(parts)
            .is_none_or(|maximum| actual <= maximum)
}
