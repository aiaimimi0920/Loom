//! Versioned contracts for live screenshot sessions.
//!
//! `loom.live.v1` owns control, media metadata, input, observation, and trigger
//! messages. It is independent from the declarative `loom.surface.v1` stream.

mod control;
mod domain;
mod input;
mod media;
mod observation;
mod trigger_validation;
mod validation;

pub use control::*;
pub use domain::*;
pub use input::*;
pub use media::*;
pub use observation::*;
pub use trigger_validation::validate_trigger_condition;
pub use validation::*;

pub const LIVE_PROTOCOL_VERSION: &str = "loom.live.v1";
pub const LIVE_BINARY_VERSION: u8 = 1;
pub const LIVE_MAX_DIMENSION: u32 = 16_384;
pub const LIVE_MAX_VIEWERS: usize = 32;
pub const LIVE_MAX_FRAME_PAYLOAD: usize = 64 * 1024 * 1024;
pub const LIVE_MAX_OBSERVATION_VALUE: usize = 64 * 1024;
pub const LIVE_MAX_TRIGGER_BINDINGS: usize = 64;
pub const LIVE_MAX_TRIGGER_OPERAND: usize = 4 * 1024;

#[cfg(test)]
mod tests;
