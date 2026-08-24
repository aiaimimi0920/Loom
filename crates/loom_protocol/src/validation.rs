//! Compatibility and identifier validation at the public package boundary.

use semver::{Version, VersionReq};
use thiserror::Error;

use crate::package::{FrameworkAuthoringSchema, FrameworkPackageManifest, HostCompatibility};
use crate::{
    ART_EXECUTION_REQUEST_SCHEMA, ART_EXECUTION_RESPONSE_SCHEMA,
    FRAMEWORK_AUTHORING_SCHEMA_VERSION, FRAMEWORK_PROTOCOL_VERSION,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolValidationError {
    #[error("package id is not safe: {0}")]
    UnsafePackageId(String),
    #[error("publisher id is not safe: {0}")]
    UnsafePublisherId(String),
    #[error("invalid semantic version `{value}`: {reason}")]
    InvalidVersion { value: String, reason: String },
    #[error("invalid host compatibility requirement `{value}`: {reason}")]
    InvalidCompatibility { value: String, reason: String },
    #[error("unsupported protocol; package advertises {advertised:?}")]
    UnsupportedProtocol { advertised: Vec<String> },
    #[error("unsupported Art execution schema")]
    UnsupportedArtExecutionSchema,
    #[error("authoring schema version {0} is not supported")]
    UnsupportedAuthoringSchema(u32),
    #[error("authoring field id is not safe: {0}")]
    UnsafeAuthoringField(String),
    #[error("authoring port name is not safe: {0}")]
    UnsafeAuthoringPort(String),
}

pub fn is_windows_reserved_device_name(value: &str) -> bool {
    let base = value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || base
            .strip_prefix("COM")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
        || base
            .strip_prefix("LPT")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
}

pub fn is_safe_package_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && !is_windows_reserved_device_name(value)
}

pub fn is_safe_publisher_id(value: &str) -> bool {
    is_safe_package_id(value)
}

pub fn validate_framework_manifest_contract(
    manifest: &FrameworkPackageManifest,
) -> Result<(), ProtocolValidationError> {
    if !is_safe_package_id(&manifest.id) {
        return Err(ProtocolValidationError::UnsafePackageId(
            manifest.id.clone(),
        ));
    }
    let publisher = &manifest.publisher;
    if !is_safe_publisher_id(&publisher.id) {
        return Err(ProtocolValidationError::UnsafePublisherId(
            publisher.id.clone(),
        ));
    }
    Version::parse(manifest.version.trim()).map_err(|error| {
        ProtocolValidationError::InvalidVersion {
            value: manifest.version.clone(),
            reason: error.to_string(),
        }
    })?;
    validate_host_compatibility(&manifest.host_compatibility)?;
    negotiate_framework_protocol(manifest)?;
    if manifest.art_execution.request_schema != ART_EXECUTION_REQUEST_SCHEMA
        || manifest.art_execution.response_schema != ART_EXECUTION_RESPONSE_SCHEMA
    {
        return Err(ProtocolValidationError::UnsupportedArtExecutionSchema);
    }
    if let Some(authoring) = &manifest.authoring_schema {
        validate_authoring_schema(authoring)?;
    }
    Ok(())
}

pub fn negotiate_framework_protocol(
    manifest: &FrameworkPackageManifest,
) -> Result<&'static str, ProtocolValidationError> {
    let advertised = manifest.advertised_protocol_versions();
    if advertised.contains(&FRAMEWORK_PROTOCOL_VERSION) {
        Ok(FRAMEWORK_PROTOCOL_VERSION)
    } else {
        Err(ProtocolValidationError::UnsupportedProtocol {
            advertised: advertised.into_iter().map(ToOwned::to_owned).collect(),
        })
    }
}

pub fn validate_host_compatibility(
    compatibility: &HostCompatibility,
) -> Result<(), ProtocolValidationError> {
    for requirement in [&compatibility.minimum, &compatibility.maximum]
        .into_iter()
        .flatten()
    {
        VersionReq::parse(requirement).map_err(|error| {
            ProtocolValidationError::InvalidCompatibility {
                value: requirement.clone(),
                reason: error.to_string(),
            }
        })?;
    }
    Ok(())
}

pub fn validate_authoring_schema(
    schema: &FrameworkAuthoringSchema,
) -> Result<(), ProtocolValidationError> {
    if schema.schema_version != FRAMEWORK_AUTHORING_SCHEMA_VERSION {
        return Err(ProtocolValidationError::UnsupportedAuthoringSchema(
            schema.schema_version,
        ));
    }
    for field in &schema.fields {
        if !is_safe_package_id(&field.id) {
            return Err(ProtocolValidationError::UnsafeAuthoringField(
                field.id.clone(),
            ));
        }
    }
    for port in schema.inputs.iter().chain(&schema.outputs) {
        if !is_safe_package_id(&port.name) {
            return Err(ProtocolValidationError::UnsafeAuthoringPort(
                port.name.clone(),
            ));
        }
    }
    Ok(())
}

pub fn response_status_is_success(status: &str) -> bool {
    status == "success"
}
