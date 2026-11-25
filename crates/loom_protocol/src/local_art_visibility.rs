//! v3 默认显隐与原端口同域签名；数组逐方向按端口索引对应，不赋予执行权限。
use super::{ports::Ports, Package as LegacyPackage};
use crate::local_art::Error;
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "loom.art.local.package.v3";
pub const DOMAIN: &[u8] = b"loom.art.local.package.v3\0";
pub const PREPARE: &str = "loom.art.local.prepare-package.v3";
pub const ROUTE: &str = "/v3/art/local/prepare-package";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visibility {
    pub inputs: Vec<bool>,
    pub outputs: Vec<bool>,
}
impl Visibility {
    pub fn validate(&self, ports: &Ports) -> Result<(), Error> {
        ports.validate()?;
        if self.inputs.len() != ports.inputs.len() || self.outputs.len() != ports.outputs.len() {
            return Err(Error::Source);
        }
        if serde_json::to_vec(&(ports, self))
            .map_err(|_| Error::Source)?
            .len()
            > super::ports::MAX_BYTES
        {
            return Err(Error::Source);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Package {
    pub protocol_version: String,
    pub package: LegacyPackage,
    pub ports: Ports,
    pub visibility: Visibility,
}
impl Package {
    pub fn validate(&self) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Version);
        }
        self.package.validate()?;
        self.visibility.validate(&self.ports)?;
        if serde_json::to_vec(self).map_err(|_| Error::Source)?.len()
            > crate::local_art::MAX_PAYLOAD
        {
            return Err(Error::Source);
        }
        Ok(())
    }
}
