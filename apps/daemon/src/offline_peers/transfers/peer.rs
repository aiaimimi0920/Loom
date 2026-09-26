use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoteAction {
    projection_id: String,
    source_epoch: u64,
    target_id: String,
    target_epoch: Option<u64>,
    operation: String,
    input: Value,
}
impl OfflinePeers {
    pub(super) fn peer_action(
        &self,
        peer_id: &str,
        revision: u64,
        mut payload: Value,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        if payload["action"] == "offer" {
            let offer: Offer = actions::from(payload["body"].take())?;
            verify_projection_signature(&offer.envelope, &offer.source_key).map_err(convert)?;
            validate_projection_snapshot(&offer.snapshot, &offer.envelope.content.digest)
                .map_err(convert)?;
            let now = unix_time_millis();
            if offer.envelope.expires_at_ms <= now
                || offer.envelope.expires_at_ms > now.saturating_add(300_000)
            {
                return Err(failure(410, "projection_invitation_expired"));
            }
            let state = self
                .state
                .lock()
                .map_err(|_| failure(503, "peer_unavailable"))?;
            if state.document.revision != revision {
                return Err(failure(403, "projection_peer_revoked"));
            }
            let registry = registry
                .try_lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            let target = device(&registry, &offer.target_id)?;
            let mut store = self
                .transfers
                .lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            if let Some(record) = store.records.get(&offer.envelope.projection_id) {
                authorized(&state, &registry, record)?;
                if !record.incoming
                    || record.peer_id != peer_id
                    || record.envelope != offer.envelope
                    || record.source_epoch != offer.source_epoch
                    || record.source_key != offer.source_key
                    || record.target_id != offer.target_id
                {
                    return Err(failure(409, "projection_invitation_replayed"));
                }
                record.active()?;
                return Ok(json!({"targetEpoch": record.target_epoch}));
            }
            if !registry
                .projections
                .presence
                .get(&target.id)
                .is_some_and(|presence| {
                    presence.epoch == target.session_epoch
                        && now.saturating_sub(presence.seen) < PROJECTION_PRESENCE_TTL
                        && presence.policy != ProjectionReceivePolicy::Disabled
                })
            {
                return Err(failure(409, "projection_target_offline"));
            }
            let record = Record {
                incoming: true,
                peer_id: peer_id.to_owned(),
                trust_revision: revision,
                revision: offer.envelope.source.revision,
                digest: offer.envelope.content.digest.clone(),
                envelope: offer.envelope,
                snapshot: offer.snapshot,
                source_key: offer.source_key,
                source_name: offer.source_name,
                source_epoch: offer.source_epoch,
                target_id: offer.target_id,
                target_epoch: Some(target.session_epoch),
                receiver_unit: None,
                status: DeliveryStatus::AwaitingConfirmation,
                unlinked: false,
                updated_ms: now,
            };
            record.validate_metadata()?;
            store.commit(record)?;
            return Ok(json!({"targetEpoch": target.session_epoch}));
        }
        if payload["action"] != "receiver" && payload["action"] != "stop" {
            return Err(failure(400, "projection_invalid_request"));
        }
        let stop = payload["action"] == "stop";
        let input: RemoteAction = actions::from(payload["body"].take())?;
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        if state.document.revision != revision {
            return Err(failure(403, "projection_peer_revoked"));
        }
        let registry = registry
            .try_lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        let mut store = self
            .transfers
            .lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        let mut record = store.get(&input.projection_id)?;
        authorized(&state, &registry, &record)?;
        if record.peer_id != peer_id
            || record.source_epoch != input.source_epoch
            || record.target_id != input.target_id
            || record.incoming != stop
            || (record.target_epoch.is_some() && record.target_epoch != input.target_epoch)
            || input.target_epoch.is_none()
        {
            return Err(failure(403, "projection_access_denied"));
        }
        let epoch_changed = record.target_epoch != input.target_epoch;
        record.target_epoch = input.target_epoch;
        let response = if stop {
            record.unlinked = true;
            json!({"unlinked": true})
        } else {
            actions::receiver_action(&mut record, &input.operation, input.input)?
        };
        if stop || epoch_changed || !matches!(input.operation.as_str(), "read" | "inspect") {
            store.commit(record)?;
        }
        Ok(response)
    }
}
