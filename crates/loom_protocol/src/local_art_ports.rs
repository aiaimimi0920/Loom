//! 精确包端口证明；不含参数默认值、路径或授权，新签名域不改变旧 package.v1。
use super::{Error, Package as LegacyPackage};
use serde::{Deserialize, Serialize};
pub const PROTOCOL: &str = "loom.art.local.package.v2";
pub const DOMAIN: &[u8] = b"loom.art.local.package.v2\0";
pub const PREPARE: &str = "loom.art.local.prepare-package.v2";
pub const ROUTE: &str = "/v2/art/local/prepare-package";
pub const MAX_PORTS: usize = 32;
pub const MAX_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ports {
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
}
fn text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
impl Ports {
    pub fn validate(&self) -> Result<(), Error> {
        for ports in [&self.inputs, &self.outputs] {
            if ports.len() > MAX_PORTS
                || ports.iter().enumerate().any(|(i, p)| {
                    !text(&p.name, 128)
                        || !text(&p.kind, 64)
                        || ports[..i].iter().any(|other| other.name == p.name)
                })
            {
                return Err(Error::Source);
            }
        }
        if serde_json::to_vec(self).map_err(|_| Error::Source)?.len() > MAX_BYTES {
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
}
impl Package {
    pub fn validate(&self) -> Result<(), Error> {
        if self.protocol_version != PROTOCOL {
            return Err(Error::Version);
        }
        self.package.validate()?;
        self.ports.validate()?;
        // 端口和资源共享原证明预算，不因逐项有界而放大原 envelope/payload 门。
        if serde_json::to_vec(self).map_err(|_| Error::Source)?.len() > super::super::MAX_PAYLOAD {
            return Err(Error::Source);
        }
        Ok(())
    }
}
