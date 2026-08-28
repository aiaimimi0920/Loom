//! Independent package registry for installable Capability Plugins.

mod config_store;
mod faults;
mod grant_store;
mod install;
mod lifecycle;
mod registry;
mod runtime_package;
mod types;

pub use faults::{CAPABILITY_FAILURE_WINDOW_MILLIS, CAPABILITY_MAX_RUNTIME_FAILURES};
pub use install::install_capability_from_zip;
pub use registry::CapabilityPluginRegistry;
pub use runtime_package::VerifiedCapabilityPackage;
pub use types::{
    CapabilityInstallError, CapabilityInstallReport, CapabilityInstalledVersion,
    CapabilityLifecycleStatus, CapabilityPluginRecord, CapabilityRuntimeFailureState,
};

#[cfg(test)]
mod tests;
pub use config_store::{CapabilityConfigDocument, CapabilityConfigStore};
pub use grant_store::{CapabilityGrantStore, CapabilityPermissionGrant};
