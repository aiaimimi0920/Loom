//! Per-Art preferences and parameter binding utilities.
//!
//! Public exports remain rooted at `art_settings::*`; implementation modules
//! separate persistence, metadata projection, binding, and validation.

mod bindings;
mod metadata;
mod model;
mod parameters;
mod store;
mod validation;

pub(crate) use bindings::control_plane_root_for_tool;
pub use bindings::resolve_tool_value_bindings;
pub use metadata::{apply_settings_metadata, art_is_locally_authored, merge_tool_arguments};
pub use model::{ArtParameterDefinition, ArtSettingsError, ArtUpdateSource, ArtUserSettings};
pub use parameters::{
    art_parameter_definitions, credential_value_type_matches_parameter, validate_parameter_value,
};
pub use store::ArtSettingsStore;

#[cfg(test)]
mod tests;
