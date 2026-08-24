//! Content-addressed resources, transports, streams, and typed port values.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceResourceKind {
    Image,
    Audio,
    Video,
    File,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceResourceDescriptor {
    pub resource_id: String,
    pub kind: SurfaceResourceKind,
    pub mime: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceResourceTransportKind {
    SharedMemory,
    LoomResource,
    Stream,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceResourceTransport {
    pub kind: SurfaceResourceTransportKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceResourceLease {
    pub lease_id: String,
    pub resource: SurfaceResourceDescriptor,
    pub transport: SurfaceResourceTransport,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStreamDescriptor {
    pub stream_id: String,
    pub item_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default)]
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfacePortValue {
    Value { value: Value },
    Resource { resource: SurfaceResourceDescriptor },
    Stream { stream: SurfaceStreamDescriptor },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfacePortKind {
    Boolean,
    Integer,
    Number,
    String,
    Enum,
    Object,
    List,
    Table,
    Image,
    Audio,
    Video,
    File,
    Binary,
    Stream,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfacePortDefinition {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub port_type: SurfacePortKind,
    #[serde(default)]
    pub schema: Value,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub required: bool,
}
