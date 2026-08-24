//! Tool definitions, execution records, and workflow binding models.

use super::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub execution: ToolExecution,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl ToolDefinition {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        execution: ToolExecution,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            enabled: true,
            execution,
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: Vec::new(),
            metadata: None,
        }
    }

    pub fn validate(&self) -> ToolRegistryResult<()> {
        require_non_empty(&self.id, &self.id, "id")?;
        require_no_path_separator(&self.id, &self.id)?;
        require_non_empty(&self.id, &self.name, "name")?;
        if let Some(publisher) = self.publisher_identity() {
            if !is_safe_publisher_id(&publisher.id) {
                return Err(ToolRegistryError::InvalidToolDefinition {
                    id: self.id.clone(),
                    reason: "publisher id must be a safe package namespace".to_owned(),
                });
            }
        }
        if let Some(surface) = self.surface_manifest()? {
            validate_surface_package_manifest(&self.id, &surface)?;
        }
        self.execution.validate(&self.id)
    }

    #[must_use]
    pub fn publisher_identity(&self) -> Option<PublisherIdentity> {
        self.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("packageSecurity"))
            .and_then(|security| security.get("publisher"))
            .and_then(|publisher| serde_json::from_value(publisher.clone()).ok())
    }

    #[must_use]
    pub fn qualified_id(&self) -> String {
        self.publisher_identity()
            .map(|publisher| format!("{}/{}", publisher.id, self.id))
            .unwrap_or_else(|| self.id.clone())
    }

    pub fn surface_manifest(&self) -> ToolRegistryResult<Option<SurfacePackageManifest>> {
        let Some(surface) = self
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("capabilities"))
            .and_then(|capabilities| capabilities.get("surface"))
        else {
            return Ok(None);
        };
        serde_json::from_value(surface.clone())
            .map(Some)
            .map_err(|error| ToolRegistryError::InvalidToolDefinition {
                id: self.id.clone(),
                reason: format!("Surface manifest is invalid: {error}"),
            })
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ToolExecution {
    #[serde(rename_all = "camelCase")]
    CloudApi {
        endpoint: String,
        method: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        headers: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Mcp {
        server_id: String,
        tool_name: String,
    },
    #[serde(rename_all = "camelCase")]
    Workflow {
        workflow_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workflow_bindings: Option<WorkflowExecutionBindings>,
    },
    #[serde(rename_all = "camelCase")]
    FrameworkArt { framework: String },
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionBindings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<WorkflowInputBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_output: Option<WorkflowOutputBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_output: Option<WorkflowOutputBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview_required_nodes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInputBinding {
    pub workflow_param: String,
    pub node_id: String,
    pub target: String,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutputBinding {
    pub node_id: String,
    pub output: String,
    pub kind: String,
}

impl ToolExecution {
    fn validate(&self, tool_id: &str) -> ToolRegistryResult<()> {
        match self {
            Self::CloudApi {
                endpoint, method, ..
            } => {
                require_non_empty(tool_id, endpoint, "endpoint")?;
                require_non_empty(tool_id, method, "method")
            }
            Self::Mcp {
                server_id,
                tool_name,
            } => {
                require_non_empty(tool_id, server_id, "server_id")?;
                require_non_empty(tool_id, tool_name, "tool_name")
            }
            Self::Workflow { workflow_id, .. } => {
                require_non_empty(tool_id, workflow_id, "workflow_id")
            }
            Self::FrameworkArt { framework } => {
                require_non_empty(tool_id, framework, "framework")?;
                if !framework::is_valid_framework_reference(framework) {
                    return Err(ToolRegistryError::InvalidToolDefinition {
                        id: tool_id.to_owned(),
                        reason: "framework must be a safe package id".to_owned(),
                    });
                }
                Ok(())
            }
        }
    }
}
