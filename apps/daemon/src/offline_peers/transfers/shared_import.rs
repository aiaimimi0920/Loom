//! Import routes only through the selected Loom or an explicitly pinned peer, never a link URL.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Import {
    source_loom_id: String,
    envelope: ProjectionEnvelope,
}
impl OfflinePeers {
    pub(super) fn import_shared(
        &self,
        actor: &str,
        input: Import,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        let (revision, epoch, local) = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure(503, "peer_unavailable"))?;
            let registry = registry
                .try_lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            let local = state.document.identity.key_id == input.source_loom_id;
            if !local
                && !state
                    .document
                    .peers
                    .get(&input.source_loom_id)
                    .is_some_and(|p| p.enabled)
            {
                return Err(failure(403, "projection_peer_revoked"));
            }
            (
                state.document.revision,
                device(&registry, actor)?.session_epoch,
                local,
            )
        };
        if local {
            let body = json!({"envelope":input.envelope}).to_string();
            let (status, text) =
                handle_projection_route("/v1/projections/inspect", &body, Some(actor), registry)
                    .map_err(|_| failure(503, "projection_unavailable"))?;
            let mut value: Value =
                serde_json::from_str(&text).map_err(|_| failure(502, "peer_invalid_response"))?;
            if status != 200 {
                return Err(failure(
                    status,
                    match value["error"]["code"].as_str() {
                        Some("projection_invitation_consumed") => "projection_invitation_consumed",
                        Some("projection_invitation_expired") => "projection_invitation_expired",
                        Some("projection_receiver_rejected") => "projection_receiver_rejected",
                        Some("projection_source_revoked") => "projection_source_revoked",
                        Some("projection_unlinked") => "projection_unlinked",
                        Some("projection_not_found") => "projection_not_found",
                        Some("projection_busy") => "projection_busy",
                        Some("projection_unavailable") => "projection_unavailable",
                        _ => "projection_access_denied",
                    },
                ));
            }
            value["sourceLoomId"] = json!(input.source_loom_id);
            value["route"] = json!("shared_loom");
            return Ok(value);
        }
        let response = self.exchange(&input.source_loom_id, revision, json!({"action":"share", "body":{
            "envelope":input.envelope,"targetId":actor,"targetEpoch":epoch,"sourceEpoch":null,"operation":"import","input":{}}}))?;
        let mut record: Record = actions::from(response["record"].clone())?;
        if !record.incoming
            || !record.shared_source
            || record.envelope != input.envelope
            || record.peer_id != input.source_loom_id
            || record.target_id != actor
            || record.target_epoch != Some(epoch)
        {
            return Err(failure(502, "peer_invalid_response"));
        }
        record.trust_revision = revision;
        record.validate()?;
        record.active()?;
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        let registry = registry
            .try_lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        authorized(&state, &registry, &record)?;
        crate::projection_receive_allowed(
            &registry,
            actor,
            &record.envelope.source.device_id,
            Some(&record.peer_id),
        )
        .map_err(convert)?;
        let mut store = self
            .transfers
            .lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        if let Some(old) = store.records.get(&record.envelope.projection_id) {
            authorized(&state, &registry, old)?;
            if !old.incoming
                || !old.shared_source
                || old.peer_id != record.peer_id
                || old.envelope != record.envelope
                || old.target_id != actor
                || old.source_epoch != record.source_epoch
                || old.source_key != record.source_key
            {
                return Err(failure(409, "projection_invitation_replayed"));
            }
            old.active()?;
        }
        let mut value = record.response(true);
        value["sourceLoomId"] = json!(input.source_loom_id);
        store.commit(record)?;
        Ok(value)
    }
}
