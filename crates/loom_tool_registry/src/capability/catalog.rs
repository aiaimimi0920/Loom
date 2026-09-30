//! Signed official catalog metadata and bounded package downloads.

mod client;
mod model;
mod verification;

pub use client::{CapabilityCatalogClient, DownloadedCapabilityPackage};
pub use model::{
    CapabilityCatalogArtifact, CapabilityCatalogDocument, CapabilityCatalogEntry,
    CapabilityCatalogPackage, CapabilityCatalogPackageSignature, CapabilityCatalogPayload,
    CapabilityCatalogSignature, CapabilityHostSupport, CAPABILITY_CATALOG_SCHEMA_VERSION,
    MAX_CAPABILITY_CATALOG_BYTES, MAX_CAPABILITY_PACKAGE_DOWNLOAD_BYTES,
    OFFICIAL_CAPABILITY_CATALOG_PUBLISHER_ID,
};
pub use verification::{
    capability_host_compatibility_error, parse_and_verify_capability_catalog,
    CapabilityCatalogError,
};
