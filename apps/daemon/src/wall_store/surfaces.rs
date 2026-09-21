//! Connection-scoped Surface views; the wall owns grants, never the underlying Art instance.
use super::catalog::{ensure_owner, find_endpoint};
use super::*;
use loom_protocol::wall::{TileInputCapability, TileRenderMode, WallContentSource, WallGeometry};

#[derive(Clone)]
pub(crate) struct WallSurfaceLink {
    pub binding: WallInputBinding,
    pub device_id: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub width: u32,
    pub height: u32,
    pub sequence: u64,
    pub accepted_requests: WallSurfaceRequests,
    pub cancelable_actions: Vec<String>,
    pub closing: bool,
}

impl WallStore {
    pub(crate) fn authorize_surface(
        &self,
        actor: &str,
        binding: &WallInputBinding,
        instance_id: &str,
        interactive_placement: Option<&str>,
    ) -> WallResult<()> {
        self.with_surface_authority(
            actor,
            binding,
            instance_id,
            interactive_placement,
            None,
            |_| Ok(()),
        )
    }

    pub(crate) fn surface_inputs(
        &self,
        actor: &str,
        binding: &WallInputBinding,
        instance_id: &str,
    ) -> WallResult<Vec<TileInputCapability>> {
        self.with_surface_authority(actor, binding, instance_id, None, None, |endpoint| {
            Ok(endpoint.input_capabilities.clone())
        })
    }

    pub(crate) fn with_surface_authority<T>(
        &self,
        actor: &str,
        binding: &WallInputBinding,
        instance_id: &str,
        interactive_placement: Option<&str>,
        pixel: Option<loom_protocol::wall::TilePixelPoint>,
        action: impl FnOnce(&loom_protocol::wall::TileEndpoint) -> WallResult<T>,
    ) -> WallResult<T> {
        let mut state = self.lock()?;
        let lease = super::leases::lease_mut(&mut state, &binding.endpoint_id, &binding.lease_id)?;
        if interactive_placement.is_some() {
            super::identification::require_input(lease)?;
        }
        if interactive_placement.is_some() && lease.applied_revision != Some(binding.revision) {
            return Err(WallStoreError::conflict(
                "Surface input requires an applied layout",
            ));
        }
        let endpoint = find_endpoint(&state.document, &binding.endpoint_id)?;
        ensure_owner(endpoint, Some(actor))?;
        if !endpoint.render_modes.contains(&TileRenderMode::SurfaceV1) {
            return Err(WallStoreError::new(
                409,
                "wall_surface_unavailable",
                "endpoint cannot render Surface v1",
            ));
        }
        if interactive_placement.is_some()
            && !endpoint.input_capabilities.iter().any(|capability| {
                matches!(
                    capability,
                    TileInputCapability::Pointer | TileInputCapability::Keyboard
                )
            })
        {
            return Err(WallStoreError::new(
                403,
                "wall_surface_input_unavailable",
                "endpoint has no supported Surface input capability",
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
            .ok_or_else(|| WallStoreError::conflict("endpoint has no wall"))?;
        if layout.revision != binding.revision {
            return Err(WallStoreError::conflict(
                "Surface binding uses an old layout",
            ));
        }
        if interactive_placement.is_some() {
            super::presentation::require_running(&state.document, &layout.wall_id)?;
        }
        let geometry = WallGeometry::new(layout, endpoint, &tile.tile_id)
            .map_err(|_| WallStoreError::invalid("invalid Surface geometry"))?;
        let visible = geometry.projections();
        if let Some(pixel) = pixel {
            let hit = geometry
                .hit_test(binding.revision, pixel)
                .map_err(|_| WallStoreError::invalid("invalid Surface input pixel"))?;
            if !hit.is_some_and(|hit| Some(hit.placement_id.as_str()) == interactive_placement) {
                return Err(WallStoreError::new(
                    403,
                    "wall_surface_forbidden",
                    "Surface input is obscured or outside its placement",
                ));
            }
        }
        let allowed = layout.placements.iter().any(|placement| {
            matches!(&placement.source, WallContentSource::Surface(id) if id == instance_id)
                && visible
                    .iter()
                    .any(|projection| projection.placement_id == placement.placement_id)
                && interactive_placement
                    .is_none_or(|id| placement.interactive && placement.placement_id == id)
        });
        if !allowed {
            return Err(WallStoreError::new(
                403,
                "wall_surface_forbidden",
                "Surface is not visible or interactive on this endpoint",
            ));
        }
        action(endpoint)
    }

    // Route admission and maintenance use one owner lock, so a stale cleanup cannot
    // remove an attachment that has just been rebound to a new presentation revision.
    pub(crate) fn with_surface_links<T>(
        &self,
        action: impl FnOnce(&mut BTreeMap<(String, String), WallSurfaceLink>) -> WallResult<T>,
    ) -> WallResult<T> {
        let mut links = self
            .surface_links
            .lock()
            .map_err(|_| WallStoreError::unavailable())?;
        action(&mut links)
    }
}
