use super::*;
impl OfflinePeers {
    pub(super) fn create_transfer(
        &self,
        actor: &str,
        input: Create,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        let now = unix_time_millis();
        if input.envelope.source.device_id != actor
            || input.envelope.source.revision != 1
            || input.target_device_id != model::target_id(&input.peer_id, &input.remote_device_id)
        {
            return Err(failure(403, "projection_access_denied"));
        }
        let source = {
            let registry = registry
                .lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            device(&registry, actor)?
        };
        let key = source
            .public_key
            .clone()
            .ok_or_else(|| failure(403, "projection_access_denied"))?;
        verify_projection_signature(&input.envelope, &key).map_err(convert)?;
        validate_projection_snapshot(&input.snapshot, &input.envelope.content.digest)
            .map_err(convert)?;
        if input.envelope.expires_at_ms <= now
            || input.envelope.expires_at_ms > now.saturating_add(300_000)
        {
            return Err(failure(410, "projection_invitation_expired"));
        }
        let record = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure(503, "peer_unavailable"))?;
            if !state
                .document
                .peers
                .get(&input.peer_id)
                .is_some_and(|p| p.enabled)
            {
                return Err(failure(403, "projection_peer_revoked"));
            }
            let registry = registry
                .try_lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            let mut store = self
                .transfers
                .lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            if let Some(record) = store.records.get(&input.envelope.projection_id) {
                authorized(&state, &registry, record)?;
                if record.incoming
                    || record.envelope != input.envelope
                    || record.peer_id != input.peer_id
                    || record.target_id != input.remote_device_id
                {
                    return Err(failure(409, "projection_invitation_replayed"));
                }
                record.active()?;
                record.clone()
            } else {
                let record = Record {
                    incoming: false,
                    peer_id: input.peer_id,
                    trust_revision: state.document.revision,
                    revision: 1,
                    digest: input.envelope.content.digest.clone(),
                    envelope: input.envelope,
                    snapshot: input.snapshot,
                    source_key: key,
                    source_name: source.name,
                    source_epoch: source.session_epoch,
                    target_id: input.remote_device_id,
                    target_epoch: None,
                    receiver_unit: None,
                    status: DeliveryStatus::AwaitingConfirmation,
                    unlinked: false,
                    updated_ms: now,
                };
                authorized(&state, &registry, &record)?;
                record.validate_metadata()?;
                store.commit(record.clone())?;
                record
            }
        };
        let offer = Offer {
            envelope: record.envelope.clone(),
            snapshot: record.snapshot.clone(),
            source_key: record.source_key.clone(),
            source_name: record.source_name.clone(),
            source_epoch: record.source_epoch,
            target_id: record.target_id.clone(),
        };
        let response = self.exchange(
            &record.peer_id,
            record.trust_revision,
            json!({"action": "offer", "body": offer}),
        )?;
        let epoch = response["targetEpoch"]
            .as_u64()
            .ok_or_else(|| failure(502, "peer_invalid_response"))?;
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        let registry = registry
            .try_lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        let mut store = self
            .transfers
            .lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        // A receiver may accept while the offer response is in flight; preserve that committed state.
        let mut current = store.get(&record.envelope.projection_id)?;
        authorized(&state, &registry, &current)?;
        if current.target_epoch.is_some_and(|old| old != epoch) {
            return Err(failure(403, "projection_access_denied"));
        }
        current.target_epoch = Some(epoch);
        let response = current.response(false);
        store.commit(current)?;
        Ok(response)
    }
}
