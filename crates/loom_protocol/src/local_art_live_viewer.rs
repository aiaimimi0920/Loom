//! Explicit native-host viewer join; Preview proves a host, never script networking or source control.
use super::{Binding, Envelope, Grant, Permission, MAX_ENVELOPE};
use crate::{LiveControlEnvelope, LiveControlMessage};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "loom.art.local.live-viewer.v1";
pub const MAX_REQUEST: usize = 32 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attach {
    pub protocol_version: String,
    pub binding: Binding,
    pub grant: Envelope,
    pub envelope: LiveControlEnvelope,
    pub require_existing_membership: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Wire,
    Limit,
    Identity,
    Time,
    Permission,
}
impl Attach {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_REQUEST {
            return Err(Error::Limit);
        }
        let request: Self = serde_json::from_slice(bytes).map_err(|_| Error::Wire)?;
        if request.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        if serde_json::to_vec(&request.grant)
            .map_err(|_| Error::Wire)?
            .len()
            > MAX_ENVELOPE
        {
            return Err(Error::Limit);
        }
        Ok(request)
    }
    /// Signature/current approval/package/device remain daemon responsibilities, not inferred here.
    pub fn validate(&self, grant: &Grant, actor: &str, now: u64) -> Result<(), Error> {
        validate_host(&self.binding, grant, actor, Some(now))?;
        if self.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        crate::validate_control_envelope(&self.envelope).map_err(|_| Error::Wire)?;
        match &self.envelope.message {
            LiveControlMessage::SessionAck(ack)
                if ack.accepted && ack.responder_device_id == actor =>
            {
                Ok(())
            }
            _ => Err(Error::Identity),
        }
    }
}

/// Preview proves the exact native host, not script control. Only an explicit host operation
/// may use this proof; release-only reduction can outlive approval without creating authority.
pub(super) fn validate_host(
    binding: &Binding,
    grant: &Grant,
    actor: &str,
    now: Option<u64>,
) -> Result<(), Error> {
    grant.validate().map_err(|_| Error::Wire)?;
    if binding != &grant.binding || binding.device_id != actor {
        return Err(Error::Identity);
    }
    if now.is_some_and(|n| n < grant.issued_at_ms || n >= grant.expires_at_ms) {
        return Err(Error::Time);
    }
    if !grant.permissions.contains(&Permission::Preview) {
        return Err(Error::Permission);
    }
    Ok(())
}

#[cfg(test)]
#[path = "local_art_live_viewer_tests.rs"]
mod tests;
