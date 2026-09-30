use crate::{Envelope, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    pub protocol: String,
    pub server_origin: String,
    pub relay_urls: Vec<String>,
    pub sync_interval_ms: u64,
    pub authorization_lease_ms: u64,
    pub presence_ttl_ms: u64,
    pub max_records: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Address {
    pub ip: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointAddress {
    pub endpoint_id: String,
    pub addresses: Vec<Address>,
    pub relay_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageMetadata {
    pub revision: u64,
    pub digest: String,
    pub width: u32,
    pub height: u32,
    pub byte_length: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InitialImage {
    pub width: u32,
    pub height: u32,
    pub byte_length: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receiver {
    pub device_id: String,
    pub unit_id: String,
    pub revision: u64,
    pub digest: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Invited,
    Linked,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub envelope: Envelope,
    pub initial_image: InitialImage,
    pub revision: u64,
    pub digest: String,
    pub width: u32,
    pub height: u32,
    pub byte_length: usize,
    pub status: Status,
    pub receiver: Option<Receiver>,
    pub expires_at_ms: u64,
    pub updated_at_ms: u64,
}

impl Record {
    pub fn image_metadata(&self) -> ImageMetadata {
        ImageMetadata {
            revision: self.revision,
            digest: self.digest.clone(),
            width: self.width,
            height: self.height,
            byte_length: self.byte_length,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Peer {
    pub device_id: String,
    pub public_key: String,
    pub device_name: String,
    pub endpoint: Option<EndpointAddress>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct View {
    pub record: Record,
    pub available: bool,
    pub authorized_until_ms: u64,
    pub peer: Option<Peer>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CentralResponse {
    Configuration {
        policy: Policy,
    },
    Projection {
        view: View,
    },
    Sync {
        views: Vec<View>,
    },
    Peer {
        peer: Peer,
        authorized_until_ms: u64,
    },
}

impl ImageMetadata {
    pub fn validate(&self) -> Result<()> {
        crate::validation::image_metadata(self)
    }
}
