use super::*;
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record {
    pub incoming: bool,
    pub peer_id: String,
    pub trust_revision: u64,
    pub envelope: ProjectionEnvelope,
    pub snapshot: ProjectionSnapshot,
    pub source_key: String,
    pub source_name: String,
    pub source_epoch: u64,
    pub target_id: String,
    pub target_epoch: Option<u64>,
    pub receiver_unit: Option<String>,
    pub revision: u64,
    pub digest: String,
    pub status: DeliveryStatus,
    pub unlinked: bool,
    pub updated_ms: u64,
}

pub(super) fn target_id(peer: &str, device: &str) -> String {
    format!(
        "peer-target:{:x}",
        Sha256::digest(format!("{peer}\n{device}"))
    )
}

impl Record {
    pub fn validate(&self) -> PeerResult<()> {
        verify_projection_signature(&self.envelope, &self.source_key).map_err(convert)?;
        validate_projection_snapshot(&self.snapshot, &self.digest).map_err(convert)?;
        self.validate_metadata()
    }

    // Construction paths validate signatures and PNGs before acquiring shared locks.
    pub fn validate_metadata(&self) -> PeerResult<()> {
        if !loom_protocol::projection::projection_identifier_valid(&self.target_id)
            || self.peer_id.len() != 69
            || !self.peer_id.starts_with("loom-")
            || !self.peer_id[5..].bytes().all(|b| b.is_ascii_hexdigit())
            || self.source_name.is_empty()
            || self.source_name.len() > 1024
            || self.source_name.chars().any(char::is_control)
            || self.revision < self.envelope.source.revision
            || self.revision > loom_protocol::projection::MAX_PROJECTION_REVISION
            || (self.incoming && self.target_epoch.is_none())
            || (matches!(
                self.status,
                DeliveryStatus::Accepted | DeliveryStatus::Displayed
            ) && (self.receiver_unit.is_none() || self.target_epoch.is_none()))
            || self
                .receiver_unit
                .as_ref()
                .is_some_and(|id| !loom_protocol::projection::projection_identifier_valid(id))
            || (self.status == DeliveryStatus::Rejected && !self.unlinked)
        {
            return Err(failure(400, "projection_invalid"));
        }
        Ok(())
    }

    pub fn active(&self) -> PeerResult<()> {
        if self.status == DeliveryStatus::Rejected {
            return Err(failure(410, "projection_rejected"));
        }
        if self.unlinked {
            return Err(failure(410, "projection_unlinked"));
        }
        if self.receiver_unit.is_none() && self.envelope.expires_at_ms <= unix_time_millis() {
            return Err(failure(410, "projection_invitation_expired"));
        }
        Ok(())
    }

    pub fn response(&self, image: bool) -> Value {
        json!({"envelope": self.envelope, "revision": self.revision, "digest": self.digest,
            "width": self.snapshot.width, "height": self.snapshot.height,
            "snapshot": if image {Some(&self.snapshot)} else {None}, "linked": !self.unlinked,
            "receiverDeviceId": self.receiver_unit.as_ref().map(|_| &self.target_id),
            "receiverUnitId": self.receiver_unit, "sourceName": self.source_name,
            "route": "offline_peer", "peerId": self.peer_id,
            "delivery": {"targetDeviceId": if self.incoming {self.target_id.clone()} else {target_id(&self.peer_id, &self.target_id)},
                "status": self.status}})
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Offer {
    pub envelope: ProjectionEnvelope,
    pub snapshot: ProjectionSnapshot,
    pub source_key: String,
    pub source_name: String,
    pub source_epoch: u64,
    pub target_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Create {
    pub envelope: ProjectionEnvelope,
    pub snapshot: ProjectionSnapshot,
    pub target_device_id: String,
    pub peer_id: String,
    pub remote_device_id: String,
}
