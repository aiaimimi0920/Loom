use loom_protocol::{CapabilityHostCompatibility, CapabilityPublisher};
use serde::{Deserialize, Serialize};

pub const CAPABILITY_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const OFFICIAL_CAPABILITY_CATALOG_PUBLISHER_ID: &str = "neuro.official";
pub const MAX_CAPABILITY_CATALOG_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CAPABILITY_PACKAGE_DOWNLOAD_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogDocument {
    pub signed: CapabilityCatalogPayload,
    pub signature: CapabilityCatalogSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogPayload {
    pub schema_version: u32,
    pub publisher: CapabilityPublisher,
    pub generated_at: String,
    pub expires_at: String,
    pub packages: Vec<CapabilityCatalogEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogEntry {
    pub qualified_id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub publisher: CapabilityPublisher,
    pub package: CapabilityCatalogPackage,
    pub sbom: CapabilityCatalogArtifact,
    pub provenance: CapabilityCatalogArtifact,
    pub host_compatibility: CapabilityHostCompatibility,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub disk_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogPackage {
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    pub signature: CapabilityCatalogPackageSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogPackageSignature {
    pub algorithm: String,
    pub key_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogArtifact {
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityHostSupport {
    pub loom_api_version: String,
    pub loom_features: Vec<String>,
    pub hook_connected: bool,
    pub hook_api_version: String,
    pub hook_features: Vec<String>,
    pub surface_api_version: String,
    pub surface_features: Vec<String>,
}
