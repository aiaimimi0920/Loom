// Connection-owned ack identities are bounded and never enter the durable source projection.
const MAX_EPHEMERAL_SURFACE_EVENTS: usize = 64;

fn terminal_surface_ack(ack: &SurfaceActionAck) -> bool {
    matches!(
        ack.status,
        SurfaceActionStatus::Succeeded
            | SurfaceActionStatus::Failed
            | SurfaceActionStatus::Cancelled
            | SurfaceActionStatus::Interrupted
    )
}

impl SurfaceInstanceRecord {
    pub(crate) fn ephemeral_event_acks(
        &self,
        attachment_id: &str,
    ) -> BTreeMap<String, SurfaceActionAck> {
        self.ephemeral_events
            .get(attachment_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.event_acks.get(id).map(|ack| (id.clone(), ack.clone())))
            .collect()
    }
}

fn validate_ephemeral_event_owner(
    instance: &SurfaceInstanceRecord,
    event: &SurfaceEvent,
) -> Result<(), SurfaceStoreError> {
    if !instance.event_acks.contains_key(&event.event_id) {
        return Ok(());
    }
    let owner = instance
        .ephemeral_events
        .iter()
        .find(|(_, ids)| ids.contains(&event.event_id))
        .map(|(owner, _)| owner);
    let ephemeral = instance
        .attachments
        .get(&event.attachment_id)
        .is_some_and(|view| view.ephemeral);
    if (ephemeral || owner.is_some()) && owner != Some(&event.attachment_id) {
        return Err(SurfaceStoreError::Conflict(
            "Surface event belongs to a different attachment".into(),
        ));
    }
    Ok(())
}

fn remember_ephemeral_event(
    instance: &mut SurfaceInstanceRecord,
    event: &SurfaceEvent,
) -> Result<(), SurfaceStoreError> {
    if !instance
        .attachments
        .get(&event.attachment_id)
        .is_some_and(|view| view.ephemeral)
    {
        return Ok(());
    }
    let ids = instance
        .ephemeral_events
        .entry(event.attachment_id.clone())
        .or_default();
    if ids.len() >= MAX_EPHEMERAL_SURFACE_EVENTS {
        let completed = ids
            .iter()
            .position(|id| {
                instance
                    .event_acks
                    .get(id)
                    .is_some_and(terminal_surface_ack)
                    && !instance
                        .pending_events
                        .iter()
                        .any(|pending| pending.event_id == *id)
            })
            .ok_or_else(|| {
                SurfaceStoreError::Conflict("Surface view has too much unfinished work".into())
            })?;
        if let Some(id) = ids.remove(completed) {
            instance.event_acks.remove(&id);
        }
    }
    ids.push_back(event.event_id.clone());
    Ok(())
}

fn ephemeral_surface_has_execution(instance: &SurfaceInstanceRecord, attachment_id: &str) -> bool {
    instance
        .ephemeral_events
        .get(attachment_id)
        .into_iter()
        .flatten()
        .any(|id| {
            instance.event_acks.get(id).is_none_or(|ack| {
                !terminal_surface_ack(ack)
                    && ack.status != SurfaceActionStatus::AwaitingConfirmation
            })
        })
}

impl SurfaceInstanceStore {
    pub(crate) fn event_ack_for_event(
        &self,
        instance_id: &str,
        event: &SurfaceEvent,
    ) -> Result<Option<SurfaceActionAck>, SurfaceStoreError> {
        let Some(instance) = self.instances.get(instance_id) else {
            return Ok(None);
        };
        // Check under both executor admission locks, including the early idempotency return.
        validate_ephemeral_event_owner(instance, event)?;
        Ok(instance.event_acks.get(&event.event_id).cloned())
    }
}
