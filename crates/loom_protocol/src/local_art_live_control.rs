//! Trusted native-user operations, separate from read-only join and from Art JS permissions.
use super::live_viewer::{validate_host, Error};
use super::{Binding, Envelope, Grant, MAX_ENVELOPE};
use crate::{LiveControlEnvelope, LiveControlMessage};
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "loom.art.local.live-control.v1";
pub const MAX_REQUEST: usize = 32 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_version: String,
    pub binding: Binding,
    pub grant: Envelope,
    pub operation: Operation,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Operation {
    Acquire {
        session_id: String,
        epoch: u64,
        sequence: u64,
    },
    Release {
        session_id: String,
        epoch: u64,
        sequence: u64,
    },
    Input {
        envelope: Box<LiveControlEnvelope>,
    },
}
impl Request {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_REQUEST {
            return Err(Error::Limit);
        }
        let input: Self = serde_json::from_slice(bytes).map_err(|_| Error::Wire)?;
        if input.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        if serde_json::to_vec(&input.grant)
            .map_err(|_| Error::Wire)?
            .len()
            > MAX_ENVELOPE
        {
            return Err(Error::Limit);
        }
        Ok(input)
    }
    pub fn reducing(&self) -> bool {
        matches!(self.operation, Operation::Release { .. })
    }
    pub fn validate(&self, grant: &Grant, actor: &str, now: u64) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        validate_host(
            &self.binding,
            grant,
            actor,
            (!self.reducing()).then_some(now),
        )?;
        match &self.operation {
            Operation::Acquire {
                session_id,
                epoch,
                sequence,
            }
            | Operation::Release {
                session_id,
                epoch,
                sequence,
            } => {
                crate::validate_live_identifier(session_id, "session_id")
                    .map_err(|_| Error::Wire)?;
                if *epoch == 0
                    || *sequence == 0
                    || *epoch > 9_007_199_254_740_991
                    || *sequence > 9_007_199_254_740_991
                {
                    return Err(Error::Wire);
                }
            }
            Operation::Input { envelope } => {
                crate::validate_control_envelope(envelope).map_err(|_| Error::Wire)?;
                if !matches!(&envelope.message, LiveControlMessage::InputEvent(input) if input.source_device_id == actor)
                {
                    return Err(Error::Identity);
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "local_art_live_control_tests.rs"]
mod tests;
