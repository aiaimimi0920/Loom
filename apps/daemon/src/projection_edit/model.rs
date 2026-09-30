use super::*;

pub(super) const SCHEMA: &str = "neuro.projection-edit.v1";
pub(super) const MAX_SESSION_BYTES: usize = 256 * 1024;
const MAX_REVISION: u64 = loom_protocol::projection::MAX_PROJECTION_REVISION;

/// Authenticated binding context; foreign actors use a peer-scoped identity.
pub(crate) struct Access {
    pub projection_id: String,
    pub source_device_id: String,
    pub source_unit_id: String,
    pub source_epoch: u64,
    pub actor: String,
    pub is_source: bool,
    pub offline: bool,
    pub digest: String,
    pub width: u32,
    pub height: u32,
}
impl Access {
    pub(super) fn source_only(&self) -> EditResult<()> {
        if self.is_source {
            Ok(())
        } else {
            Err(error(403, "projection_source_mismatch"))
        }
    }
}
fn first_revision() -> u64 {
    1
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Mode {
    OneWay,
    TwoWay,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Object {
    pub revision: u64,
    pub value: Option<Value>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub actor: String,
    pub digest: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Basis {
    pub digest: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Session {
    pub schema: String,
    pub session_id: String,
    pub source_device_id: String,
    pub source_unit_id: String,
    pub source_epoch: u64,
    pub initialization_digest: String,
    pub basis: Basis,
    pub bindings: std::collections::BTreeSet<String>,
    #[serde(default)]
    pub offline_bindings: std::collections::BTreeSet<String>,
    pub revision: u64,
    pub mode_revision: u64,
    #[serde(default = "first_revision")]
    pub checkpoint_revision: u64,
    pub mode: Mode,
    pub objects: BTreeMap<String, Object>,
    pub receipts: BTreeMap<String, Receipt>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Change {
    pub object_id: String,
    pub value: Option<Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum Request {
    Initialize {
        projection_id: String,
        session_id: String,
        expected_digest: String,
        objects: BTreeMap<String, Value>,
    },
    Attach {
        projection_id: String,
        session_id: String,
    },
    Read {
        projection_id: String,
    },
    Mode {
        projection_id: String,
        session_id: String,
        op_id: String,
        base_mode_revision: u64,
        mode: Mode,
    },
    Apply {
        projection_id: String,
        session_id: String,
        op_id: String,
        base_revision: u64,
        mode_revision: u64,
        changes: Vec<Change>,
    },
    Checkpoint {
        projection_id: String,
        session_id: String,
        expected_revision: u64,
    },
}

impl Request {
    pub fn projection_id(&self) -> &str {
        match self {
            Self::Initialize { projection_id, .. }
            | Self::Attach { projection_id, .. }
            | Self::Read { projection_id }
            | Self::Mode { projection_id, .. }
            | Self::Apply { projection_id, .. }
            | Self::Checkpoint { projection_id, .. } => projection_id,
        }
    }
}

pub(super) fn identifier(id: &str) -> bool {
    loom_protocol::projection::projection_identifier_valid(id)
}
pub(super) fn session_id_valid(id: &str) -> bool {
    id.strip_prefix("edit:").is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}

// Payloads are opaque Hook annotation documents. Loom never renders or executes them.
pub(super) fn value_valid(id: &str, value: &Value) -> bool {
    identifier(id)
        && value.is_object()
        && value["id"].as_str() == Some(id)
        && value["type"].as_str().is_some_and(|kind| {
            matches!(
                kind,
                "rect"
                    | "round-rect"
                    | "ellipse"
                    | "triangle"
                    | "polygon"
                    | "line"
                    | "polyline"
                    | "arrow"
                    | "text"
                    | "brush"
                    | "highlighter"
                    | "serial"
            )
        })
        && serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 16 * 1024)
}

impl Session {
    pub fn valid(&self) -> bool {
        self.schema == SCHEMA
            && self.basis.digest.len() == 64
            && self.basis.digest.bytes().all(|b| b.is_ascii_hexdigit())
            && self.basis.width > 0
            && self.basis.height > 0
            && self.basis.width <= 8192
            && self.basis.height <= 8192
            && u64::from(self.basis.width) * u64::from(self.basis.height) <= 16_777_216
            && session_id_valid(&self.session_id)
            && self.initialization_digest.len() == 64
            && self
                .initialization_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            && identifier(&self.source_device_id)
            && identifier(&self.source_unit_id)
            && self.source_epoch > 0
            && (1..=MAX_REVISION).contains(&self.revision)
            && (1..=self.revision).contains(&self.mode_revision)
            && (1..=self.mode_revision).contains(&self.checkpoint_revision)
            && !self.bindings.is_empty()
            && self.offline_bindings.is_subset(&self.bindings)
            && self.bindings.len() <= 8
            && self
                .bindings
                .iter()
                .all(|id| loom_protocol::projection::projection_id_valid(id))
            && self.objects.len() <= 256
            && self.receipts.len() <= 256
            && self.objects.iter().all(|(id, object)| {
                identifier(id)
                    && (1..=self.revision).contains(&object.revision)
                    && object
                        .value
                        .as_ref()
                        .is_none_or(|value| value_valid(id, value))
            })
            && self.receipts.iter().all(|(id, receipt)| {
                identifier(id)
                    && identifier(&receipt.actor)
                    && receipt.digest.len() == 64
                    && receipt.digest.bytes().all(|b| b.is_ascii_hexdigit())
            })
    }
    pub fn response(&self) -> Value {
        json!({"schema": self.schema, "sessionId": self.session_id, "basis": self.basis, "revision": self.revision,
            "modeRevision": self.mode_revision, "mode": self.mode, "objects": self.objects,
            "checkpointRevision": self.checkpoint_revision, "receiptCount": self.receipts.len()})
    }
    pub fn next_revision(&self) -> EditResult<u64> {
        self.revision
            .checked_add(1)
            .filter(|revision| *revision <= MAX_REVISION)
            .ok_or_else(|| error(409, "projection_edit_revision_exhausted"))
    }
    pub fn replay(&self, op: &str, actor: &str, digest: &str) -> EditResult<bool> {
        if !identifier(op) {
            return Err(error(400, "projection_edit_invalid_operation"));
        }
        if let Some(receipt) = self.receipts.get(op) {
            if receipt.actor == actor && receipt.digest == digest {
                return Ok(true);
            }
            return Err(error(409, "projection_edit_operation_reused"));
        }
        // Never evict deduplication history and accidentally apply an old operation again.
        if self.receipts.len() >= 256 {
            return Err(error(413, "projection_edit_log_full"));
        }
        Ok(false)
    }
    pub fn receipt(&mut self, op: String, actor: &str, digest: String) {
        self.receipts.insert(
            op,
            Receipt {
                actor: actor.to_owned(),
                digest,
            },
        );
    }
}
