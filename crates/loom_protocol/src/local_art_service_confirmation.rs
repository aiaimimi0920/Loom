//! Explicit promptable service call; requesting a prompt is not approval, and confirmation challenges are never mixed into untrusted runtime progress.
use super::{image_output, images, Control, Error, Owner, Reply, Status, Submit};
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "loom.art.local.service.confirmation.v1";
pub const MAX_REPLY: usize = image_output::MAX_REPLY + 4096;
#[cfg(test)]
#[path = "local_art_service_confirmation_tests.rs"]
mod tests;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_version: String,
    pub call: Submit,
    #[serde(default)]
    pub images: Vec<images::Image>,
    pub image_output: bool,
}
impl Request {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > images::MAX_REQUEST {
            return Err(Error::Limit);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| Error::Wire)?;
        if value.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        if !value.images.is_empty() {
            images::validate_images(&value.images)?;
        }
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Challenge {
    pub nonce: String,
    pub fingerprint: String,
    pub plugin_id: String,
    pub command_id: String,
    pub package_digest: String,
    pub scope_id: String,
    pub expires_at_ms: u64,
}
impl Challenge {
    pub fn validate(&self) -> Result<(), Error> {
        let text = |value: &str| {
            !value.is_empty()
                && value.len() <= 256
                && value
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || b"._:-/".contains(&v))
        };
        let hash = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        };
        if !text(&self.nonce)
            || !text(&self.plugin_id)
            || !text(&self.command_id)
            || !text(&self.scope_id)
            || !hash(&self.fingerprint)
            || !hash(&self.package_digest)
            || self.expires_at_ms == 0
        {
            return Err(Error::Identity);
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub challenge: Challenge,
    pub approve: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestControl {
    pub protocol_version: String,
    pub control: Control,
    pub decision: Option<Decision>,
}
impl RequestControl {
    pub fn parse(bytes: &[u8], decision: bool) -> Result<Self, Error> {
        if bytes.len() > super::MAX_REQUEST {
            return Err(Error::Limit);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| Error::Wire)?;
        if value.protocol_version != PROTOCOL || value.decision.is_some() != decision {
            return Err(Error::Wire);
        }
        if let Some(value) = &value.decision {
            value.challenge.validate()?;
        }
        Ok(value)
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Response {
    pub protocol_version: String,
    pub reply: Reply,
    pub image: Option<image_output::Image>,
    pub confirmation: Option<Challenge>,
}
impl Response {
    pub fn validate(&self, owner: &Owner, image_output: bool) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        self.reply.validate(owner)?;
        if (image_output && self.reply.status == Status::Succeeded) != self.image.is_some() {
            return Err(Error::State);
        }
        if let Some(image) = &self.image {
            image.validate()?;
        }
        if let Some(challenge) = &self.confirmation {
            challenge.validate()?;
            if !matches!(self.reply.status, Status::Accepted | Status::Running) {
                return Err(Error::State);
            }
        }
        Ok(())
    }
}
