use super::operation::CentralOperation;
use crate::{
    error, validation, CentralResponse, Identity, Peer, Policy, Record, Result, Status, View,
    MAX_PNG_BYTES, PROTOCOL,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::net::IpAddr;

const MAX_RELAYS: usize = 4;
const MAX_ENDPOINT_ADDRESSES: usize = 8;
const MAX_DEVICE_NAME_BYTES: usize = 80;
const MAX_AUTHORIZATION_LEASE_MS: u64 = 60_000;
const MAX_PRESENCE_TTL_MS: u64 = 120_000;

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorValue,
}

#[derive(Deserialize)]
struct ErrorValue {
    code: String,
}

pub(crate) fn server_error(status: u16, body: &[u8]) -> crate::Error {
    let code = serde_json::from_slice::<ErrorBody>(body)
        .ok()
        .and_then(|value| known_error_code(&value.error.code));
    error(status, code.unwrap_or("projection_request_failed"))
}

pub(crate) fn validate(
    value: &mut CentralResponse,
    operation: &CentralOperation,
    identity: &Identity,
    now: u64,
) -> Result<()> {
    match (operation, value) {
        (CentralOperation::Configuration, CentralResponse::Configuration { policy }) => {
            validate_policy(policy, identity)
        }
        (CentralOperation::Sync { .. }, CentralResponse::Sync { views }) => {
            if views.len() > 64 {
                return Err(invalid_response());
            }
            let mut ids = HashSet::with_capacity(views.len());
            for view in views {
                validate_view(view, identity, now, false)?;
                if !ids.insert(view.record.envelope.projection_id.clone()) {
                    return Err(invalid_response());
                }
            }
            Ok(())
        }
        (
            CentralOperation::Peer {
                projection_id,
                peer_device_id,
                peer_public_key,
                envelope,
            },
            CentralResponse::Peer {
                peer,
                authorized_until_ms,
            },
        ) => {
            if peer_device_id != &peer.device_id || peer_public_key != &peer.public_key {
                return Err(invalid_response());
            }
            if !validation::projection_id(projection_id) {
                return Err(invalid_response());
            }
            if let Some(envelope) = envelope {
                envelope.validate(identity.origin())?;
                if envelope.projection_id != *projection_id {
                    return Err(invalid_response());
                }
            }
            validate_peer(peer, identity)?;
            validate_lease(*authorized_until_ms, identity, now)
        }
        (_, CentralResponse::Projection { view }) => {
            validate_projection_view(view, operation, identity, now)
        }
        _ => Err(invalid_response()),
    }
}

fn validate_projection_view(
    view: &View,
    operation: &CentralOperation,
    identity: &Identity,
    now: u64,
) -> Result<()> {
    let allow_unlinked_receiver = matches!(operation, CentralOperation::Inspect { .. });
    validate_view(view, identity, now, allow_unlinked_receiver)?;
    let record = &view.record;
    match operation {
        CentralOperation::Create { envelope, .. }
        | CentralOperation::Inspect { envelope }
        | CentralOperation::Accept { envelope, .. } => {
            if record.envelope != *envelope {
                return Err(invalid_response());
            }
            if matches!(operation, CentralOperation::Create { .. })
                && (record.status != Status::Invited || record.receiver.is_some())
            {
                return Err(invalid_response());
            }
            if matches!(operation, CentralOperation::Inspect { .. })
                && (record.status != Status::Invited || record.receiver.is_some())
            {
                return Err(invalid_response());
            }
            if let CentralOperation::Accept { .. } = operation {
                if record
                    .receiver
                    .as_ref()
                    .is_none_or(|receiver| receiver.device_id != identity.session().device_id)
                {
                    return Err(invalid_response());
                }
            }
        }
        CentralOperation::Publish {
            projection_id,
            revision,
            digest,
            width,
            height,
            byte_length,
            ..
        } => {
            if record.envelope.projection_id != *projection_id
                || record.revision != *revision
                || record.digest != *digest
                || record.width != *width
                || record.height != *height
                || record.byte_length != *byte_length
                || record.envelope.source.device_id != identity.session().device_id
            {
                return Err(invalid_response());
            }
        }
        CentralOperation::Read { projection_id } | CentralOperation::Unlink { projection_id } => {
            if record.envelope.projection_id != *projection_id
                || !is_participant(record, identity.session().device_id.as_str())
            {
                return Err(invalid_response());
            }
            if matches!(operation, CentralOperation::Unlink { .. })
                && record.status != Status::Stopped
            {
                return Err(invalid_response());
            }
        }
        _ => return Err(invalid_response()),
    }
    Ok(())
}

fn validate_view(view: &View, identity: &Identity, now: u64, allow_guest: bool) -> Result<()> {
    validate_record(&view.record, identity)?;
    let participant = is_participant(&view.record, identity.session().device_id.as_str());
    if !participant
        && !(allow_guest
            && view.record.status == Status::Invited
            && view.record.receiver.is_none()
            && view.record.envelope.source.device_id != identity.session().device_id)
    {
        return Err(invalid_response());
    }
    if view.available {
        if view.authorized_until_ms <= now || view.authorized_until_ms > view.record.expires_at_ms {
            return Err(invalid_response());
        }
        if let Some(peer) = &view.peer {
            validate_peer(peer, identity)?;
        }
    } else if view.authorized_until_ms > now {
        return Err(invalid_response());
    }
    Ok(())
}

fn validate_record(record: &Record, identity: &Identity) -> Result<()> {
    record.envelope.validate(identity.origin())?;
    if record.envelope.source.account_id != identity.session().account_id
        || record.envelope.source.revision != 1
    {
        return Err(invalid_response());
    }
    validate_shape(
        record.initial_image.width,
        record.initial_image.height,
        record.initial_image.byte_length,
    )?;
    let image = record.image_metadata();
    image.validate()?;
    if record.revision == 1 && record.digest != record.envelope.content.digest {
        return Err(invalid_response());
    }
    if record.status == Status::Invited && record.receiver.is_some() {
        return Err(invalid_response());
    }
    if record.status == Status::Linked && record.receiver.is_none() {
        return Err(invalid_response());
    }
    if record.expires_at_ms < record.envelope.expires_at_ms || record.updated_at_ms == 0 {
        return Err(invalid_response());
    }
    if let Some(receiver) = &record.receiver {
        if uuid::Uuid::parse_str(&receiver.device_id).is_err()
            || !validation::identifier(&receiver.unit_id)
            || !validation::revision(receiver.revision)
            || !validation::hex(&receiver.digest, 64)
            || receiver.revision > record.revision
        {
            return Err(invalid_response());
        }
    }
    Ok(())
}

fn validate_policy(policy: &Policy, identity: &Identity) -> Result<()> {
    if policy.protocol != PROTOCOL
        || policy.server_origin != identity.origin()
        || validation::origin(&policy.server_origin).as_deref() != Ok(policy.server_origin.as_str())
        || policy.relay_urls.len() > MAX_RELAYS
        || policy.sync_interval_ms == 0
        || policy.authorization_lease_ms == 0
        || policy.authorization_lease_ms > MAX_AUTHORIZATION_LEASE_MS
        || policy.presence_ttl_ms < policy.authorization_lease_ms
        || policy.presence_ttl_ms > MAX_PRESENCE_TTL_MS
        || !(1..=64).contains(&policy.max_records)
    {
        return Err(invalid_response());
    }
    let mut relays = HashSet::with_capacity(policy.relay_urls.len());
    for relay in &policy.relay_urls {
        if !relays.insert(relay) || !valid_relay_url(relay, identity.origin().starts_with("http:"))
        {
            return Err(invalid_response());
        }
    }
    Ok(())
}

fn validate_peer(peer: &Peer, identity: &Identity) -> Result<()> {
    if uuid::Uuid::parse_str(&peer.device_id).is_err()
        || peer.device_id == identity.session().device_id
        || peer.device_name.is_empty()
        || peer.device_name.len() > MAX_DEVICE_NAME_BYTES
        || peer
            .device_name
            .bytes()
            .any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return Err(invalid_response());
    }
    let key = validation::public_key(&peer.public_key)?;
    if peer.public_key == identity.session().public_key {
        return Err(invalid_response());
    }
    if let Some(endpoint) = &peer.endpoint {
        if endpoint.endpoint_id != hex(key.as_bytes())
            || endpoint.addresses.len() > MAX_ENDPOINT_ADDRESSES
        {
            return Err(invalid_response());
        }
        for address in &endpoint.addresses {
            let ip = address
                .ip
                .parse::<IpAddr>()
                .map_err(|_| invalid_response())?;
            if address.port == 0 || ip.is_unspecified() || ip.is_multicast() {
                return Err(invalid_response());
            }
            if matches!(ip, IpAddr::V4(value) if value.is_broadcast()) || address.ip.contains('%') {
                return Err(invalid_response());
            }
        }
        if let Some(relay) = &endpoint.relay_url {
            if !valid_relay_url(relay, identity.origin().starts_with("http:")) {
                return Err(invalid_response());
            }
        }
    }
    Ok(())
}

fn validate_lease(value: u64, identity: &Identity, now: u64) -> Result<()> {
    if value <= now
        || value > identity.session().expires_at_ms
        || value - now > MAX_AUTHORIZATION_LEASE_MS
    {
        return Err(invalid_response());
    }
    Ok(())
}

fn validate_shape(width: u32, height: u32, byte_length: usize) -> Result<()> {
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16_777_216
        || byte_length == 0
        || byte_length > MAX_PNG_BYTES
    {
        return Err(invalid_response());
    }
    Ok(())
}

fn valid_relay_url(value: &str, allow_http: bool) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    value.len() <= 256
        && value.is_ascii()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && value == format!("{}/", url.origin().ascii_serialization())
        && (url.scheme() == "https" || (allow_http && url.scheme() == "http"))
}

fn is_participant(record: &Record, device_id: &str) -> bool {
    record.envelope.source.device_id == device_id
        || record
            .receiver
            .as_ref()
            .is_some_and(|receiver| receiver.device_id == device_id)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn known_error_code(value: &str) -> Option<&'static str> {
    Some(match value {
        "device_session_unavailable" => "device_session_unavailable",
        "device_clock_skew" => "device_clock_skew",
        "device_proof_replayed" => "device_proof_replayed",
        "invalid_device_proof" => "invalid_device_proof",
        "projection_not_configured" => "projection_not_configured",
        "projection_invalid_request" => "projection_invalid_request",
        "projection_invalid_invitation" => "projection_invalid_invitation",
        "projection_invalid_endpoint" => "projection_invalid_endpoint",
        "projection_access_denied" => "projection_access_denied",
        "projection_source_mismatch" => "projection_source_mismatch",
        "projection_invitation_expired" => "projection_invitation_expired",
        "projection_not_found" => "projection_not_found",
        "projection_stopped" => "projection_stopped",
        "projection_conflict" => "projection_conflict",
        "projection_already_linked" => "projection_already_linked",
        "projection_revision_conflict" => "projection_revision_conflict",
        "projection_retry_required" => "projection_retry_required",
        "projection_limit_reached" => "projection_limit_reached",
        "projection_peer_unavailable" => "projection_peer_unavailable",
        _ => return None,
    })
}

fn invalid_response() -> crate::Error {
    error(502, "projection_response_invalid")
}
