//! Hold the layout lock through input admission so a mapping cannot change mid-event.
use super::catalog::{ensure_owner, find_endpoint};
use super::*;
use loom_protocol::wall::{
    TileInputCapability, TilePixelPoint, WallContentSource, WallGeometry, WallPoint,
};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WallInputBinding {
    pub endpoint_id: String,
    pub lease_id: String,
    pub revision: u64,
}

pub(crate) struct WallInputTarget {
    pub session_id: String,
    pub placement_id: String,
    pub point: Option<WallPoint>,
}

impl WallStore {
    pub(crate) fn with_input_target<T>(
        &self,
        actor: &str,
        binding: &WallInputBinding,
        placement_id: Option<&str>,
        pixel: Option<TilePixelPoint>,
        capability: TileInputCapability,
        action: impl FnOnce(WallInputTarget) -> WallResult<T>,
    ) -> WallResult<T> {
        let mut state = self.lock()?;
        let lease = super::leases::lease_mut(&mut state, &binding.endpoint_id, &binding.lease_id)?;
        super::identification::require_input(lease)?;
        if lease.applied_revision != Some(binding.revision) {
            return Err(WallStoreError::conflict("input requires an applied layout"));
        }
        let endpoint = find_endpoint(&state.document, &binding.endpoint_id)?;
        ensure_owner(endpoint, Some(actor))?;
        if !endpoint.input_capabilities.contains(&capability) {
            return Err(WallStoreError::new(
                409,
                "wall_input_unavailable",
                "endpoint input capability unavailable",
            ));
        }
        let (layout, tile) = state
            .document
            .layouts
            .iter()
            .find_map(|layout| {
                layout
                    .tiles
                    .iter()
                    .find(|tile| tile.endpoint_id == binding.endpoint_id)
                    .map(|tile| (layout, tile))
            })
            .ok_or_else(|| WallStoreError::conflict("endpoint has no assigned wall"))?;
        if layout.revision != binding.revision {
            return Err(WallStoreError::conflict("input uses a stale layout"));
        }
        super::presentation::require_running(&state.document, &layout.wall_id)?;
        let geometry = WallGeometry::new(layout, endpoint, &tile.tile_id)
            .map_err(|_| WallStoreError::invalid("invalid input geometry"))?;
        let hit = pixel
            .map(|pixel| geometry.hit_test(binding.revision, pixel))
            .transpose()
            .map_err(|_| WallStoreError::invalid("pixel outside endpoint"))?
            .flatten();
        let target_id = placement_id
            .or_else(|| hit.as_ref().map(|hit| hit.placement_id.as_str()))
            .ok_or_else(|| {
                WallStoreError::new(
                    409,
                    "wall_input_no_target",
                    "no interactive content at this pixel",
                )
            })?;
        // Leaving the captured content cancels instead of guessing a cross-tile pointer handoff.
        if pixel.is_some() && hit.as_ref().is_none_or(|hit| hit.placement_id != target_id) {
            return Err(WallStoreError::new(
                409,
                "wall_input_target_changed",
                "pointer left its authorized content",
            ));
        }
        let placement = layout
            .placements
            .iter()
            .find(|p| p.placement_id == target_id && p.interactive)
            .ok_or_else(|| WallStoreError::conflict("interactive placement was removed"))?;
        if !geometry
            .projections()
            .iter()
            .any(|p| p.placement_id == target_id)
        {
            return Err(WallStoreError::conflict(
                "placement is not visible on this tile",
            ));
        }
        let WallContentSource::Live(session_id) = &placement.source else {
            return Err(WallStoreError::new(
                409,
                "wall_input_source_unavailable",
                "this source has no wall input adapter",
            ));
        };
        action(WallInputTarget {
            session_id: session_id.clone(),
            placement_id: target_id.to_owned(),
            point: hit.map(|hit| hit.source_point),
        })
    }
}
