//! Edit authority remains at the source Loom. Peer signatures and epochs scope actors.
use super::*;

pub(super) fn prune(
    state: &State,
    registry: &mut DeviceRegistryStore,
    store: &Store,
) -> PeerResult<()> {
    // Cold initialize/attach transitions reclaim expired or revoked bindings in
    // one bounded pass, without making a shared-Loom read guess peer liveness.
    let active = store
        .records
        .values()
        .filter(|record| {
            !record.incoming
                && record.active().is_ok()
                && authorized(state, registry, record).is_ok()
        })
        .map(|record| record.envelope.projection_id.clone())
        .collect();
    registry
        .projection_edits
        .prune_offline(&active)
        .map_err(convert)?;
    crate::projection_edit::prune_shared(registry, unix_time_millis()).map_err(convert)
}

pub(super) fn access(record: &Record, source: bool) -> crate::projection_edit::Access {
    crate::projection_edit::Access {
        projection_id: record.envelope.projection_id.clone(),
        source_device_id: record.envelope.source.device_id.clone(),
        source_unit_id: record.envelope.source.unit_id.clone(),
        source_epoch: record.source_epoch,
        actor: if source {
            record.envelope.source.device_id.clone()
        } else {
            model::target_id(
                &record.peer_id,
                &format!(
                    "{}\n{}",
                    record.target_id,
                    record.target_epoch.unwrap_or_default()
                ),
            )
        },
        is_source: source,
        offline: true,
        digest: record.digest.clone(),
        width: record.snapshot.width,
        height: record.snapshot.height,
    }
}

pub(super) fn document(
    registry: &DeviceRegistryStore,
    record: &Record,
    mut value: Value,
) -> PeerResult<Value> {
    if let Some(editing) = registry
        .projection_edits
        .document(&record.envelope.projection_id)
        .map_err(convert)?
    {
        value["editing"] = editing;
    }
    Ok(value)
}

impl OfflinePeers {
    pub(crate) fn prune_edit_sessions(
        &self,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<()> {
        let state = self
            .state
            .try_lock()
            .map_err(|_| failure(429, "projection_busy"))?;
        let mut registry = registry
            .try_lock()
            .map_err(|_| failure(429, "projection_busy"))?;
        let store = self
            .transfers
            .try_lock()
            .map_err(|_| failure(429, "projection_busy"))?;
        prune(&state, &mut registry, &store)
    }

    pub(super) fn forward_edit(
        &self,
        actor: &str,
        record: Record,
        body: Value,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        record.active()?;
        if record.receiver_unit.is_none() {
            return Err(failure(403, "projection_access_denied"));
        }
        let response = self.exchange(
            &record.peer_id,
            record.trust_revision,
            local::remote_call(&record, "receiver", "edit", body),
        )?;
        // I/O must not hold trust/registry locks; recheck revocation and stop afterwards.
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        let registry = registry
            .try_lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        let store = self
            .transfers
            .lock()
            .map_err(|_| failure(503, "projection_busy"))?;
        let current = store.get(&record.envelope.projection_id)?;
        authorized(&state, &registry, &current)?;
        current.active()?;
        if current.target_id != actor || current.receiver_unit != record.receiver_unit {
            return Err(failure(403, "projection_access_denied"));
        }
        Ok(response)
    }
}
