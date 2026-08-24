//! Runtime request, response, credential, MCP, and diagnostic envelopes.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::package::PermissionPolicy;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteRequest {
    pub protocol_version: String,
    #[serde(default)]
    pub supported_protocol_versions: Vec<String>,
    pub framework_id: String,
    pub art_id: String,
    pub art_dir: PathBuf,
    pub inputs: Value,
    pub params: Value,
    pub disabled_params: Vec<String>,
    pub context: FrameworkExecutionContext,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecutionContext {
    pub request_id: String,
    pub cache_dir: PathBuf,
    pub temp_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_version: Option<String>,
    #[serde(default)]
    pub granted_permissions: PermissionPolicy,
    #[serde(default)]
    pub credentials: Vec<CredentialGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_server: Option<FrameworkMcpServer>,
}

/// Host-resolved MCP runtime configuration supplied to the MCP framework.
/// Art packages identify a dependency but never own its process or endpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkMcpServer {
    pub id: String,
    pub package_id: String,
    pub version: String,
    pub transport: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub optional_credential_env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub optional_credential_headers: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialGrant {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default)]
    pub output: Value,
    #[serde(default)]
    pub error: Option<FrameworkExecuteError>,
    #[serde(default)]
    pub candidates: Vec<Value>,
    #[serde(default)]
    pub cache: Value,
    #[serde(default)]
    pub events: Vec<FrameworkExecutionEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<ExecutionDiagnostics>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecutionEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionDiagnostics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout_bytes: u64,
    #[serde(default)]
    pub stderr_bytes: u64,
    #[serde(default)]
    pub stdout_truncated: bool,
    #[serde(default)]
    pub stderr_truncated: bool,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default)]
    pub resource_limited: bool,
}
