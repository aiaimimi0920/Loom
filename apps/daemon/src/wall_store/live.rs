//! A presenter may read only Live sources intersecting its assigned tile.
use super::catalog::{ensure_owner, find_endpoint};
use super::*;
use loom_protocol::wall::WallContentSource;

impl WallStore {
    pub(crate) fn authorize_live(
        &self,
        endpoint_id: &str,
        actor: &str,
        lease_id: &str,
        revision: u64,
        session_id: &str,
    ) -> WallResult<TileEndpoint> {
        let mut state = self.lock()?;
        super::leases::lease_mut(&mut state, endpoint_id, lease_id)?;
        let endpoint = find_endpoint(&state.document, endpoint_id)?;
        ensure_owner(endpoint, Some(actor))?;
        let (layout, tile) = state
            .document
            .layouts
            .iter()
            .find_map(|layout| {
                layout
                    .tiles
                    .iter()
                    .find(|tile| tile.endpoint_id == endpoint_id)
                    .map(|tile| (layout, tile))
            })
            .ok_or_else(|| WallStoreError::conflict("endpoint has no assigned wall"))?;
        if layout.revision != revision {
            return Err(WallStoreError::conflict(
                "Live request uses a stale layout revision",
            ));
        }
        if !layout.placements.iter().any(|placement| {
            let a = tile.rect;
            let b = placement.rect;
            matches!(&placement.source, WallContentSource::Live(id) if id == session_id)
                && a.x < b.x + b.width
                && b.x < a.x + a.width
                && a.y < b.y + b.height
                && b.y < a.y + a.height
        }) {
            return Err(WallStoreError::new(
                403,
                "wall_live_forbidden",
                "Live source does not intersect the assigned tile",
            ));
        }
        Ok(endpoint.clone())
    }
}
