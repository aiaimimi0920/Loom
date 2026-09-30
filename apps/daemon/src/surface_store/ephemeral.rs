// Connection-owned views never become persistent attachments or recoverable input grants.
fn durable_surface_record(
    record: &SurfaceInstanceRecord,
) -> std::borrow::Cow<'_, SurfaceInstanceRecord> {
    let ephemeral: std::collections::BTreeSet<_> = record
        .attachments
        .values()
        .filter(|a| a.ephemeral)
        .map(|a| a.descriptor.attachment_id.as_str())
        .collect();
    if ephemeral.is_empty() && record.ephemeral_events.is_empty() {
        return std::borrow::Cow::Borrowed(record);
    }
    let mut projected = record.clone();
    projected
        .attachments
        .retain(|id, _| !ephemeral.contains(id.as_str()));
    let event_ids: std::collections::BTreeSet<_> = projected
        .pending_events
        .iter()
        .filter(|event| ephemeral.contains(event.attachment_id.as_str()))
        .map(|event| event.event_id.clone())
        .chain(record.ephemeral_events.values().flatten().cloned())
        .collect();
    projected
        .pending_events
        .retain(|event| !event_ids.contains(&event.event_id));
    projected.event_acks.retain(|id, _| !event_ids.contains(id));
    projected
        .pending_confirmations
        .retain(|_, pending| !ephemeral.contains(pending.request.attachment_id.as_str()));
    std::borrow::Cow::Owned(projected)
}

impl SurfaceInstanceStore {
    pub(crate) fn remove_ephemeral_attachment(
        &mut self,
        instance_id: &str,
        attachment_id: &str,
    ) -> Result<Option<SurfaceAttachmentRecord>, SurfaceStoreError> {
        self.transaction(|instances| {
            let Some(instance) = instances.get_mut(instance_id) else {
                return Ok(None);
            };
            let Some(attachment) = instance.attachments.get(attachment_id) else {
                return Ok(None);
            };
            if !attachment.ephemeral {
                return Err(SurfaceStoreError::Invalid(
                    "attachment is not connection-owned".into(),
                ));
            }
            let confirmations: std::collections::BTreeSet<_> = instance
                .pending_confirmations
                .values()
                .filter(|pending| pending.request.attachment_id == attachment_id)
                .map(|pending| pending.event.event_id.clone())
                .collect();
            // Accepted work keeps its execution context until it reaches a terminal state.
            // Unapproved confirmation requests have no running work and can be discarded.
            if ephemeral_surface_has_execution(instance, attachment_id)
                || instance.pending_events.iter().any(|event| {
                    event.attachment_id == attachment_id && !confirmations.contains(&event.event_id)
                })
            {
                return Err(SurfaceStoreError::Conflict(
                    "attachment still has accepted actions".into(),
                ));
            }
            instance
                .pending_confirmations
                .retain(|_, pending| pending.request.attachment_id != attachment_id);
            instance
                .pending_events
                .retain(|event| !confirmations.contains(&event.event_id));
            instance
                .event_acks
                .retain(|id, _| !confirmations.contains(id));
            for id in instance
                .ephemeral_events
                .remove(attachment_id)
                .unwrap_or_default()
            {
                instance.event_acks.remove(&id);
            }
            Ok(instance.attachments.remove(attachment_id))
        })
    }
}
