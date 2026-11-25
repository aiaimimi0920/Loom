//! 可重建宿主请求的显式离线许可；独立签名域，不改变一次性grant的绑定语义。
use super::{Error, Grant, Permission};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "loom.art.local.resume.v1";
pub const DOMAIN: &[u8] = b"loom.art.local.resume.v1\0";
pub const PREPARE: &str = "loom.art.local.prepare-resumable.v1";

pub(super) fn valid_device_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    bytes.len() == 44
        && bytes[43] == b'='
        && bytes[..43]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"+/".contains(b))
        && b"AEIMQUYcgkosw048".contains(&bytes[42])
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resume {
    pub protocol_version: String,
    pub device_public_key: String,
    pub grant: Grant,
}
impl Resume {
    pub fn validate(&self) -> Result<(), Error> {
        self.grant.validate()?;
        if self.protocol_version != PROTOCOL {
            return Err(Error::Version);
        }
        if !valid_device_key(&self.device_public_key) {
            return Err(Error::Identity);
        }
        if self.grant.offline_until_ms == 0 {
            return Err(Error::Time);
        }
        if !self.grant.permissions.contains(&Permission::LocalState)
            || !self.grant.permissions.contains(&Permission::Preview)
        {
            return Err(Error::Permission);
        }
        Ok(())
    }
}
