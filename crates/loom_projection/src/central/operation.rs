use crate::{EndpointAddress, Envelope};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CentralOperation {
    Configuration,
    Create {
        envelope: Envelope,
        width: u32,
        height: u32,
        byte_length: usize,
    },
    Inspect {
        envelope: Envelope,
    },
    Accept {
        envelope: Envelope,
        receiver_unit_id: String,
        expected_revision: u64,
        expected_digest: String,
        confirmed: bool,
    },
    Publish {
        projection_id: String,
        source_session_id: String,
        prior_revision: u64,
        revision: u64,
        digest: String,
        width: u32,
        height: u32,
        byte_length: usize,
    },
    Read {
        projection_id: String,
    },
    Unlink {
        projection_id: String,
    },
    Sync {
        endpoint: EndpointAddress,
    },
    Peer {
        projection_id: String,
        peer_device_id: String,
        peer_public_key: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
}

impl CentralOperation {
    pub(crate) fn envelope(&self) -> Option<&Envelope> {
        match self {
            Self::Create { envelope, .. }
            | Self::Inspect { envelope }
            | Self::Accept { envelope, .. } => Some(envelope),
            Self::Peer { envelope, .. } => envelope.as_ref(),
            _ => None,
        }
    }
}
