//! Surface and identifier validation coverage.

use super::*;

#[cfg(windows)]
#[test]
pub(super) fn registry_file_replacement_supports_extended_length_paths() {
    let root = temp_root("long-registry-path");
    let mut directory = root.clone();
    while directory.as_os_str().to_string_lossy().len() < 270 {
        directory = directory.join("extended-registry-segment");
    }
    fs::create_dir_all(&directory).expect("create extended-length directory");
    let source = directory.join("registry.json.tmp");
    let destination = directory.join("registry.json");
    fs::write(&source, b"replacement").expect("write temporary registry file");

    replace_registry_file(&source, &destination)
        .expect("atomically replace registry file at an extended-length path");

    assert!(!source.exists());
    assert_eq!(
        fs::read(&destination).expect("read registry file"),
        b"replacement"
    );
    fs::remove_dir_all(root).expect("remove extended-length test directory");
}

#[test]
pub(super) fn mcp_tool_definition_requires_server_and_tool_name() {
    let missing_server = ToolDefinition::new(
        "brave-search",
        "Brave Search",
        "Search the web through MCP",
        ToolExecution::Mcp {
            server_id: String::new(),
            tool_name: "brave_web_search".to_owned(),
        },
    );
    assert!(missing_server.validate().is_err());

    let missing_tool = ToolDefinition::new(
        "brave-search",
        "Brave Search",
        "Search the web through MCP",
        ToolExecution::Mcp {
            server_id: "brave".to_owned(),
            tool_name: " ".to_owned(),
        },
    );
    assert!(missing_tool.validate().is_err());

    let valid = ToolDefinition::new(
        "brave-search",
        "Brave Search",
        "Search the web through MCP",
        ToolExecution::Mcp {
            server_id: "brave".to_owned(),
            tool_name: "brave_web_search".to_owned(),
        },
    );
    assert!(valid.validate().is_ok());
}

#[test]
pub(super) fn surface_manifest_requires_safe_package_local_entries() {
    let mut tool = ToolDefinition::new(
        "stock-price",
        "Stock Price",
        "Interactive stock card",
        ToolExecution::FrameworkArt {
            framework: "framework_art".to_owned(),
        },
    );
    tool.metadata = Some(serde_json::json!({
        "capabilities": {
            "surface": {
                "protocolVersion": "loom.surface.v1",
                "apiVersion": "1.0",
                "variants": [{
                    "runtime": "declarative",
                    "entry": "surface/main.json"
                }],
                "fallbackScene": "surface/fallback.json",
                "requiredNodes": ["column", "text", "button"]
            }
        }
    }));
    assert!(tool.validate().is_ok());

    tool.metadata.as_mut().expect("metadata")["capabilities"]["surface"]["variants"][0]["entry"] =
        serde_json::json!("../escape.json");
    assert!(matches!(
        tool.validate(),
        Err(ToolRegistryError::InvalidToolDefinition { reason, .. })
            if reason.contains("entry path is unsafe")
    ));
}

#[test]
pub(super) fn surface_manifest_validates_named_view_full_sizes_and_default() {
    let mut tool = ToolDefinition::new(
        "stock-price",
        "Stock Price",
        "Interactive stock card",
        ToolExecution::FrameworkArt {
            framework: "framework_art".to_owned(),
        },
    );
    tool.metadata = Some(serde_json::json!({
        "capabilities": {
            "surface": {
                "protocolVersion": "loom.surface.v1",
                "apiVersion": "1.0",
                "variants": [{
                    "runtime": "javascript",
                    "entry": "surface/main.js"
                }],
                "views": [
                    { "id": "full", "label": "Full", "fullSize": { "width": 960, "height": 820 } },
                    { "id": "price", "label": "Price", "fullSize": { "width": 620, "height": 560 } }
                ],
                "defaultViewId": "full"
            }
        }
    }));
    assert!(tool.validate().is_ok());

    tool.metadata.as_mut().expect("metadata")["capabilities"]["surface"]["defaultViewId"] =
        serde_json::json!("missing");
    assert!(matches!(
        tool.validate(),
        Err(ToolRegistryError::InvalidToolDefinition { reason, .. })
            if reason.contains("default view id missing is not declared")
    ));
}

#[test]
pub(super) fn workflow_tool_definition_requires_workflow_id() {
    let invalid = ToolDefinition::new(
        "paint-flow",
        "Paint Flow",
        "Run a saved workflow",
        ToolExecution::Workflow {
            workflow_id: String::new(),
            workflow_bindings: None,
        },
    );
    assert!(invalid.validate().is_err());

    let valid = ToolDefinition::new(
        "paint-flow",
        "Paint Flow",
        "Run a saved workflow",
        ToolExecution::Workflow {
            workflow_id: "workflow-1".to_owned(),
            workflow_bindings: None,
        },
    );
    assert!(valid.validate().is_ok());
}

#[test]
pub(super) fn framework_art_tool_definition_requires_a_safe_framework_id() {
    let invalid = ToolDefinition::new(
        "third-party-art",
        "Third-party Art",
        "Reject a framework path instead of treating it as a package id",
        ToolExecution::FrameworkArt {
            framework: "../outside".to_owned(),
        },
    );
    assert!(matches!(
        invalid.validate(),
        Err(ToolRegistryError::InvalidToolDefinition { reason, .. })
            if reason.contains("safe package id")
    ));

    let valid = ToolDefinition::new(
        "third-party-art",
        "Third-party Art",
        "Accept a safe dynamic framework id",
        ToolExecution::FrameworkArt {
            framework: "third-party.echo-v2".to_owned(),
        },
    );
    assert!(valid.validate().is_ok());
}

#[test]
pub(super) fn tool_definition_preserves_desktop_port_metadata() {
    let tool: ToolDefinition = serde_json::from_value(serde_json::json!({
        "id": "advanced-cli-art",
        "name": "Advanced CLI Art",
        "description": "Desktop Add Art advanced ports",
        "enabled": true,
        "execution": { "type": "framework_art", "framework": "process" },
        "inputs": [{
            "name": "image",
            "label": "Image",
            "type": "image",
            "executionType": "image_path",
            "default": "input.png"
        }],
        "outputs": [{
            "name": "result",
            "label": "Result",
            "type": "image",
            "executionType": "image_path",
            "captureMode": "derived_template",
            "filename": "{{inputs.image.path}}_out.png"
        }],
        "params": [{
            "id": "shaderMode",
            "label": "Shader mode",
            "widget": "checkbox",
            "dataType": "bool",
            "default": true
        }]
    }))
    .expect("deserialize advanced Add Art tool definition");

    assert_eq!(tool.inputs[0]["name"], "image");
    assert_eq!(tool.outputs[0]["captureMode"], "derived_template");
    assert_eq!(tool.params[0]["id"], "shaderMode");

    let serialized =
        serde_json::to_value(&tool).expect("serialize advanced Add Art tool definition");
    assert_eq!(serialized["inputs"][0]["executionType"], "image_path");
    assert_eq!(
        serialized["outputs"][0]["filename"],
        "{{inputs.image.path}}_out.png"
    );
    assert_eq!(serialized["params"][0]["default"], true);
}

#[test]
pub(super) fn framework_art_execution_type_deserializes_without_host_specific_fields() {
    let value = serde_json::from_value::<ToolDefinition>(serde_json::json!({
        "id": "third-party-art",
        "name": "Third-party Art",
        "description": "External framework Art",
        "enabled": true,
        "execution": {
            "type": "framework_art",
            "framework": "process"
        }
    }));
    assert!(
        value.is_ok(),
        "framework_art execution should deserialize: {value:?}"
    );
}
