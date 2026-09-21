//! A bounded, lease-owned display marker never changes or persists wall geometry.
use super::catalog::{ensure_owner, find_endpoint, snapshot};
use super::*;

const IDENTIFICATION_TTL: Duration = Duration::from_secs(10);

pub(super) struct IdentificationControl {
    pub request_id: String,
    pub deadline: Instant,
    pub applied: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WallIdentification {
    pub request_id: String,
    pub remaining_ms: u64,
    pub applied: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WallIdentificationOutcome {
    Applied,
    Dismissed,
}

impl IdentificationControl {
    pub(super) fn snapshot(&self, now: Instant) -> Option<WallIdentification> {
        let remaining_ms = self.deadline.saturating_duration_since(now).as_millis() as u64;
        (remaining_ms > 0).then(|| WallIdentification {
            request_id: self.request_id.clone(),
            remaining_ms,
            applied: self.applied,
        })
    }
}

impl WallStore {
    pub(crate) fn endpoint_device(&self, endpoint_id: &str) -> WallResult<String> {
        Ok(find_endpoint(&self.lock()?.document, endpoint_id)?
            .device_id
            .clone())
    }

    pub(crate) fn identify_endpoint(&self, endpoint_id: &str) -> WallResult<WallStateSnapshot> {
        let mut state = self.lock()?;
        let endpoint = find_endpoint(&state.document, endpoint_id)?;
        if !endpoint
            .display
            .as_ref()
            .is_some_and(|display| display.can_identify)
        {
            return Err(WallStoreError::new(
                409,
                "wall_identification_unsupported",
                "endpoint cannot identify its display",
            ));
        }
        if let Some(layout) = state.document.layouts.iter().find(|layout| {
            layout
                .tiles
                .iter()
                .any(|tile| tile.endpoint_id == endpoint_id)
        }) {
            super::presentation::require_running(&state.document, &layout.wall_id)?;
        }
        let now = Instant::now();
        let lease = state
            .leases
            .get_mut(endpoint_id)
            .filter(|lease| lease.deadline > now)
            .ok_or_else(|| {
                WallStoreError::new(
                    409,
                    "wall_endpoint_offline",
                    "endpoint has no online presenter",
                )
            })?;
        // Repeated clicks do not extend an already active marker or grow a command queue.
        if lease
            .identification
            .as_ref()
            .is_none_or(|control| control.deadline <= now)
        {
            lease.identification = Some(IdentificationControl {
                request_id: uuid::Uuid::new_v4().to_string(),
                deadline: now + IDENTIFICATION_TTL,
                applied: false,
            });
        }
        Ok(snapshot(&state, None, now))
    }

    pub(crate) fn report_identification(
        &self,
        endpoint_id: &str,
        actor: &str,
        lease_id: &str,
        request_id: &str,
        outcome: WallIdentificationOutcome,
    ) -> WallResult<()> {
        if uuid::Uuid::parse_str(request_id).is_err() {
            return Err(WallStoreError::invalid("invalid identification request ID"));
        }
        let mut state = self.lock()?;
        ensure_owner(find_endpoint(&state.document, endpoint_id)?, Some(actor))?;
        let lease = super::leases::lease_mut(&mut state, endpoint_id, lease_id)?;
        if let Some(control) = &mut lease.identification {
            // Delayed reports cannot revive an expired request or acknowledge its successor.
            if control.deadline <= Instant::now() {
                lease.identification = None;
            } else if control.request_id == request_id {
                match outcome {
                    WallIdentificationOutcome::Applied => control.applied = true,
                    WallIdentificationOutcome::Dismissed => lease.identification = None,
                }
            }
        }
        Ok(())
    }
}

pub(super) fn require_input(lease: &EndpointLease) -> WallResult<()> {
    if lease
        .identification
        .as_ref()
        .is_some_and(|control| control.deadline > Instant::now())
    {
        return Err(WallStoreError::new(
            409,
            "wall_endpoint_identifying",
            "endpoint input is paused during identification",
        ));
    }
    Ok(())
}
