//! Stable, language-neutral contracts for independently packaged Loom plugins.
//!
//! V1 envelopes are strict. New protocol behavior must be negotiated explicitly
//! instead of accepting alternative historical field names.

mod execution;
mod package;
mod runtime;
mod validation;

pub mod device;
pub mod hook;
pub mod schemas;
pub mod surface;

pub use device::*;
pub use execution::*;
pub use hook::*;
pub use package::*;
pub use runtime::*;
pub use surface::*;
pub use validation::*;

pub const FRAMEWORK_PROTOCOL_VERSION: &str = "loom.framework.v1";
pub const ART_EXECUTION_REQUEST_SCHEMA: &str = "loom.art.execute.v1";
pub const ART_EXECUTION_RESPONSE_SCHEMA: &str = "loom.art.result.v1";
pub const ART_RUNTIME_PROTOCOL_VERSION: &str = "loom.art.runtime.v1";
pub const FRAMEWORK_AUTHORING_SCHEMA_VERSION: u32 = 1;
pub const PLUGIN_LOCKFILE_SCHEMA_VERSION: u32 = 1;

#[cfg(test)]
mod tests;
