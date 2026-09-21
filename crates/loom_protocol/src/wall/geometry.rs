use super::validation::require;
use super::*;

/// Validates once per immutable layout/endpoint snapshot, not on every frame.
pub struct WallGeometry<'a> {
    layout: &'a WallLayout,
    tile: &'a WallTile,
    endpoint: &'a TileEndpoint,
}

impl<'a> WallGeometry<'a> {
    pub fn new(
        layout: &'a WallLayout,
        endpoint: &'a TileEndpoint,
        tile_id: &str,
    ) -> Result<Self, WallValidationError> {
        validate_wall_layout(layout)?;
        validate_tile_endpoint(endpoint)?;
        let tile = layout
            .tiles
            .iter()
            .find(|tile| tile.tile_id == tile_id)
            .ok_or(WallValidationError("tile not found"))?;
        require(
            tile.endpoint_id == endpoint.endpoint_id,
            "endpoint mismatch",
        )?;
        Ok(Self {
            layout,
            tile,
            endpoint,
        })
    }

    pub fn pixel_to_wall(&self, pixel: TilePixelPoint) -> Result<WallPoint, WallValidationError> {
        let size = self.endpoint.pixel_size;
        require(
            pixel.x < size.width && pixel.y < size.height,
            "pixel outside endpoint",
        )?;
        // A pixel identifies its center. This keeps rotated edge pixels inside
        // their tile and preserves half-open seam ownership without epsilon hacks.
        let u = (f64::from(pixel.x) + 0.5) / f64::from(size.width);
        let v = (f64::from(pixel.y) + 0.5) / f64::from(size.height);
        let (u, v) = rotate(u, v, self.tile.rotation);
        Ok(WallPoint {
            x: self.tile.rect.x + u * self.tile.rect.width,
            y: self.tile.rect.y + v * self.tile.rect.height,
        })
    }

    pub fn wall_to_pixel(&self, point: WallPoint) -> Option<TilePixelPoint> {
        if !contains(self.tile.rect, point) {
            return None;
        }
        let native = self.wall_to_native(point);
        let size = self.endpoint.pixel_size;
        Some(TilePixelPoint {
            x: (native.x * f64::from(size.width)).floor().max(0.0) as u32,
            y: (native.y * f64::from(size.height)).floor().max(0.0) as u32,
        })
        .map(|pixel| TilePixelPoint {
            x: pixel.x.min(size.width - 1),
            y: pixel.y.min(size.height - 1),
        })
    }

    pub fn hit_test(
        &self,
        revision: u64,
        pixel: TilePixelPoint,
    ) -> Result<Option<WallHit>, WallValidationError> {
        require(revision == self.layout.revision, "stale layout revision")?;
        let point = self.pixel_to_wall(pixel)?;
        let placement = self
            .layout
            .placements
            .iter()
            .filter(|placement| contains(placement.rect, point))
            .max_by(|a, b| paint_order(a, b));
        // A visible noninteractive foreground blocks input to covered content.
        Ok(placement
            .filter(|placement| placement.interactive)
            .map(|placement| WallHit {
                placement_id: placement.placement_id.clone(),
                source: placement.source.clone(),
                source_point: source_point(placement, point),
            }))
    }

    pub fn projections(&self) -> Vec<WallProjection> {
        let mut placements: Vec<_> = self.layout.placements.iter().collect();
        placements.sort_by(|a, b| paint_order(a, b));
        placements
            .into_iter()
            .filter_map(|placement| {
                let visible = intersection(self.tile.rect, placement.rect)?;
                let top_left = WallPoint {
                    x: visible.x,
                    y: visible.y,
                };
                let bottom_right = WallPoint {
                    x: visible.x + visible.width,
                    y: visible.y + visible.height,
                };
                let source_start = source_point(placement, top_left);
                let source_end = source_point(placement, bottom_right);
                Some(WallProjection {
                    placement_id: placement.placement_id.clone(),
                    visible_wall_rect: visible,
                    source_crop: WallRect {
                        x: source_start.x,
                        y: source_start.y,
                        width: source_end.x - source_start.x,
                        height: source_end.y - source_start.y,
                    },
                    output_quad: [
                        top_left,
                        WallPoint {
                            x: bottom_right.x,
                            y: top_left.y,
                        },
                        bottom_right,
                        WallPoint {
                            x: top_left.x,
                            y: bottom_right.y,
                        },
                    ]
                    .map(|point| self.wall_to_native(point)),
                })
            })
            .collect()
    }

    fn wall_to_native(&self, point: WallPoint) -> WallPoint {
        let u = (point.x - self.tile.rect.x) / self.tile.rect.width;
        let v = (point.y - self.tile.rect.y) / self.tile.rect.height;
        let inverse = match self.tile.rotation {
            TileRotation::Deg0 => TileRotation::Deg0,
            TileRotation::Deg90 => TileRotation::Deg270,
            TileRotation::Deg180 => TileRotation::Deg180,
            TileRotation::Deg270 => TileRotation::Deg90,
        };
        let (x, y) = rotate(u, v, inverse);
        WallPoint { x, y }
    }
}

fn rotate(u: f64, v: f64, rotation: TileRotation) -> (f64, f64) {
    match rotation {
        TileRotation::Deg0 => (u, v),
        TileRotation::Deg90 => (1.0 - v, u),
        TileRotation::Deg180 => (1.0 - u, 1.0 - v),
        TileRotation::Deg270 => (v, 1.0 - u),
    }
}

fn contains(rect: WallRect, point: WallPoint) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.x >= rect.x
        && point.x < rect.x + rect.width
        && point.y >= rect.y
        && point.y < rect.y + rect.height
}

fn source_point(placement: &WallPlacement, point: WallPoint) -> WallPoint {
    WallPoint {
        x: placement.source_crop.x
            + (point.x - placement.rect.x) / placement.rect.width * placement.source_crop.width,
        y: placement.source_crop.y
            + (point.y - placement.rect.y) / placement.rect.height * placement.source_crop.height,
    }
}

fn paint_order(a: &WallPlacement, b: &WallPlacement) -> std::cmp::Ordering {
    a.z_index
        .cmp(&b.z_index)
        .then_with(|| a.placement_id.cmp(&b.placement_id))
}

fn intersection(a: WallRect, b: WallRect) -> Option<WallRect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let width = (a.x + a.width).min(b.x + b.width) - x;
    let height = (a.y + a.height).min(b.y + b.height) - y;
    (width > 0.0 && height > 0.0).then_some(WallRect {
        x,
        y,
        width,
        height,
    })
}
