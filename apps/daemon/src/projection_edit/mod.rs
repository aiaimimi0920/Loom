//! Authoritative edit sessions shared by a source Unit's projection bindings.
//! Bitmap v1 remains unchanged until Hook opts in to this separate document contract.
use crate::{DeviceRegistryStore, ManagedDevice, ProjectionError, ProjectionRecord};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, io::ErrorKind, path::PathBuf};
mod model;
mod mutation;
mod store;
#[cfg(test)]
mod tests;
pub(crate) use model::Access;
use model::*;
pub(crate) use store::Store;
type EditResult<T> = std::result::Result<T, ProjectionError>;
fn error(status: u16, code: &'static str) -> ProjectionError {
    ProjectionError::new(status, code)
}

fn authorized<'a>(
    registry: &'a DeviceRegistryStore,
    device: &ManagedDevice,
    id: &str,
    now: u64,
) -> EditResult<&'a ProjectionRecord> {
    let record = registry.projections.get(id)?;
    crate::projection_source_authorized(registry, record)?;
    if device.id != record.envelope.source.device_id
        && (record.receiver.as_deref() != Some(&device.id)
            || record.receiver_epoch != Some(device.session_epoch))
    {
        return Err(error(403, "projection_access_denied"));
    }
    if record.unlinked {
        return Err(error(410, "projection_unlinked"));
    }
    if record.receiver.is_none() && record.envelope.expires_at_ms <= now {
        return Err(error(410, "projection_invitation_expired"));
    }
    Ok(record)
}

pub(crate) fn handle(
    body: &str,
    device: &ManagedDevice,
    registry: &mut DeviceRegistryStore,
    now: u64,
) -> EditResult<Value> {
    let request = parse(body)?;
    let record = authorized(registry, device, request.projection_id(), now)?.clone();
    let access = Access {
        projection_id: record.envelope.projection_id.clone(),
        source_device_id: record.envelope.source.device_id.clone(),
        source_unit_id: record.envelope.source.unit_id.clone(),
        source_epoch: record.source_epoch,
        actor: device.id.clone(),
        is_source: device.id == record.envelope.source.device_id,
        offline: false,
        digest: record.digest.clone(),
        width: record.snapshot.width,
        height: record.snapshot.height,
    };
    if matches!(request, Request::Initialize { .. } | Request::Attach { .. }) {
        prune_shared(registry, now)?;
    }
    handle_authorized(body, access, &mut registry.projection_edits)
}

pub(crate) fn prune_shared(registry: &mut DeviceRegistryStore, now: u64) -> EditResult<()> {
    let active = registry
        .projections
        .records
        .iter()
        .filter(|(_, record)| {
            !record.unlinked
                && (record.receiver.is_some() || record.envelope.expires_at_ms > now)
                && crate::projection_source_authorized(registry, record).is_ok()
        })
        .map(|(id, _)| id.clone())
        .collect();
    registry.projection_edits.prune(&active)
}

fn parse(body: &str) -> EditResult<Request> {
    if body.len() > MAX_SESSION_BYTES {
        return Err(error(413, "projection_request_budget"));
    }
    serde_json::from_str(body).map_err(|_| error(400, "projection_invalid_request"))
}

pub(crate) fn needs_prune(body: &str) -> bool {
    matches!(
        parse(body),
        Ok(Request::Initialize { .. } | Request::Attach { .. })
    )
}

/// Callers must authorize a live binding under their trust/device locks first.
pub(crate) fn handle_authorized(
    body: &str,
    access: Access,
    store: &mut Store,
) -> EditResult<Value> {
    let request = parse(body)?;
    if request.projection_id() != access.projection_id {
        return Err(error(403, "projection_access_denied"));
    }
    store.available()?;
    let digest = crate::sha256_bytes(
        &serde_json::to_vec(&request).map_err(|_| error(400, "projection_invalid_request"))?,
    );
    match request {
        Request::Initialize {
            projection_id,
            session_id,
            expected_digest,
            objects,
        } => {
            access.source_only()?;
            if expected_digest != access.digest {
                return Err(error(409, "projection_edit_basis_conflict"));
            }
            if let Some(existing) = store.find(&projection_id) {
                if existing.session_id == session_id && existing.initialization_digest == digest {
                    return Ok(existing.response());
                }
                return Err(error(409, "projection_edit_already_bound"));
            }
            if store.get(&session_id).is_ok() {
                return Err(error(409, "projection_edit_session_reused"));
            }
            let session = Session {
                schema: SCHEMA.to_owned(),
                session_id,
                source_device_id: access.source_device_id,
                source_unit_id: access.source_unit_id,
                source_epoch: access.source_epoch,
                initialization_digest: digest,
                basis: Basis {
                    digest: access.digest,
                    width: access.width,
                    height: access.height,
                },
                offline_bindings: if access.offline {
                    [projection_id.clone()].into()
                } else {
                    Default::default()
                },
                bindings: [projection_id].into(),
                revision: 1,
                mode_revision: 1,
                checkpoint_revision: 1,
                mode: Mode::OneWay,
                receipts: BTreeMap::new(),
                objects: objects
                    .into_iter()
                    .map(|(id, value)| {
                        (
                            id,
                            Object {
                                revision: 1,
                                value: Some(value),
                            },
                        )
                    })
                    .collect(),
            };
            if !session.valid() {
                return Err(error(400, "projection_edit_invalid_document"));
            }
            store.commit(session)
        }
        Request::Attach {
            projection_id,
            session_id,
        } => {
            access.source_only()?;
            let mut session = store.get(&session_id)?.clone();
            if session.source_device_id != access.source_device_id
                || session.source_epoch != access.source_epoch
                || session.source_unit_id != access.source_unit_id
            {
                return Err(error(403, "projection_source_mismatch"));
            }
            if store
                .find(&projection_id)
                .is_some_and(|existing| existing.session_id != session_id)
            {
                return Err(error(409, "projection_edit_already_bound"));
            }
            if session.bindings.contains(&projection_id) {
                return Ok(session.response());
            }
            if session.basis.digest != access.digest
                || session.basis.width != access.width
                || session.basis.height != access.height
            {
                return Err(error(409, "projection_edit_basis_conflict"));
            }
            if access.offline {
                session.offline_bindings.insert(projection_id.clone());
            }
            session.bindings.insert(projection_id);
            store.commit(session)
        }
        request => {
            let mut session = store
                .find(request.projection_id())
                .ok_or_else(|| error(404, "projection_edit_not_found"))?
                .clone();
            if session.source_device_id != access.source_device_id
                || session.source_epoch != access.source_epoch
                || session.source_unit_id != access.source_unit_id
            {
                return Err(error(403, "projection_source_mismatch"));
            }
            if matches!(&request, Request::Read { .. }) {
                return Ok(session.response());
            }
            if matches!(&request, Request::Mode { session_id, .. } | Request::Apply { session_id, .. } | Request::Checkpoint { session_id, .. }
                if session_id != &session.session_id)
            {
                return Err(error(409, "projection_edit_session_mismatch"));
            }
            if !mutation::apply(&mut session, request, &access, digest)? {
                return Ok(session.response());
            }
            store.commit(session)
        }
    }
}
