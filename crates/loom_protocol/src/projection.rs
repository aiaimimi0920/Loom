//! Signed invitations identify a source; only a paired, confirmed device may read its raster.
use serde::{Deserialize, Serialize};

#[path = "projection_origin.rs"]
mod origin;
pub use origin::projection_origin_valid;

pub const QR_PROJECTION_PROTOCOL: &str = "neuro.qr-projection.v1";
pub const PROJECTION_INVITE_TTL_MS: u64 = 300_000;
pub const MAX_PROJECTION_IMAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PROJECTION_HTTP_BYTES: usize = 6 * 1024 * 1024;
pub const MAX_PROJECTION_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionSource {
    pub device_id: String,
    pub session_id: String,
    pub unit_id: String,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionKind {
    Sticker,
    Art,
}

impl ProjectionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sticker => "sticker",
            Self::Art => "art",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionContent {
    pub kind: ProjectionKind,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionEnvelope {
    pub protocol: String,
    pub projection_id: String,
    pub server_origin: String,
    pub source: ProjectionSource,
    pub content: ProjectionContent,
    pub expires_at_ms: u64,
    pub nonce: String,
    pub signature: ProjectionSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionSnapshot {
    pub image_base64: String,
    pub width: u32,
    pub height: u32,
}

impl ProjectionEnvelope {
    pub fn signature_message(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.protocol,
            self.projection_id,
            self.server_origin,
            self.source.device_id,
            self.source.session_id,
            self.source.unit_id,
            self.source.revision,
            self.content.kind.as_str(),
            self.content.digest,
            self.expires_at_ms,
            self.nonce
        )
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.protocol != QR_PROJECTION_PROTOCOL
            || !projection_id_valid(&self.projection_id)
            || !projection_origin_valid(&self.server_origin)
            || !projection_identifier_valid(&self.source.device_id)
            || !projection_identifier_valid(&self.source.session_id)
            || !projection_identifier_valid(&self.source.unit_id)
            || self.source.revision == 0
            || self.source.revision > MAX_PROJECTION_REVISION
            || !projection_digest_valid(&self.content.digest)
            || self.expires_at_ms == 0
            || self.expires_at_ms > MAX_PROJECTION_REVISION
            || self.nonce.len() != 32
            || !self.nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.signature.algorithm != "ed25519"
            || self.signature.key_id != self.source.device_id
            || self.signature.value.len() != 86
            || !self
                .signature
                .value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return Err("projection envelope is invalid");
        }
        Ok(())
    }
}

pub fn projection_identifier_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}

pub fn projection_id_valid(value: &str) -> bool {
    value
        .strip_prefix("projection:")
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

pub fn projection_digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
