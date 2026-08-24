//! Declarative scene nodes, snapshots, patches, and input events.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::default_surface_protocol_version;
use super::manifest::SurfaceRuntimeKind;
use super::resources::{SurfaceResourceDescriptor, SurfaceResourceLease};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub props: Value,
    #[serde(default)]
    pub layout: Value,
    #[serde(default)]
    pub style: Value,
    #[serde(default)]
    pub accessibility: Value,
    #[serde(default)]
    pub events: BTreeMap<String, String>,
    #[serde(default)]
    pub children: Vec<SurfaceNode>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSnapshot {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub art_id: String,
    pub art_version: String,
    pub revision: u64,
    #[serde(default)]
    pub runtime: SurfaceRuntimeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_id: Option<String>,
    pub scene: SurfaceNode,
    #[serde(default)]
    pub authoritative_state: Value,
    #[serde(default)]
    pub resources: Vec<SurfaceResourceDescriptor>,
    #[serde(default)]
    pub resource_leases: Vec<SurfaceResourceLease>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SurfacePatchOperation {
    Set {
        node_id: String,
        path: String,
        value: Value,
    },
    Remove {
        node_id: String,
        path: String,
    },
    InsertNode {
        parent_id: String,
        index: usize,
        node: SurfaceNode,
    },
    RemoveNode {
        node_id: String,
    },
    MoveNode {
        node_id: String,
        parent_id: String,
        index: usize,
    },
    ReplaceNode {
        node_id: String,
        node: SurfaceNode,
    },
    SetVisibility {
        node_id: String,
        visible: bool,
    },
    SetBinding {
        node_id: String,
        path: String,
        binding: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfacePatch {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub base_revision: u64,
    pub revision: u64,
    #[serde(default)]
    pub operations: Vec<SurfacePatchOperation>,
    #[serde(default)]
    pub state_patch: Value,
    #[serde(default)]
    pub resources: Vec<SurfaceResourceDescriptor>,
    #[serde(default)]
    pub resource_leases: Vec<SurfaceResourceLease>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceEventClass {
    Discrete,
    Continuous,
    Commit,
    Local,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceEvent {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub instance_id: String,
    pub attachment_id: String,
    pub event_id: String,
    pub node_id: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    pub class: SurfaceEventClass,
    pub generation: u64,
    pub base_revision: u64,
    #[serde(default)]
    pub payload: Value,
}
