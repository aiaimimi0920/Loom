//! Stable contracts for distributed Art surfaces.
//!
//! Surface v1 deliberately describes UI state, events, resources, and formal
//! results without exposing Hook's internal frontend framework. Art packages
//! may author TypeScript, JavaScript, or declarative scenes, but hosts exchange
//! only these language-neutral envelopes.

mod actions;
mod manifest;
mod resources;
mod scene;
mod validation;

pub use actions::*;
pub use manifest::*;
pub use resources::*;
pub use scene::*;
pub use validation::*;

pub const SURFACE_PROTOCOL_VERSION: &str = "loom.surface.v1";
/// Version tag on the daemon's surface stream poll envelope. Hook carries its own copy of this
/// literal, so changing the value here is a two-repo change and has to be announced before it lands.
pub const SURFACE_STREAM_PROTOCOL_VERSION: &str = "loom.surface-stream.v1";
pub const SURFACE_API_VERSION: &str = "1.0";
pub const SURFACE_EVENT_SNAPSHOT: &str = "loom.surface.snapshot";
pub const SURFACE_EVENT_PATCH: &str = "loom.surface.patch";
pub const SURFACE_EVENT_GENERATION: &str = "loom.surface.generation";
pub const SURFACE_EVENT_ACTION_ACK: &str = "loom.surface.action.ack";
pub const SURFACE_EVENT_CONFIRMATION_REQUEST: &str = "loom.surface.confirmation.request";
pub const SURFACE_EVENT_ACTION_PROGRESS: &str = "loom.surface.action.progress";
pub const SURFACE_EVENT_PREVIEW: &str = "loom.surface.preview";
pub const SURFACE_EVENT_RESULT: &str = "loom.surface.result";
pub const SURFACE_EVENT_FAILURE: &str = "loom.surface.failure";
pub const SURFACE_EVENT_LIFECYCLE: &str = "loom.surface.lifecycle";
pub const SURFACE_EVENT_DISPOSE: &str = "loom.surface.dispose";

pub const SURFACE_EVENT_METHODS: &[&str] = &[
    SURFACE_EVENT_SNAPSHOT,
    SURFACE_EVENT_PATCH,
    SURFACE_EVENT_GENERATION,
    SURFACE_EVENT_ACTION_ACK,
    SURFACE_EVENT_CONFIRMATION_REQUEST,
    SURFACE_EVENT_ACTION_PROGRESS,
    SURFACE_EVENT_PREVIEW,
    SURFACE_EVENT_RESULT,
    SURFACE_EVENT_FAILURE,
    SURFACE_EVENT_LIFECYCLE,
    SURFACE_EVENT_DISPOSE,
];
pub const DECLARATIVE_SURFACE_NODE_TYPES: &[&str] = &[
    "view", "row", "column", "stack", "scroll", "text", "image", "icon", "button", "input",
    "textarea", "number", "slider", "switch", "select", "progress", "divider", "spacer",
];

fn default_surface_protocol_version() -> String {
    SURFACE_PROTOCOL_VERSION.to_owned()
}

fn default_surface_api_version() -> String {
    SURFACE_API_VERSION.to_owned()
}

#[cfg(test)]
mod tests;
