use crate::{error, validation, Result, PROTOCOL};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub device_id: String,
    pub account_id: String,
    pub public_key: String,
    pub session_id: String,
    pub unit_id: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub kind: String,
    pub digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvelopeSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Envelope {
    pub protocol: String,
    pub projection_id: String,
    pub server_origin: String,
    pub source: Source,
    pub content: Content,
    pub expires_at_ms: u64,
    pub nonce: String,
    pub signature: EnvelopeSignature,
}

impl Envelope {
    pub fn signature_message(&self) -> String {
        let source = &self.source;
        [
            self.protocol.as_str(),
            self.projection_id.as_str(),
            self.server_origin.as_str(),
            source.device_id.as_str(),
            source.account_id.as_str(),
            source.public_key.as_str(),
            source.session_id.as_str(),
            source.unit_id.as_str(),
            &source.revision.to_string(),
            self.content.kind.as_str(),
            self.content.digest.as_str(),
            &self.expires_at_ms.to_string(),
            self.nonce.as_str(),
            self.signature.algorithm.as_str(),
            self.signature.key_id.as_str(),
        ]
        .join("\n")
    }

    pub fn validate(&self, trusted_origin: &str) -> Result<()> {
        let invalid = || error(400, "projection_invalid_invitation");
        let source = &self.source;
        if self.protocol != PROTOCOL
            || self.server_origin != trusted_origin
            || validation::origin(trusted_origin).as_deref() != Ok(trusted_origin)
            || !validation::projection_id(&self.projection_id)
            || uuid::Uuid::parse_str(&source.device_id).is_err()
            || !validation::identifier(&source.account_id)
            || !validation::identifier(&source.session_id)
            || !validation::identifier(&source.unit_id)
            || source.revision != 1
            || !matches!(self.content.kind.as_str(), "sticker" | "art")
            || !validation::hex(&self.content.digest, 64)
            || !validation::revision(self.expires_at_ms)
            || !validation::hex(&self.nonce, 32)
            || self.signature.algorithm != "ed25519"
            || self.signature.key_id != source.device_id
        {
            return Err(invalid());
        }
        let key = validation::public_key(&source.public_key).map_err(|_| invalid())?;
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.signature.value)
            .map_err(|_| invalid())?;
        if URL_SAFE_NO_PAD.encode(&bytes) != self.signature.value {
            return Err(invalid());
        }
        let signature = Signature::from_slice(&bytes).map_err(|_| invalid())?;
        key.verify_strict(self.signature_message().as_bytes(), &signature)
            .map_err(|_| invalid())
    }
}
