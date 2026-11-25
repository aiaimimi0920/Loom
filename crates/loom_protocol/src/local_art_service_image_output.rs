//! 显式单PNG结果；旧service.v1的JSON门和控制owner不扩义。
use super::{images, Error, Owner, Reply, Status, Submit};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "loom.art.local.service.image-output.v1";
pub const MAX_PNG: usize = 2 * 1024 * 1024;
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;
pub const MAX_REPLY: usize = super::MAX_REQUEST + 4 * MAX_PNG.div_ceil(3);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_version: String,
    pub call: Submit,
    #[serde(default)]
    pub images: Vec<images::Image>,
}
impl Request {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > images::MAX_REQUEST {
            return Err(Error::Limit);
        }
        let input: Self = serde_json::from_slice(bytes).map_err(|_| Error::Wire)?;
        if input.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        if !input.images.is_empty() {
            images::validate_images(&input.images)?;
        }
        Ok(input)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub digest: String,
    pub png: String,
}
impl Image {
    pub fn validate(&self) -> Result<(), Error> {
        let pixels = u64::from(self.width) * u64::from(self.height) * 4;
        if self.width == 0
            || self.height == 0
            || self.width > 4096
            || self.height > 4096
            || pixels > MAX_PIXELS as u64
            || self.png.len() > 4 * MAX_PNG.div_ceil(3)
            || self.digest.len() != 64
            || !self
                .digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(Error::Limit);
        }
        images::validate_images(&[images::Image {
            png: self.png.clone(),
        }])?;
        let padding = self.png.bytes().rev().take_while(|b| *b == b'=').count();
        if self.png.len() / 4 * 3 - padding > MAX_PNG {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Response {
    pub protocol_version: String,
    pub reply: Reply,
    pub image: Option<Image>,
}
impl Response {
    pub fn validate(&self, owner: &Owner) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        self.reply.validate(owner)?;
        if (self.reply.status == Status::Succeeded) != self.image.is_some() {
            return Err(Error::State);
        }
        if let Some(image) = &self.image {
            image.validate()?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_output_contract_is_explicit_bounded_and_rejects_foreign_fields() {
        let mut image = Image {
            width: 1,
            height: 1,
            digest: "a".repeat(64),
            png: "AQID".into(),
        };
        assert!(image.validate().is_ok());
        image.width = 8193;
        assert_eq!(image.validate(), Err(Error::Limit));
        image.width = 4096;
        image.height = 4096;
        assert_eq!(image.validate(), Err(Error::Limit));
        image.width = 1;
        image.height = 1;
        image.png = "A".repeat(4 * MAX_PNG.div_ceil(3));
        assert_eq!(image.validate(), Err(Error::Limit));
        assert!(Request::parse(br#"{"protocolVersion":"x","call":{},"path":"secret"}"#).is_err());
        assert!(serde_json::from_str::<Image>(
            r#"{"width":1,"height":1,"digest":"a","png":"AQID","path":"x"}"#
        )
        .is_err());
    }
}
