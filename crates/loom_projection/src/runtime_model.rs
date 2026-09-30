use crate::{error, validation, Envelope, Result, Snapshot, MAX_PNG_BYTES};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalImage {
    pub image_base64: String,
    pub width: u32,
    pub height: u32,
}

impl LocalImage {
    pub(crate) fn snapshot(self, envelope: &Envelope, revision: u64) -> Result<Snapshot> {
        if self.image_base64.len() > MAX_PNG_BYTES.div_ceil(3) * 4 {
            return Err(error(413, "projection_image_budget"));
        }
        let png = STANDARD
            .decode(self.image_base64)
            .map_err(|_| error(400, "projection_invalid_image"))?;
        let snapshot = Snapshot {
            projection_id: envelope.projection_id.clone(),
            source_session_id: envelope.source.session_id.clone(),
            revision,
            digest: format!("{:x}", Sha256::digest(&png)),
            width: self.width,
            height: self.height,
            png,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

impl From<&Snapshot> for LocalImage {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            image_base64: STANDARD.encode(&snapshot.png),
            width: snapshot.width,
            height: snapshot.height,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LocalOperation {
    Context {},
    Create {
        unit_id: String,
        content_kind: String,
        snapshot: LocalImage,
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
    Update {
        projection_id: String,
        source_session_id: String,
        prior_revision: u64,
        revision: u64,
        snapshot: LocalImage,
    },
    Read {
        projection_id: String,
        known_revision: u64,
    },
    Unlink {
        projection_id: String,
    },
}

impl LocalOperation {
    pub fn route(&self) -> &'static str {
        match self {
            Self::Context {} => "context",
            Self::Create { .. } => "create",
            Self::Inspect { .. } => "inspect",
            Self::Accept { .. } => "accept",
            Self::Update { .. } => "update",
            Self::Read { .. } => "read",
            Self::Unlink { .. } => "unlink",
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let valid = match self {
            Self::Create {
                unit_id,
                content_kind,
                ..
            } => {
                validation::identifier(unit_id)
                    && matches!(content_kind.as_str(), "art" | "sticker")
            }
            Self::Accept {
                receiver_unit_id,
                expected_revision,
                expected_digest,
                confirmed,
                ..
            } => {
                *confirmed
                    && validation::identifier(receiver_unit_id)
                    && validation::revision(*expected_revision)
                    && validation::hex(expected_digest, 64)
            }
            Self::Read {
                projection_id,
                known_revision,
            } => validation::projection_id(projection_id) && validation::revision(*known_revision),
            Self::Unlink { projection_id } => validation::projection_id(projection_id),
            Self::Update {
                projection_id,
                source_session_id,
                prior_revision,
                revision,
                ..
            } => {
                validation::projection_id(projection_id)
                    && validation::identifier(source_session_id)
                    && validation::revision(*prior_revision)
                    && validation::revision(*revision)
                    && *revision == prior_revision + 1
            }
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(error(400, "projection_invalid_request"))
        }
    }
}

// Persist PNGs compactly; a JSON array of byte integers would multiply the disk budget.
pub(crate) mod png_base64 {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        bytes: &[u8],
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Vec<u8>, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        if encoded.len() > MAX_PNG_BYTES.div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("PNG budget"));
        }
        STANDARD.decode(encoded).map_err(serde::de::Error::custom)
    }
}
