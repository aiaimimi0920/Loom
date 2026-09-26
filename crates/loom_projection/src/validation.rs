use crate::{error, ImageMetadata, Result, MAX_PNG_BYTES, MAX_REVISION};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::VerifyingKey;

pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}
pub(crate) fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn projection_id(value: &str) -> bool {
    value
        .strip_prefix("projection:")
        .is_some_and(|id| hex(id, 32))
}
pub(crate) fn revision(value: u64) -> bool {
    (1..=MAX_REVISION).contains(&value)
}

pub(crate) fn public_key(encoded: &str) -> Result<VerifyingKey> {
    let invalid = || error(400, "projection_invalid_key");
    let bytes = STANDARD.decode(encoded).map_err(|_| invalid())?;
    if STANDARD.encode(&bytes) != encoded {
        return Err(invalid());
    }
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| invalid())?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| invalid())
}

pub(crate) fn origin(value: &str) -> Result<String> {
    let invalid = || error(400, "projection_invalid_origin");
    let url = reqwest::Url::parse(value).map_err(|_| invalid())?;
    if value.len() > 256
        || !value.is_ascii()
        || value != url.origin().ascii_serialization()
        || (url.scheme() != "https"
            && !(url.scheme() == "http"
                && url
                    .host_str()
                    .is_some_and(loom_security::network::host_is_loopback_literal)))
    {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

pub(crate) fn image_metadata(image: &ImageMetadata) -> Result<()> {
    if !revision(image.revision)
        || !hex(&image.digest, 64)
        || image.width == 0
        || image.height == 0
        || image.width > 8192
        || image.height > 8192
        || u64::from(image.width) * u64::from(image.height) > 16_777_216
        || image.byte_length == 0
        || image.byte_length > MAX_PNG_BYTES
    {
        return Err(error(400, "projection_invalid_image"));
    }
    Ok(())
}
