//! Peer access to one canonical shared invitation; no second source snapshot is persisted.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    envelope: ProjectionEnvelope,
    target_id: String,
    target_epoch: u64,
    source_epoch: Option<u64>,
    operation: String,
    input: Value,
}

impl OfflinePeers {
    pub(super) fn shared_source_action(
        &self,
        peer: &str,
        revision: u64,
        body: Value,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        let input: Request = actions::from(body)?;
        if !loom_protocol::projection::projection_identifier_valid(&input.target_id) {
            return Err(failure(400, "projection_invalid_request"));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        if state.document.revision != revision
            || !state.document.peers.get(peer).is_some_and(|p| p.enabled)
        {
            return Err(failure(403, "projection_peer_revoked"));
        }
        let mut registry = registry
            .try_lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        let mut source = registry
            .projections
            .get(&input.envelope.projection_id)
            .map_err(convert)?
            .clone();
        projection_source_authorized(&registry, &source).map_err(convert)?;
        if source.envelope != input.envelope
            || source.delivery.is_some()
            || input
                .source_epoch
                .is_some_and(|epoch| epoch != source.source_epoch)
        {
            return Err(failure(403, "projection_access_denied"));
        }
        let device = device(&registry, &source.envelope.source.device_id)?;
        let bound = source.remote_receiver.as_ref();
        if let Some(bound) = bound {
            if bound.peer_id != peer
                || bound.device_id != input.target_id
                || bound.trust_revision != revision
                || source.receiver_epoch != Some(input.target_epoch)
            {
                return Err(failure(403, "projection_access_denied"));
            }
        } else if source.receiver.is_some() {
            return Err(failure(409, "projection_invitation_consumed"));
        }
        let mut record = Record {
            incoming: true,
            shared_source: true,
            peer_id: state.document.identity.key_id.clone(),
            trust_revision: revision,
            envelope: source.envelope.clone(),
            snapshot: source.snapshot.clone(),
            source_key: device
                .public_key
                .ok_or_else(|| failure(403, "projection_source_revoked"))?,
            source_name: device.name,
            source_epoch: source.source_epoch,
            target_id: input.target_id.clone(),
            target_epoch: Some(input.target_epoch),
            receiver_unit: source.receiver_unit_id.clone(),
            revision: source.revision,
            digest: source.digest.clone(),
            status: bound.map_or(DeliveryStatus::AwaitingConfirmation, |r| r.status),
            unlinked: source.unlinked,
            updated_ms: source.updated_at_ms,
        };
        if input.operation == "import" {
            record.active()?;
            return Ok(json!({"record":record}));
        }
        if input.operation == "edit" {
            record.active()?;
            if bound.is_none() {
                return Err(failure(403, "projection_access_denied"));
            }
            let mut access = edit::access(&record, false);
            access.actor = model::target_id(
                peer,
                &format!("{}\n{}", input.target_id, input.target_epoch),
            );
            access.offline = false; // The source's shared edit document owns this binding.
            let body = serde_json::to_string(&input.input)
                .map_err(|_| failure(400, "projection_invalid_request"))?;
            return crate::projection_edit::handle_authorized(
                &body,
                access,
                &mut registry.projection_edits,
            )
            .map_err(convert);
        }
        if input.operation == "unlink" && bound.is_none() {
            return Err(failure(403, "projection_access_denied"));
        }
        let response = actions::receiver_action(&mut record, &input.operation, input.input)?;
        // A declined preview belongs only to that receiving device; it cannot consume a public invitation.
        if bound.is_none() && record.receiver_unit.is_none() {
            return Ok(response);
        }
        if !matches!(input.operation.as_str(), "read" | "inspect") {
            source.receiver = Some(model::target_id(peer, &input.target_id));
            source.receiver_epoch = Some(input.target_epoch);
            source.receiver_unit_id = record.receiver_unit.clone();
            source.remote_receiver = Some(ProjectionRemoteReceiver {
                peer_id: peer.into(),
                device_id: input.target_id,
                trust_revision: revision,
                status: record.status,
            });
            source.unlinked = record.unlinked;
            registry.projections.commit(source).map_err(convert)?;
            if record.unlinked {
                registry
                    .projection_edits
                    .release(&record.envelope.projection_id)
                    .map_err(convert)?;
            }
        }
        edit::document(&registry, &record, response)
    }
}
