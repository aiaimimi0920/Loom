//! Security primitives shared by every Loom crate that handles untrusted input.
//!
//! This crate deliberately sits at the bottom of the dependency graph. `loom_mcp` and
//! `loom_tool_registry` both need the same archive extractor, the same outbound network
//! policy, and the same depth limit for untrusted JSON, but `loom_tool_registry` already
//! depends on `loom_mcp`, so neither can host the shared code. Keeping these primitives in
//! one leaf crate is what allows both sides of that edge to enforce identical rules.
//!
//! Nothing here may depend on another `loom_*` crate.

pub mod archive;
pub mod json;
pub mod network;
