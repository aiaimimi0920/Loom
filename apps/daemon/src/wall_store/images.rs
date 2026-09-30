//! Image reads and resource retention are authorized by the durable wall and its live presenter.
use super::catalog::{ensure_owner, find_endpoint};
use super::*;
use loom_protocol::wall::WallContentSource;

impl WallStore {
    pub(crate) fn authorize_image(
        &self,
        endpoint_id: &str,
        actor: &str,
        lease_id: &str,
        revision: u64,
        resource_id: &str,
    ) -> WallResult<()> {
        let mut state = self.lock()?;
        ensure_owner(find_endpoint(&state.document, endpoint_id)?, Some(actor))?;
        super::leases::lease_mut(&mut state, endpoint_id, lease_id)?;
        let layout = state
            .document
            .layouts
            .iter()
            .find(|layout| {
                layout
                    .tiles
                    .iter()
                    .any(|tile| tile.endpoint_id == endpoint_id)
            })
            .ok_or_else(|| WallStoreError::conflict("endpoint has no assigned wall"))?;
        if layout.revision != revision {
            return Err(WallStoreError::conflict(
                "image request uses a stale layout revision",
            ));
        }
        if !layout.placements.iter().any(|placement| {
            matches!(&placement.source, WallContentSource::Image(id) if id == resource_id)
        }) {
            return Err(WallStoreError::new(403, "wall_image_forbidden", "image is not referenced by the assigned wall"));
        }
        Ok(())
    }

    /// Hold wall configuration stable through GC. Lock order is wall, then resources;
    /// resource owners must never acquire the wall lock while holding their own lock.
    pub(crate) fn with_image_references<T>(
        &self,
        action: impl FnOnce(BTreeSet<String>) -> T,
    ) -> WallResult<T> {
        let state = self.lock()?;
        let ids = state
            .document
            .layouts
            .iter()
            .flat_map(|wall| &wall.placements)
            .filter_map(|placement| match &placement.source {
                WallContentSource::Image(id) => Some(id.clone()),
                _ => None,
            })
            .collect();
        let result = action(ids);
        drop(state);
        Ok(result)
    }
}
