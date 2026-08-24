//! Credential persistence, protection, and scope-aware resolution.
//!
//! The facade keeps the historical `credentials::*` API stable while the
//! implementation is split by responsibility.

mod error;
mod grants;
mod protection;
mod store;
mod types;
mod values;

pub use error::CredentialError;
pub use store::CredentialStore;
pub use types::{
    CredentialDetails, CredentialInput, CredentialScope, CredentialSummary, CredentialValueType,
    ResolvedCredentialValue,
};

#[cfg(test)]
mod tests;
