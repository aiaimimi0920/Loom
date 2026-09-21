use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TilePixelPoint {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TilePixelSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TileRotation {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TileRenderMode {
    RawBgra,
    H264,
    Image,
    SurfaceV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TileInputCapability {
    Pointer,
    Wheel,
    Keyboard,
    Text,
    Touch,
    Pen,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TileDisplayInfo {
    pub name: String,
    pub can_identify: bool,
}

fn deserialize_display<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<TileDisplayInfo>, D::Error> {
    TileDisplayInfo::deserialize(deserializer).map(Some)
}

/// Advertised capabilities are not permission grants or throughput promises.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TileEndpoint {
    pub protocol_version: String,
    pub endpoint_id: String,
    pub device_id: String,
    pub output_id: String,
    pub pixel_size: TilePixelSize,
    pub render_modes: Vec<TileRenderMode>,
    pub input_capabilities: Vec<TileInputCapability>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub scheduled_presentation: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_display"
    )]
    pub display: Option<TileDisplayInfo>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallTile {
    pub tile_id: String,
    pub endpoint_id: String,
    /// Final axis-aligned footprint, after clockwise rotation of the endpoint.
    pub rect: WallRect,
    pub rotation: TileRotation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WallContentSource {
    Live(String),
    Image(String),
    Surface(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallPlacement {
    pub placement_id: String,
    pub source: WallContentSource,
    pub rect: WallRect,
    /// Normalized source rectangle. Layout does not change the source session.
    pub source_crop: WallRect,
    pub z_index: i32,
    pub interactive: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WallLayout {
    pub protocol_version: String,
    pub wall_id: String,
    pub revision: u64,
    pub bounds: WallRect,
    pub tiles: Vec<WallTile>,
    pub placements: Vec<WallPlacement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WallHit {
    pub placement_id: String,
    pub source: WallContentSource,
    pub source_point: WallPoint,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WallProjection {
    pub placement_id: String,
    pub visible_wall_rect: WallRect,
    pub source_crop: WallRect,
    /// Native normalized output coordinates for wall TL, TR, BR, BL corners.
    pub output_quad: [WallPoint; 4],
}
