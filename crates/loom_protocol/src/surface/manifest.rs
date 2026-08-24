//! Surface package, host capability, instance, and attachment metadata.

use serde::{Deserialize, Serialize};

use super::actions::SurfaceActionDefinition;
use super::{default_surface_api_version, default_surface_protocol_version};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceRuntimeKind {
    Declarative,
    Javascript,
    Shader,
    LoomRemote,
}

impl Default for SurfaceRuntimeKind {
    fn default() -> Self {
        Self::Declarative
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceThemeMode {
    Host,
    Custom,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceInstanceMode {
    #[default]
    Independent,
    Shared,
}

impl Default for SurfaceThemeMode {
    fn default() -> Self {
        Self::Host
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceSizeClass {
    Compact,
    Medium,
    Expanded,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceViewDefinition {
    pub id: String,
    pub label: String,
    pub full_size: SurfaceSize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceVariant {
    pub runtime: SurfaceRuntimeKind,
    pub entry: String,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStateMigration {
    pub from: u32,
    pub to: u32,
    pub entry: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfacePackageManifest {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    #[serde(default = "default_surface_api_version")]
    pub api_version: String,
    #[serde(default)]
    pub variants: Vec<SurfaceVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_scene: Option<String>,
    #[serde(default)]
    pub required_nodes: Vec<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub actions: Vec<SurfaceActionDefinition>,
    #[serde(default)]
    pub instance_mode: SurfaceInstanceMode,
    #[serde(default = "default_surface_state_schema_version")]
    pub state_schema_version: u32,
    #[serde(default)]
    pub migrations: Vec<SurfaceStateMigration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_size: Option<SurfaceSize>,
    #[serde(default)]
    pub views: Vec<SurfaceViewDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_view_id: Option<String>,
    #[serde(default)]
    pub theme_mode: SurfaceThemeMode,
}

const fn default_surface_state_schema_version() -> u32 {
    1
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInputCapabilities {
    #[serde(default)]
    pub pointer: bool,
    #[serde(default)]
    pub hover: bool,
    #[serde(default)]
    pub touch: bool,
    #[serde(default)]
    pub keyboard: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceHostCapabilities {
    pub api_version: String,
    #[serde(default)]
    pub runtimes: Vec<SurfaceRuntimeKind>,
    #[serde(default)]
    pub nodes: Vec<String>,
    #[serde(default)]
    pub transports: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub input: SurfaceInputCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceHandshake {
    #[serde(default = "default_surface_protocol_version")]
    pub protocol_version: String,
    pub client_id: String,
    pub client_version: String,
    pub platform: String,
    pub capabilities: SurfaceHostCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceInstancePersistence {
    Temporary,
    Persistent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceInstanceDescriptor {
    pub instance_id: String,
    pub art_id: String,
    pub art_version: String,
    pub package_digest: String,
    #[serde(default)]
    pub instance_mode: SurfaceInstanceMode,
    #[serde(default)]
    pub state_schema_version: u32,
    pub persistence: SurfaceInstancePersistence,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub surface_revision: u64,
    #[serde(default)]
    pub preview_revision: u64,
    #[serde(default)]
    pub result_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceAttachmentDescriptor {
    pub attachment_id: String,
    pub instance_id: String,
    pub hook_node_id: String,
    pub device_id: String,
}
