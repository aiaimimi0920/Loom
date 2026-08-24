//! Public MCP transport and server configuration models.

use super::*;

/// Transport used to connect to a configured MCP server.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpTransport {
    #[default]
    Stdio,
    StreamableHttp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCredentialRequirement {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerPackageState {
    pub qualified_id: String,
    pub publisher_id: String,
    pub version: String,
    pub digest: String,
    pub package_dir: PathBuf,
    /// SHA-256 of every file extracted at install, keyed by package-relative path with `/`
    /// separators. Checked against the package's `active.json` before a stdio server is spawned.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
    /// What the install-time trust check concluded about this package's signature. Defaults to
    /// `Unsigned`, which is also what a package installed before signatures existed reports.
    #[serde(default)]
    pub trust_status: PackageTrustStatus,
}

impl McpTransport {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable-http",
        }
    }
}

/// User-configured MCP server definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_bindings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_requirements: Vec<McpCredentialRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<McpServerPackageState>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

pub(super) fn default_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SpawnCommandSpec {
    pub(super) program: String,
    pub(super) args: Vec<String>,
}

impl SpawnCommandSpec {
    pub(super) fn direct(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }
}

impl McpServerConfig {
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            command: command.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            transport: McpTransport::Stdio,
            url: String::new(),
            headers: BTreeMap::new(),
            credential_env: BTreeMap::new(),
            credential_headers: BTreeMap::new(),
            credential_bindings: BTreeMap::new(),
            credential_requirements: Vec::new(),
            tools: Vec::new(),
            package: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn remote(id: impl Into<String>, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            transport: McpTransport::StreamableHttp,
            url: url.into(),
            headers: BTreeMap::new(),
            credential_env: BTreeMap::new(),
            credential_headers: BTreeMap::new(),
            credential_bindings: BTreeMap::new(),
            credential_requirements: Vec::new(),
            tools: Vec::new(),
            package: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn validate(&self) -> McpResult<()> {
        validate_server_metadata_and_limits(self)?;
        match self.transport {
            McpTransport::Stdio if self.command.trim().is_empty() => Err(McpError::InvalidConfig(
                "stdio command is required".to_owned(),
            )),
            McpTransport::StreamableHttp => validate_remote_config(self),
            McpTransport::Stdio => validate_stdio_command(&self.command),
        }
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}
