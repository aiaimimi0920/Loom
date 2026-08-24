//! Framework installation state and public dependency models.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FrameworkInstallationState {
    pub version: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FrameworkActivationState {
    pub active: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FrameworkLifecycleJournal {
    pub(super) old_activation: Option<FrameworkActivationState>,
    pub(super) next_activation: FrameworkActivationState,
    pub(super) target: String,
    /// Whether `target` was created by the operation this journal describes.
    ///
    /// Recovery may only delete a version directory the interrupted operation itself put on disk.
    /// An install that reuses an already-present directory, and a rollback that activates an older
    /// version, both name a directory that predates the operation; deleting it would destroy the
    /// very version recovery is supposed to restore. Journals written by an older build lack the
    /// field, so the default is `false`: never delete.
    #[serde(default)]
    pub(super) created_target: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkStatus {
    pub id: String,
    pub qualified_id: String,
    pub name: String,
    pub description: String,
    /// Whether the user has installed/enabled this framework.
    pub installed: bool,
    /// Whether an installed framework package is enabled for execution.
    pub enabled: bool,
    /// Whether the framework's runtime is actually available (probed).
    pub ready: bool,
    pub ready_detail: String,
    /// Version read from the installed package manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Directory containing the installed package, when installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<PublisherIdentity>,
    #[serde(default)]
    pub permission_policy: PermissionPolicy,
    #[serde(default)]
    pub declared_permissions: Vec<String>,
    #[serde(default)]
    pub resources: ResourceLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring_schema: Option<FrameworkAuthoringSchema>,
    #[serde(default)]
    pub trust_status: PackageTrustStatus,
}
/// The framework id that an execution belongs to (same mapping as
/// `execution_type_name`, exposed for readiness checks).
pub fn framework_id_for_execution(execution: &ToolExecution) -> &str {
    match execution {
        ToolExecution::CloudApi { .. } => "cloud_api",
        ToolExecution::Mcp { .. } => "mcp",
        ToolExecution::Workflow { .. } => "workflow",
        ToolExecution::FrameworkArt { framework } => framework,
    }
}

/// A third-party binary an art needs (installed in phase 1 to the art dir).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtBinary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// An art's dependency manifest, carried under `metadata.dependencies`. The
/// `framework` field defaults to the execution-derived framework when absent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtDependencies {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binaries: Vec<ArtBinary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<ArtMcpServerDependency>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtMcpServerDependency {
    pub id: String,
    pub version: String,
}

/// Read an art's dependency manifest from `metadata.dependencies`, defaulting
/// `framework` to the one derived from its execution kind.
pub fn read_dependencies(tool: &ToolDefinition) -> ArtDependencies {
    let mut deps: ArtDependencies = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("dependencies"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    if deps.framework.is_none() {
        deps.framework = Some(framework_id_for_execution(&tool.execution).to_owned());
    }
    deps
}
