use super::catalog::{ensure_owner, find_endpoint};
use super::*;

impl WallStore {
    pub(crate) fn connect(
        &self,
        endpoint_id: &str,
        actor: &str,
    ) -> WallResult<EndpointLeaseResponse> {
        let mut state = self.lock()?;
        ensure_owner(find_endpoint(&state.document, endpoint_id)?, Some(actor))?;
        let now = Instant::now();
        if state
            .leases
            .get(endpoint_id)
            .is_some_and(|lease| lease.deadline > now)
        {
            return Err(WallStoreError::conflict(
                "endpoint already has an active presenter",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        state.leases.insert(
            endpoint_id.to_owned(),
            EndpointLease {
                id: id.clone(),
                deadline: now + LEASE_TTL,
                sequence: 0,
                applied_revision: None,
                scene: None,
                presentation: None,
                identification: None,
            },
        );
        Ok(EndpointLeaseResponse {
            protocol_version: WALL_PROTOCOL_VERSION,
            lease_id: id,
            lease_ttl_ms: LEASE_TTL.as_millis() as u64,
        })
    }

    #[cfg(test)]
    pub(crate) fn heartbeat(
        &self,
        endpoint_id: &str,
        actor: &str,
        lease_id: &str,
        sequence: u64,
        applied_revision: Option<u64>,
        presentation: Option<WallPresentationReport>,
    ) -> WallResult<()> {
        self.heartbeat_with_scene(
            endpoint_id,
            actor,
            lease_id,
            sequence,
            applied_revision,
            presentation,
            None,
        )
    }

    pub(crate) fn heartbeat_with_scene(
        &self,
        endpoint_id: &str,
        actor: &str,
        lease_id: &str,
        sequence: u64,
        applied_revision: Option<u64>,
        presentation: Option<WallPresentationReport>,
        scene: Option<WallSceneReport>,
    ) -> WallResult<()> {
        let mut state = self.lock()?;
        ensure_owner(find_endpoint(&state.document, endpoint_id)?, Some(actor))?;
        let layout = state.document.layouts.iter().find(|layout| {
            layout
                .tiles
                .iter()
                .any(|tile| tile.endpoint_id == endpoint_id)
        });
        let current_revision = layout.map(|layout| layout.revision);
        state
            .timeline
            .validate_report(layout, applied_revision, scene)?;
        if applied_revision.is_some() && applied_revision != current_revision {
            return Err(WallStoreError::conflict(
                "presenter reports a stale layout revision",
            ));
        }
        if let Some(report) = presentation {
            super::presentation::validate_report(
                &state.document,
                layout,
                applied_revision,
                report,
            )?;
        }
        let lease = lease_mut(&mut state, endpoint_id, lease_id)?;
        if sequence <= lease.sequence || sequence > WALL_MAX_REVISION {
            return Err(WallStoreError::conflict(
                "presenter sequence is stale or exhausted",
            ));
        }
        lease.sequence = sequence;
        lease.applied_revision = applied_revision;
        lease.scene = scene;
        lease.presentation = presentation;
        lease.deadline = Instant::now() + LEASE_TTL;
        Ok(())
    }

    pub(crate) fn disconnect(
        &self,
        endpoint_id: &str,
        actor: &str,
        lease_id: &str,
    ) -> WallResult<()> {
        let mut state = self.lock()?;
        ensure_owner(find_endpoint(&state.document, endpoint_id)?, Some(actor))?;
        lease_mut(&mut state, endpoint_id, lease_id)?;
        state.leases.remove(endpoint_id);
        Ok(())
    }
}

pub(super) fn lease_mut<'a>(
    state: &'a mut WallState,
    endpoint_id: &str,
    lease_id: &str,
) -> WallResult<&'a mut EndpointLease> {
    state
        .leases
        .get_mut(endpoint_id)
        .filter(|lease| lease.id == lease_id && lease.deadline > Instant::now())
        .ok_or_else(|| {
            WallStoreError::new(
                409,
                "wall_lease_invalid",
                "presenter lease is missing, expired or replaced",
            )
        })
}
