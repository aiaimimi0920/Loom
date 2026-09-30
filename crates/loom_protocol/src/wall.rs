//! Physical display endpoints, logical walls and content-to-input geometry.
//! This control contract references Live/Surface owners; it never carries media.

mod geometry;
pub mod media;
mod model;
mod validation;

pub use geometry::*;
pub use model::*;
pub use validation::*;

pub const WALL_PROTOCOL_VERSION: &str = "loom.wall.v1";
pub const WALL_MAX_TILES: usize = 64;
pub const WALL_MAX_PLACEMENTS: usize = 256;
pub const WALL_MAX_PIXEL_DIMENSION: u32 = 16_384;
pub const WALL_MAX_COORDINATE: f64 = 1_000_000.0;
pub const WALL_MIN_EXTENT: f64 = 1.0 / 65_536.0;
// Revisions cross a JavaScript boundary without lossy integer conversion.
pub const WALL_MAX_REVISION: u64 = 9_007_199_254_740_991;

#[cfg(test)]
mod tests;
