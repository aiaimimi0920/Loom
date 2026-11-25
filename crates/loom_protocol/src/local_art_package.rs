//! 包资源证明独立于旧grant/resume；只有显式设备公钥才委托同期限内的新宿主绑定。
use super::{digest, Error, Grant};
use serde::{Deserialize, Serialize};
#[path = "local_art_ports.rs"]
pub mod ports;
#[path = "local_art_visibility.rs"]
pub mod visibility;
pub const PROTOCOL: &str = "loom.art.local.package.v1";
pub const DOMAIN: &[u8] = b"loom.art.local.package.v1\0";
pub const PREPARE: &str = "loom.art.local.prepare-package.v1";
pub const RESOURCE: &str = "loom.art.local.resource.v1";
pub const MAX_RESOURCES: usize = 16;
pub const MAX_PNG: u64 = 4 * 1024 * 1024;
pub const MAX_INLINE: u64 = 256 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub mime_type: String,
    pub size: u64,
    pub digest: String,
    pub dimensions: [u32; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Package {
    pub protocol_version: String,
    pub grant: Grant,
    pub device_public_key: String,
    pub resources: Vec<Resource>,
}
pub fn validate_resources(resources: &[Resource]) -> Result<(), Error> {
    if resources.len() > MAX_RESOURCES {
        return Err(Error::Source);
    }
    let (mut png, mut pixels, mut inline, mut images) = (0u64, 0u64, 2u64, 0);
    for (i, r) in resources.iter().enumerate() {
        if r.id.is_empty()
            || r.id.len() > 96
            || !r
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || resources[..i].iter().any(|v| v.id == r.id)
            || !digest(&r.digest)
            || r.size == 0
        {
            return Err(Error::Source);
        }
        if r.mime_type == "image/png" {
            let [w, h] = r.dimensions;
            if w == 0 || h == 0 || r.size > MAX_PNG {
                return Err(Error::Source);
            }
            pixels = u64::from(w)
                .checked_mul(u64::from(h))
                .and_then(|n| n.checked_mul(4))
                .and_then(|n| pixels.checked_add(n))
                .ok_or(Error::Source)?;
            png += r.size;
            images += 1;
        } else {
            if !matches!(
                r.mime_type.as_str(),
                "text/plain" | "application/json" | "application/octet-stream"
            ) || r.dimensions != [0, 0]
                || r.size > MAX_INLINE
            {
                return Err(Error::Source);
            }
            inline += 4 * r.size.div_ceil(3) + r.id.len() as u64 + r.mime_type.len() as u64 + 64;
        }
        if images > 8 || png > 8 * 1024 * 1024 || pixels > 16 * 1024 * 1024 || inline > 512 * 1024 {
            return Err(Error::Source);
        }
    }
    Ok(())
}
impl Package {
    pub fn validate(&self) -> Result<(), Error> {
        self.grant.validate()?;
        if self.protocol_version != PROTOCOL {
            return Err(Error::Version);
        }
        if !self.device_public_key.is_empty()
            && (!super::resume::valid_device_key(&self.device_public_key)
                || self.grant.offline_until_ms == 0)
        {
            return Err(Error::Identity);
        }
        validate_resources(&self.resources)
    }
}
#[cfg(test)]
#[path = "local_art_package_tests.rs"]
mod tests;
