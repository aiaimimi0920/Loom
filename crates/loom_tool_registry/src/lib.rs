//! User-managed tool and Art registry contracts for Loom.

pub mod art_settings;
pub mod credentials;
pub mod dependency;
pub mod framework;
pub mod framework_process;
pub mod install;

/// Outbound network policy, shared with `loom_mcp` through the `loom_security` leaf crate.
///
/// The module used to live here, but `loom_tool_registry` depends on `loom_mcp`, so the
/// dependency could not be reversed to let both sides enforce one policy. Existing call sites
/// keep using `loom_tool_registry::network_policy`.
pub use loom_security::network as network_policy;

/// Hardened archive extraction, shared through the same leaf crate as [`network_policy`].
pub(crate) use loom_security::archive as secure_zip;

mod private_store;
#[cfg(all(test, windows))]
mod test_support;
mod tool_registry;

pub use tool_registry::{
    execute_tool, execute_tool_with_timeout, execute_tool_with_timeout_and_cancellation,
    prepare_tool_arguments, ToolDefinition, ToolExecution, ToolRegistry, ToolRegistryError,
    ToolRegistryResult, WorkflowExecutionBindings, WorkflowInputBinding, WorkflowOutputBinding,
};

pub(crate) use tool_registry::{bounded_error_text, replace_registry_file};
