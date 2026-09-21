use std::collections::HashSet;

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid wall contract: {0}")]
pub struct WallValidationError(pub &'static str);

pub fn validate_tile_endpoint(endpoint: &TileEndpoint) -> Result<(), WallValidationError> {
    require(
        endpoint.protocol_version == WALL_PROTOCOL_VERSION,
        "protocolVersion",
    )?;
    for id in [
        &endpoint.endpoint_id,
        &endpoint.device_id,
        &endpoint.output_id,
    ] {
        require(valid_id(id), "endpoint identity")?;
    }
    if let Some(display) = &endpoint.display {
        require(
            !display.name.trim().is_empty()
                && display.name.chars().count() <= 256
                && !display.name.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'),
            "display name",
        )?;
    }
    let size = endpoint.pixel_size;
    require(
        (1..=WALL_MAX_PIXEL_DIMENSION).contains(&size.width)
            && (1..=WALL_MAX_PIXEL_DIMENSION).contains(&size.height),
        "pixelSize",
    )?;
    require(
        !endpoint.render_modes.is_empty()
            && endpoint.render_modes.len() <= 4
            && unique(&endpoint.render_modes),
        "renderModes",
    )?;
    require(
        endpoint.input_capabilities.len() <= 6 && unique(&endpoint.input_capabilities),
        "inputCapabilities",
    )
}

pub fn validate_wall_layout(layout: &WallLayout) -> Result<(), WallValidationError> {
    require(
        layout.protocol_version == WALL_PROTOCOL_VERSION,
        "protocolVersion",
    )?;
    require(valid_id(&layout.wall_id), "wallId")?;
    require(
        (1..=WALL_MAX_REVISION).contains(&layout.revision),
        "revision",
    )?;
    validate_rect(layout.bounds)?;
    require(layout.tiles.len() <= WALL_MAX_TILES, "tiles limit")?;
    require(
        layout.placements.len() <= WALL_MAX_PLACEMENTS,
        "placements limit",
    )?;
    let mut tiles = HashSet::new();
    let mut endpoints = HashSet::new();
    for (index, tile) in layout.tiles.iter().enumerate() {
        require(
            valid_id(&tile.tile_id) && valid_id(&tile.endpoint_id),
            "tile identity",
        )?;
        require(tiles.insert(&tile.tile_id), "duplicate tileId")?;
        require(endpoints.insert(&tile.endpoint_id), "duplicate endpointId")?;
        validate_rect(tile.rect)?;
        require(contains_rect(layout.bounds, tile.rect), "tile outside wall")?;
        require(
            !layout.tiles[..index]
                .iter()
                .any(|prior| intersects(prior.rect, tile.rect)),
            "overlapping tiles",
        )?;
    }
    let mut placements = HashSet::new();
    for placement in &layout.placements {
        require(valid_id(&placement.placement_id), "placementId")?;
        require(
            placements.insert(&placement.placement_id),
            "duplicate placementId",
        )?;
        validate_rect(placement.rect)?;
        validate_rect(placement.source_crop)?;
        require(
            contains_rect(
                WallRect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                placement.source_crop,
            ),
            "sourceCrop",
        )?;
        let valid_source = match &placement.source {
            WallContentSource::Live(id) | WallContentSource::Surface(id) => valid_id(id),
            WallContentSource::Image(id) => id.strip_prefix("sha256:").is_some_and(|digest| {
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            }),
        };
        require(valid_source, "source identity")?;
        require(
            !placement.interactive || !matches!(placement.source, WallContentSource::Image(_)),
            "image cannot receive application input",
        )?;
    }
    Ok(())
}

pub(super) fn require(condition: bool, reason: &'static str) -> Result<(), WallValidationError> {
    if condition {
        Ok(())
    } else {
        Err(WallValidationError(reason))
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.:/".contains(&c))
}

fn unique<T: Eq + std::hash::Hash>(values: &[T]) -> bool {
    values.iter().collect::<HashSet<_>>().len() == values.len()
}

fn validate_rect(rect: WallRect) -> Result<(), WallValidationError> {
    require(
        [
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.x + rect.width,
            rect.y + rect.height,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v.abs() <= WALL_MAX_COORDINATE)
            && rect.width >= WALL_MIN_EXTENT
            && rect.height >= WALL_MIN_EXTENT
            && rect.x + rect.width > rect.x
            && rect.y + rect.height > rect.y,
        "rect",
    )
}

fn contains_rect(outer: WallRect, inner: WallRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

fn intersects(a: WallRect, b: WallRect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
