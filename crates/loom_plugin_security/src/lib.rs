//! Signing, trust-policy, package-digest, and private-storage boundaries for Loom plugins.

mod atomic;
mod digest;
mod error;
mod model;
mod permissions;
mod signing;
mod trust_store;
mod verify;

pub use digest::canonical_package_digest;
pub use error::PluginSecurityError;
pub use model::{SigningKeyDocument, TrustPolicy};
pub use permissions::{repair_private_tree_permissions, restrict_private_path_permissions};
pub use signing::{
    generate_signing_key, read_signing_key, sign_message, sign_package, write_signing_key,
};
pub use trust_store::TrustStore;
pub use verify::verify_package_signature;

pub(crate) const TRUST_STORE_SCHEMA_VERSION: u32 = 1;
pub(crate) const SIGNING_KEY_SCHEMA_VERSION: u32 = 1;
pub(crate) const PACKAGE_SIGNATURE_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_SIGNED_PACKAGE_FILES: usize = 4096;
pub(crate) const MAX_SIGNED_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_TRUST_STORE_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_SIGNING_KEY_BYTES: u64 = 64 * 1024;
pub(crate) const MAX_SIGNATURE_DOCUMENT_BYTES: u64 = 1024 * 1024;

#[cfg(test)]
mod tests;
