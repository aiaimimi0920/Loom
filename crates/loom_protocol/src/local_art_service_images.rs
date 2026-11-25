//! 独立图像输入请求；旧service.v1读写者保持严格，控制和JSON结果仍沿原owner合同。
use super::{Error, Grant, Submit};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "loom.art.local.service.images.v1";
pub const MAX_IMAGES: usize = 4;
pub const MAX_PNG: usize = 4 * 1024 * 1024;
pub const TOTAL_PNG: usize = 8 * 1024 * 1024;
pub const MAX_REQUEST: usize = super::MAX_REQUEST + 4 * (TOTAL_PNG + 2 * MAX_IMAGES).div_ceil(3);
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub png: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitImages {
    pub protocol_version: String,
    pub call: Submit,
    pub images: Vec<Image>,
}
impl SubmitImages {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_REQUEST {
            return Err(Error::Limit);
        }
        serde_json::from_slice(bytes).map_err(|_| Error::Wire)
    }
    pub fn validate(&self, grant: &Grant, actor: &str, now: u64) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Wire);
        }
        self.call.validate(grant, actor, now)?;
        validate_images(&self.images)
    }
}
pub fn validate_images(images: &[Image]) -> Result<(), Error> {
    if images.is_empty() || images.len() > MAX_IMAGES {
        return Err(Error::Limit);
    }
    let mut total = 0usize;
    for image in images {
        let text = image.png.as_bytes();
        if text.is_empty() || text.len() % 4 != 0 || text.len() > 4 * MAX_PNG.div_ceil(3) {
            return Err(Error::Limit);
        }
        let padding = text.iter().rev().take_while(|v| **v == b'=').count();
        if padding > 2
            || !text[..text.len() - padding]
                .iter()
                .all(|v| v.is_ascii_alphanumeric() || b"+/".contains(v))
        {
            return Err(Error::Wire);
        }
        let bytes = text.len() / 4 * 3 - padding;
        if bytes == 0 || bytes > MAX_PNG {
            return Err(Error::Limit);
        }
        total = total.checked_add(bytes).ok_or(Error::Limit)?;
        if total > TOTAL_PNG {
            return Err(Error::Limit);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_envelope_is_strict_and_bounded_before_decode() {
        assert_eq!(validate_images(&[]), Err(Error::Limit));
        assert!(validate_images(&[Image { png: "AQID".into() }]).is_ok());
        for png in [
            "=QID",
            "AQ=D",
            "AQID=",
            "data:image/png;base64,AQID",
            "====",
        ] {
            assert!(
                validate_images(&[Image { png: png.into() }]).is_err(),
                "{png}"
            );
        }
        assert_eq!(
            validate_images(&[Image {
                png: "A".repeat(4 * MAX_PNG.div_ceil(3))
            }]),
            Err(Error::Limit)
        );
        assert!(SubmitImages::parse(
            br#"{"protocolVersion":"x","call":{},"images":[],"path":"c:/secret"}"#
        )
        .is_err());
        assert_eq!(
            SubmitImages::parse(&vec![b' '; MAX_REQUEST + 1]).unwrap_err(),
            Error::Limit
        );
    }
}
