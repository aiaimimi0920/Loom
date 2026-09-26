//! Offline transfers keep foreign identities scoped to a pinned Loom, never in the local device registry.
use super::*;
use handshake::{Challenge, ProbeGuard};
mod actions;
mod create;
mod local;
mod model;
mod peer;
mod receiver;
mod store;
mod wire;
use model::{Create, Offer, Record};
pub(crate) use store::Store;
const MAX_BODY: usize = loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES;
pub(crate) const ROUTES: &[&str] = &[
    "/v1/offline-projections/create",
    "/v1/offline-projections/inbox",
    "/v1/offline-projections/inspect",
    "/v1/offline-projections/accept",
    "/v1/offline-projections/read",
    "/v1/offline-projections/update",
    "/v1/offline-projections/receipt",
    "/v1/offline-projections/unlink",
];
fn parse_transfer<T: serde::de::DeserializeOwned>(body: &str) -> PeerResult<T> {
    if body.len() > MAX_BODY {
        return Err(failure(413, "projection_request_budget"));
    }
    serde_json::from_str(body).map_err(|_| failure(400, "projection_invalid_request"))
}
fn convert(error: ProjectionError) -> PeerError {
    failure(error.status, error.code)
}
fn device(registry: &DeviceRegistryStore, id: &str) -> PeerResult<ManagedDevice> {
    registry
        .authorized_keyed_device(id)
        .cloned()
        .map_err(|_| failure(403, "projection_access_denied"))
}
fn authorized(state: &State, registry: &DeviceRegistryStore, record: &Record) -> PeerResult<()> {
    if state.document.revision != record.trust_revision
        || !state
            .document
            .peers
            .get(&record.peer_id)
            .is_some_and(|p| p.enabled)
    {
        return Err(failure(403, "projection_peer_revoked"));
    }
    let (id, epoch) = if record.incoming {
        (&record.target_id, record.target_epoch)
    } else {
        (&record.envelope.source.device_id, Some(record.source_epoch))
    };
    if Some(device(registry, id)?.session_epoch) != epoch {
        return Err(failure(403, "projection_access_denied"));
    }
    Ok(())
}
