//! Contracts shared by Capability Plugin packages, runtimes, and hosts.

mod extension;
mod package;
mod runtime;
mod validation;

pub use extension::*;
pub use package::*;
pub use runtime::*;
pub use validation::*;

pub const CAPABILITY_SCHEMA_VERSION: u32 = 1;
pub const CAPABILITY_API_VERSION: &str = "1.0";

#[cfg(test)]
mod tests;
