//! Independent package registry for installable Capability Plugins.

mod catalog;
mod config_store;
mod faults;
mod grant_store;
mod install;
mod lifecycle;
mod registry;
mod runtime_package;
mod types;

pub use catalog::{
    capability_host_compatibility_error, parse_and_verify_capability_catalog,
    CapabilityCatalogArtifact, CapabilityCatalogClient, CapabilityCatalogDocument,
    CapabilityCatalogEntry, CapabilityCatalogError, CapabilityCatalogPackage,
    CapabilityCatalogPackageSignature, CapabilityCatalogPayload, CapabilityCatalogSignature,
    CapabilityHostSupport, DownloadedCapabilityPackage, CAPABILITY_CATALOG_SCHEMA_VERSION,
    MAX_CAPABILITY_CATALOG_BYTES, MAX_CAPABILITY_PACKAGE_DOWNLOAD_BYTES,
    OFFICIAL_CAPABILITY_CATALOG_PUBLISHER_ID,
};
pub use faults::{CAPABILITY_FAILURE_WINDOW_MILLIS, CAPABILITY_MAX_RUNTIME_FAILURES};
pub use install::{
    install_capability_from_catalog_zip, install_capability_from_zip,
    CapabilityCatalogInstallExpectation,
};
pub use registry::CapabilityPluginRegistry;
pub use runtime_package::VerifiedCapabilityPackage;
pub use types::{
    CapabilityInstallError, CapabilityInstallReport, CapabilityInstalledVersion,
    CapabilityLifecycleStatus, CapabilityPluginRecord, CapabilityRuntimeFailureState,
};

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod tests;
pub use config_store::{CapabilityConfigDocument, CapabilityConfigStore};
pub use grant_store::{CapabilityGrantStore, CapabilityPermissionGrant};
