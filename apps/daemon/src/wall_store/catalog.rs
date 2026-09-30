use super::persistence::next_document;
use super::*;

impl WallStore {
    pub(crate) fn snapshot(&self, device_id: Option<&str>) -> WallResult<WallStateSnapshot> {
        let state = self.lock()?;
        Ok(snapshot(&state, device_id, Instant::now()))
    }

    pub(crate) fn register(
        &self,
        base_revision: u64,
        endpoint: TileEndpoint,
        actor: Option<&str>,
    ) -> WallResult<WallStateSnapshot> {
        validate_tile_endpoint(&endpoint).map_err(|error| WallStoreError::invalid(error.0))?;
        ensure_owner(&endpoint, actor)?;
        let mut state = self.lock()?;
        let mut next = next_document(&state, base_revision)?;
        let endpoint_id = endpoint.endpoint_id.clone();
        if let Some(existing) = next
            .endpoints
            .iter_mut()
            .find(|e| e.endpoint_id == endpoint_id)
        {
            ensure_owner(existing, actor)?;
            if existing.device_id != endpoint.device_id || existing.output_id != endpoint.output_id
            {
                return Err(WallStoreError::conflict(
                    "endpoint identity binding cannot change",
                ));
            }
            if *existing == endpoint {
                return Ok(snapshot(&state, actor, Instant::now()));
            }
            *existing = endpoint;
        } else {
            next.endpoints.push(endpoint);
        }
        // A pixel-size change changes input mapping even when logical rectangles
        // stay put; every affected layout receives the new catalog revision.
        let mut affected = BTreeSet::new();
        for layout in &mut next.layouts {
            if layout
                .tiles
                .iter()
                .any(|tile| tile.endpoint_id == endpoint_id)
            {
                layout.revision = next.revision;
                affected.extend(layout.tiles.iter().map(|tile| tile.endpoint_id.clone()));
            }
        }
        self.commit(&mut state, next)?;
        state.leases.remove(&endpoint_id);
        invalidate_applied_revisions(&mut state, &affected);
        Ok(snapshot(&state, actor, Instant::now()))
    }

    pub(crate) fn remove_endpoint(
        &self,
        base_revision: u64,
        endpoint_id: &str,
        actor: Option<&str>,
    ) -> WallResult<WallStateSnapshot> {
        let mut state = self.lock()?;
        let mut next = next_document(&state, base_revision)?;
        ensure_owner(find_endpoint(&next, endpoint_id)?, actor)?;
        if next.layouts.iter().any(|layout| {
            layout
                .tiles
                .iter()
                .any(|tile| tile.endpoint_id == endpoint_id)
        }) {
            return Err(WallStoreError::conflict(
                "remove endpoint from its wall first",
            ));
        }
        next.endpoints
            .retain(|endpoint| endpoint.endpoint_id != endpoint_id);
        self.commit(&mut state, next)?;
        state.leases.remove(endpoint_id);
        Ok(snapshot(&state, actor, Instant::now()))
    }

    pub(crate) fn put_layout(
        &self,
        base_revision: u64,
        layout: WallLayout,
    ) -> WallResult<WallStateSnapshot> {
        self.put_layout_scheduled(base_revision, layout, 0)
    }

    pub(crate) fn put_layout_scheduled(
        &self,
        base_revision: u64,
        layout: WallLayout,
        activation_delay_ms: u64,
    ) -> WallResult<WallStateSnapshot> {
        if activation_delay_ms > 10_000 {
            return Err(WallStoreError::invalid(
                "scene preparation exceeds 10 seconds",
            ));
        }
        validate_wall_layout(&layout).map_err(|error| WallStoreError::invalid(error.0))?;
        let mut state = self.lock()?;
        let mut next = next_document(&state, base_revision)?;
        if activation_delay_ms > 0
            && layout.tiles.iter().any(|tile| {
                find_endpoint(&next, &tile.endpoint_id)
                    .map_or(true, |endpoint| !endpoint.scheduled_presentation)
            })
        {
            return Err(WallStoreError::new(
                409,
                "wall_scene_scheduling_unavailable",
                "every output must support scheduled presentation",
            ));
        }
        let wall_id = layout.wall_id.clone();
        if layout.revision != next.revision {
            return Err(WallStoreError::conflict(
                "layout revision must equal baseRevision + 1",
            ));
        }
        let affected = layout
            .tiles
            .iter()
            .map(|tile| tile.endpoint_id.clone())
            .chain(
                next.layouts
                    .iter()
                    .filter(|old| old.wall_id == layout.wall_id)
                    .flat_map(|old| old.tiles.iter().map(|tile| tile.endpoint_id.clone())),
            )
            .collect::<BTreeSet<_>>();
        if let Some(existing) = next
            .layouts
            .iter_mut()
            .find(|old| old.wall_id == layout.wall_id)
        {
            *existing = layout;
        } else {
            next.layouts.push(layout);
        }
        self.commit(&mut state, next)?;
        invalidate_applied_revisions(&mut state, &affected);
        state.timeline.defer(&wall_id, activation_delay_ms);
        Ok(snapshot(&state, None, Instant::now()))
    }

    pub(crate) fn remove_layout(
        &self,
        base_revision: u64,
        wall_id: &str,
    ) -> WallResult<WallStateSnapshot> {
        let mut state = self.lock()?;
        let mut next = next_document(&state, base_revision)?;
        let layout = next
            .layouts
            .iter()
            .find(|layout| layout.wall_id == wall_id)
            .ok_or_else(|| WallStoreError::new(404, "wall_not_found", "wall was not found"))?;
        let affected = layout
            .tiles
            .iter()
            .map(|tile| tile.endpoint_id.clone())
            .collect();
        next.layouts.retain(|layout| layout.wall_id != wall_id);
        next.presentations
            .retain(|control| control.wall_id != wall_id);
        self.commit(&mut state, next)?;
        invalidate_applied_revisions(&mut state, &affected);
        Ok(snapshot(&state, None, Instant::now()))
    }
}

fn invalidate_applied_revisions(state: &mut WallState, endpoints: &BTreeSet<String>) {
    for endpoint_id in endpoints {
        if let Some(lease) = state.leases.get_mut(endpoint_id) {
            lease.applied_revision = None;
            lease.scene = None;
            lease.presentation = None;
            lease.identification = None;
        }
    }
}

pub(super) fn find_endpoint<'a>(
    document: &'a WallDocument,
    endpoint_id: &str,
) -> WallResult<&'a TileEndpoint> {
    document
        .endpoints
        .iter()
        .find(|endpoint| endpoint.endpoint_id == endpoint_id)
        .ok_or_else(|| {
            WallStoreError::new(404, "wall_endpoint_not_found", "endpoint was not found")
        })
}

pub(super) fn ensure_owner(endpoint: &TileEndpoint, actor: Option<&str>) -> WallResult<()> {
    if actor.is_some_and(|id| id != endpoint.device_id) {
        return Err(WallStoreError::new(
            403,
            "wall_endpoint_forbidden",
            "endpoint belongs to another device",
        ));
    }
    Ok(())
}

pub(super) fn snapshot(
    state: &WallState,
    device_id: Option<&str>,
    now: Instant,
) -> WallStateSnapshot {
    let endpoints: Vec<_> = state
        .document
        .endpoints
        .iter()
        .filter(|endpoint| device_id.is_none_or(|id| endpoint.device_id == id))
        .map(|endpoint| {
            let lease = state
                .leases
                .get(&endpoint.endpoint_id)
                .filter(|lease| lease.deadline > now);
            EndpointStatus {
                endpoint: endpoint.clone(),
                online: lease.is_some(),
                applied_revision: lease.and_then(|lease| lease.applied_revision),
                scene: lease.and_then(|lease| lease.scene),
                presentation: lease.and_then(|lease| lease.presentation),
                identification: lease
                    .and_then(|lease| lease.identification.as_ref()?.snapshot(now)),
            }
        })
        .collect();
    let ids = endpoints
        .iter()
        .map(|status| &status.endpoint.endpoint_id)
        .collect::<BTreeSet<_>>();
    let layouts: Vec<_> = state
        .document
        .layouts
        .iter()
        .filter(|layout| {
            device_id.is_none()
                || layout
                    .tiles
                    .iter()
                    .any(|tile| ids.contains(&tile.endpoint_id))
        })
        .cloned()
        .collect();
    let presentations = state
        .document
        .presentations
        .iter()
        .filter(|control| {
            layouts
                .iter()
                .any(|layout| layout.wall_id == control.wall_id)
        })
        .cloned()
        .collect();
    WallStateSnapshot {
        protocol_version: WALL_PROTOCOL_VERSION,
        revision: state.document.revision,
        endpoints,
        timing: state.timeline.snapshot(&layouts, now),
        layouts,
        presentations,
    }
}
