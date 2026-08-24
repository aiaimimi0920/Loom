// Manifest models and serialized execution result contracts.
/// The caller-supplied key that carries a Surface invocation. Reserved by this framework, so it is
/// never forwarded to an MCP server as a tool argument.
const SURFACE_ACTION_KEY: &str = "surfaceAction";

#[derive(Debug, Deserialize)]
struct ArtManifest {
    #[serde(default)]
    metadata: ArtMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct ArtMetadata {
    #[serde(default)]
    mcp: Option<McpArtConfig>,
    #[serde(default)]
    dependencies: ArtDependencies,
}

/// The subset of `metadata.dependencies` this framework needs. Deliberately a local mirror of
/// `loom_tool_registry::framework::ArtDependencies` rather than a dependency on that crate: the
/// runtime host is a framework package that only speaks the framework protocol. If the manifest
/// shape changes, this has to change with it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtDependencies {
    #[serde(default)]
    mcp_servers: Vec<ArtMcpServerDependency>,
}

#[derive(Clone, Debug, Deserialize)]
struct ArtMcpServerDependency {
    id: String,
    version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpArtConfig {
    server_id: String,
    package_id: String,
    version: String,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    arguments: Value,
    #[serde(default)]
    calls: Vec<McpCallConfig>,
    #[serde(default)]
    surface_actions: BTreeMap<String, McpSurfaceActionConfig>,
    #[serde(default)]
    argument_aliases: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpCallConfig {
    id: String,
    tool_name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpSurfaceActionConfig {
    #[serde(default)]
    calls: Option<Vec<String>>,
    #[serde(default)]
    arguments: BTreeMap<String, McpArgumentBinding>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpArgumentBinding {
    from: Vec<String>,
}

#[derive(Debug)]
struct ResolvedCall {
    id: String,
    tool_name: String,
    arguments: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallExecution {
    tool_name: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpExecution {
    server_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    results: BTreeMap<String, McpCallExecution>,
    #[serde(skip_serializing_if = "is_false")]
    skipped: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<McpExecutionWarning>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpExecutionWarning {
    code: String,
    message: String,
    dropped_argument_count: usize,
    dropped_argument_names: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}
