//! ArtHook compatibility bridge protocol contracts for Loom.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

pub const HOOK_BRIDGE_PORT: u16 = 19820;

const LEGACY_METHOD_NAMES: &[&str] = &[
    "handshake",
    "list_arts",
    "get_enabled_arts",
    "sync_user_arts",
    "art_loom/get_user_arts",
    "art_loom/sync_user_arts",
    "art_loom/get_capabilities",
    "get_settings",
    "get_shortcuts",
    "read_arthook_session",
    "update_art_param",
    "sync_shortcuts",
    "subscribe",
    "art_loom/ocr_image",
    "art_loom/translate_text",
    "art/process",
    "art/update_property",
    "art_loom/update_workflow_node",
    "art_loom/overwrite_workflow",
    "art_loom/list_workflows",
    "art_loom/save_workflow_metadata",
    "art_loom/save_workflow_data",
    "art_loom/load_workflow_data",
    "art_loom/delete_workflow_data",
    "art_loom/instantiate_workflow",
    "art_loom/execute_art_node",
    "art_loom/workflow_updated",
    "art_loom/arts_updated",
    "art_hook/instantiate",
];

#[derive(Debug, Error)]
pub enum HookBridgeError {
    #[error("failed to parse Hook bridge request: {0}")]
    Json(#[from] serde_json::Error),
    #[error("workflow store error: {0}")]
    WorkflowStore(#[from] loom_workflow_store::WorkflowStoreError),
}

pub type HookBridgeResult<T> = Result<T, HookBridgeError>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "method", content = "params")]
pub enum HookBridgeRequest {
    #[serde(rename = "handshake")]
    Handshake { client_version: String },

    #[serde(rename = "list_arts")]
    ListArts,

    #[serde(rename = "get_enabled_arts")]
    GetEnabledArts,

    #[serde(rename = "sync_user_arts")]
    SyncUserArts {
        #[serde(default)]
        arts: Vec<JsonValue>,
    },

    #[serde(rename = "art_loom/get_user_arts")]
    GetUserArts,

    #[serde(rename = "art_loom/sync_user_arts")]
    SyncUserArtsNamespaced {
        #[serde(default)]
        arts: Vec<JsonValue>,
    },

    #[serde(rename = "art_loom/get_capabilities")]
    GetCapabilities,

    #[serde(rename = "get_settings")]
    GetSettings,

    #[serde(rename = "get_shortcuts")]
    GetShortcuts,

    #[serde(rename = "read_arthook_session")]
    ReadArtHookSession,

    #[serde(rename = "update_art_param")]
    UpdateArtParam {
        art_id: String,
        param_id: String,
        value: JsonValue,
    },

    #[serde(rename = "sync_shortcuts")]
    SyncShortcuts,

    #[serde(rename = "subscribe")]
    Subscribe {
        #[serde(default)]
        channels: Vec<String>,
    },

    #[serde(rename = "art/process")]
    Process {
        request_id: String,
        art_id: String,
        input: JsonValue,
        #[serde(default)]
        params: BTreeMap<String, JsonValue>,
        #[serde(default)]
        input_images: BTreeMap<String, JsonValue>,
        #[serde(default)]
        disabled_params: Vec<String>,
    },

    #[serde(rename = "art/update_property")]
    UpdateProperty {
        request_id: String,
        property_id: String,
        value: JsonValue,
    },

    #[serde(rename = "art_loom/update_workflow_node")]
    UpdateWorkflowNode {
        workflow_id: String,
        node_id: String,
        param: String,
        value: JsonValue,
    },

    #[serde(rename = "art_loom/overwrite_workflow")]
    OverwriteWorkflow {
        workflow_id: String,
        snapshot: JsonValue,
    },

    #[serde(rename = "art_loom/list_workflows")]
    ListWorkflows,

    #[serde(rename = "art_loom/save_workflow_metadata")]
    SaveWorkflowMetadata { workflow: JsonValue },

    #[serde(rename = "art_loom/save_workflow_data")]
    SaveWorkflowData { id: String, data: String },

    #[serde(rename = "art_loom/load_workflow_data")]
    LoadWorkflowData { id: String },

    #[serde(rename = "art_loom/delete_workflow_data")]
    DeleteWorkflowData { id: String },

    #[serde(rename = "art_loom/instantiate_workflow")]
    InstantiateWorkflow {
        nodes: Vec<JsonValue>,
        edges: Vec<JsonValue>,
        mode: String,
        workflow_id: Option<String>,
    },

    #[serde(rename = "art_loom/execute_art_node")]
    ExecuteArtNode {
        node_id: String,
        art_id: String,
        input_base64: Option<String>,
        #[serde(default)]
        params: BTreeMap<String, JsonValue>,
    },

    #[serde(rename = "art_loom/ocr_image")]
    OcrImage { image_base64: String },

    #[serde(rename = "art_loom/translate_text")]
    TranslateText { text: String, target_lang: String },
}

pub fn parse_request(raw: &str) -> HookBridgeResult<HookBridgeRequest> {
    serde_json::from_str(raw).map_err(HookBridgeError::from)
}

#[derive(Clone, Debug)]
pub struct HookBridgeRuntimeInput {
    pub tools: Vec<JsonValue>,
    pub workflow_root: PathBuf,
    pub ocr_available: bool,
}

impl HookBridgeRuntimeInput {
    #[must_use]
    pub fn new(tools: Vec<JsonValue>, workflow_root: impl Into<PathBuf>) -> Self {
        Self {
            tools,
            workflow_root: workflow_root.into(),
            ocr_available: false,
        }
    }

    #[must_use]
    pub fn with_ocr_available(mut self, ocr_available: bool) -> Self {
        self.ocr_available = ocr_available;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn empty_for_test() -> Self {
        Self::new(
            Vec::new(),
            std::env::temp_dir().join("loom-hook-bridge-empty"),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HookBridgeHandlerResult {
    pub response: JsonValue,
    pub broadcasts: Vec<JsonValue>,
}

pub fn handle_request(
    request: HookBridgeRequest,
    input: HookBridgeRuntimeInput,
) -> HookBridgeResult<HookBridgeHandlerResult> {
    match request {
        HookBridgeRequest::Handshake { .. } => Ok(handler_response(
            serde_json::json!({
                "type": "handshake",
                "data": {
                    "server_version": "0.1.0",
                    "session_id": new_session_id()
                }
            }),
            Vec::new(),
        )),
        HookBridgeRequest::ListArts => Ok(handler_response(
            serde_json::json!({
                "type": "arts",
                "data": input.tools
            }),
            Vec::new(),
        )),
        HookBridgeRequest::GetEnabledArts => {
            let enabled = input
                .tools
                .into_iter()
                .filter(|tool| {
                    tool.get("enabled")
                        .and_then(JsonValue::as_bool)
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>();
            Ok(handler_response(
                serde_json::json!({
                    "type": "arts",
                    "data": enabled
                }),
                Vec::new(),
            ))
        }
        HookBridgeRequest::GetUserArts => Ok(handler_response(
            serde_json::json!({
                "type": "success",
                "data": input
                    .tools
                    .iter()
                    .map(legacy_frontend_user_art_card)
                    .collect::<Vec<_>>()
            }),
            Vec::new(),
        )),
        HookBridgeRequest::SyncUserArts { arts }
        | HookBridgeRequest::SyncUserArtsNamespaced { arts } => {
            let count = input.tools.len();
            Ok(handler_response(
                serde_json::json!({
                    "type": "success",
                    "data": {
                        "compatCommand": "sync_user_arts",
                        "synced": true,
                        "sideEffect": false,
                        "syncedCount": arts.len(),
                        "arts": input.tools,
                        "count": count
                    }
                }),
                Vec::new(),
            ))
        }
        HookBridgeRequest::GetCapabilities => Ok(handler_response(
            serde_json::json!({
                "type": "success",
                "data": {
                    "ocr": input.ocr_available,
                    "translation": true
                }
            }),
            Vec::new(),
        )),
        HookBridgeRequest::TranslateText { text, target_lang } => Ok(handler_response(
            serde_json::json!({
                "type": "success",
                "data": {
                    "translated_text": text,
                    "target_lang": target_lang,
                    "source": "loom-hook-bridge-compat"
                }
            }),
            Vec::new(),
        )),
        HookBridgeRequest::GetSettings => Ok(handler_response(
            serde_json::json!({
                "type": "settings",
                "data": default_legacy_settings()
            }),
            Vec::new(),
        )),
        HookBridgeRequest::GetShortcuts => Ok(handler_response(
            serde_json::json!({
                "type": "shortcuts",
                "data": default_legacy_shortcuts()
            }),
            Vec::new(),
        )),
        HookBridgeRequest::ReadArtHookSession => Ok(handler_response(
            serde_json::json!({
                "type": "success",
                "data": {
                    "stickers": [],
                    "links": [],
                    "source": "read_arthook_session"
                }
            }),
            Vec::new(),
        )),
        HookBridgeRequest::UpdateArtParam {
            art_id,
            param_id,
            value,
        } => Ok(handler_response(
            serde_json::json!({
                "type": "success",
                "data": {
                    "art_id": art_id,
                    "param_id": param_id,
                    "value": value
                }
            }),
            Vec::new(),
        )),
        HookBridgeRequest::SyncShortcuts => Ok(handler_response(
            serde_json::json!({
                "type": "shortcuts",
                "data": default_legacy_shortcuts()
            }),
            Vec::new(),
        )),
        HookBridgeRequest::Subscribe { channels } => Ok(handler_response(
            serde_json::json!({
                "type": "success",
                "data": {
                    "subscribed": true,
                    "channels": channels
                }
            }),
            Vec::new(),
        )),
        HookBridgeRequest::ListWorkflows => {
            let store = loom_workflow_store::WorkflowStore::new(&input.workflow_root);
            match store.list_workflows() {
                Ok(workflows) => Ok(handler_response(
                    serde_json::json!({ "type": "success", "data": workflows }),
                    Vec::new(),
                )),
                Err(error) => Ok(error_handler_response(error.to_string())),
            }
        }
        HookBridgeRequest::SaveWorkflowData { id, data } => {
            let store = loom_workflow_store::WorkflowStore::new(&input.workflow_root);
            match store.save_workflow(&id, &data) {
                Ok(_) => Ok(handler_response(
                    serde_json::json!({ "type": "success", "data": { "id": id } }),
                    Vec::new(),
                )),
                Err(error) => Ok(error_handler_response(error.to_string())),
            }
        }
        HookBridgeRequest::LoadWorkflowData { id } => {
            let store = loom_workflow_store::WorkflowStore::new(&input.workflow_root);
            match store.load_workflow(&id) {
                Ok(content) => Ok(handler_response(
                    serde_json::json!({ "type": "success", "data": content }),
                    Vec::new(),
                )),
                Err(error) => Ok(error_handler_response(error.to_string())),
            }
        }
        HookBridgeRequest::DeleteWorkflowData { id } => {
            let store = loom_workflow_store::WorkflowStore::new(&input.workflow_root);
            match store.delete_workflow(&id) {
                Ok(_) => Ok(handler_response(
                    serde_json::json!({ "type": "success", "data": { "id": id } }),
                    Vec::new(),
                )),
                Err(error) => Ok(error_handler_response(error.to_string())),
            }
        }
        HookBridgeRequest::InstantiateWorkflow {
            nodes,
            edges,
            mode,
            workflow_id,
        } => {
            let graph = serde_json::json!({
                "nodes": nodes.clone(),
                "edges": edges.clone(),
            });
            let yaml = loom_workflow_store::graph_json_to_workflow_yaml(
                &graph,
                Some("Hook 实时工作流"),
                workflow_id.as_deref(),
            )?;
            let store = loom_workflow_store::WorkflowStore::new(&input.workflow_root);
            store.save_workflow("hook-live", &yaml)?;

            let broadcast = instantiate_workflow_broadcast(nodes, edges, mode, workflow_id);
            Ok(handler_response(
                serde_json::json!({ "type": "success" }),
                vec![broadcast],
            ))
        }
        HookBridgeRequest::UpdateWorkflowNode {
            workflow_id,
            node_id,
            param,
            value,
        } => handle_update_workflow_node(input, workflow_id, node_id, param, value),
        HookBridgeRequest::OverwriteWorkflow {
            workflow_id,
            snapshot,
        } => handle_overwrite_workflow(input, workflow_id, snapshot),
        HookBridgeRequest::UpdateProperty {
            request_id,
            property_id,
            value: _,
        } => Ok(handler_response(
            ahrp_update_property_ack_response(&request_id, &property_id),
            Vec::new(),
        )),
        HookBridgeRequest::OcrImage { .. } => Ok(handler_response(
            ocr_image_error_response("OCR enhancement unavailable"),
            Vec::new(),
        )),
        _ => Ok(error_handler_response(
            "Hook bridge method is not implemented",
        )),
    }
}

fn default_legacy_settings() -> JsonValue {
    serde_json::json!({
        "general": {
            "theme": "system",
            "language": "zh-Hans",
            "auto_start": false,
            "minimize_to_tray": true,
            "enable_tray_icon": true
        },
        "system": {
            "auto_check_updates": true,
            "enable_run_log": true,
            "run_as_admin": false,
            "record_screenshot_history": true,
            "history_retention": "7d",
            "enable_proxy": false
        },
        "engine": {
            "comfyui_url": "http://127.0.0.1:8188",
            "python_interpreter": "python.exe",
            "virtual_env_path": "./venv",
            "compute_device": "0",
            "vram_reservation_gb": 12
        },
        "quick_bindings": [
            {
                "id": "1",
                "art": "ComfyUI Workflow",
                "key": "Ctrl+Shift+1"
            }
        ],
        "shortcuts": {
            "capture": {
                "id": "capture",
                "label": "Capture",
                "keys": "Ctrl+1",
                "enabled": true
            },
            "ocr": {
                "id": "ocr",
                "label": "OCR",
                "keys": "Ctrl+2",
                "enabled": true
            },
            "color_picker": {
                "id": "color_picker",
                "label": "Color picker",
                "keys": "Ctrl+3",
                "enabled": false
            },
            "cancel": {
                "id": "cancel",
                "label": "Cancel",
                "keys": "Escape",
                "enabled": true
            },
            "copy_unit": {
                "id": "copy_unit",
                "label": "Copy Unit",
                "keys": "Ctrl+C",
                "enabled": true
            },
            "paste_unit": {
                "id": "paste_unit",
                "label": "Paste Unit",
                "keys": "Ctrl+V",
                "enabled": true
            },
            "save_image": {
                "id": "save_image",
                "label": "Save Image",
                "keys": "Ctrl+S",
                "enabled": true
            },
            "toggle_ocr": {
                "id": "toggle_ocr",
                "label": "Toggle OCR",
                "keys": "Alt+2",
                "enabled": true
            },
            "toggle_translation": {
                "id": "toggle_translation",
                "label": "Toggle Translation",
                "keys": "Alt+3",
                "enabled": true
            }
        }
    })
}

fn default_legacy_shortcuts() -> JsonValue {
    serde_json::json!([
        {
            "id": "capture",
            "label": "Capture",
            "keys": "Ctrl+1",
            "enabled": true
        },
        {
            "id": "cancel",
            "label": "Cancel",
            "keys": "Escape",
            "enabled": true
        },
        {
            "id": "copy_unit",
            "label": "Copy Unit",
            "keys": "Ctrl+C",
            "enabled": true
        },
        {
            "id": "paste_unit",
            "label": "Paste Unit",
            "keys": "Ctrl+V",
            "enabled": true
        },
        {
            "id": "save_image",
            "label": "Save Image",
            "keys": "Ctrl+S",
            "enabled": true
        },
        {
            "id": "toggle_ocr",
            "label": "Toggle OCR",
            "keys": "Alt+2",
            "enabled": true
        },
        {
            "id": "toggle_translation",
            "label": "Toggle Translation",
            "keys": "Alt+3",
            "enabled": true
        }
    ])
}

fn handle_update_workflow_node(
    input: HookBridgeRuntimeInput,
    workflow_id: String,
    node_id: String,
    param: String,
    value: JsonValue,
) -> HookBridgeResult<HookBridgeHandlerResult> {
    let store = loom_workflow_store::WorkflowStore::new(&input.workflow_root);
    let yaml = match store.load_workflow(&workflow_id) {
        Ok(yaml) => yaml,
        Err(error) => return Ok(error_handler_response(error.to_string())),
    };
    let mut graph = match loom_workflow_store::workflow_yaml_to_graph_json(&yaml) {
        Ok(graph) => graph,
        Err(error) => return Ok(error_handler_response(error.to_string())),
    };
    if !set_node_param(&mut graph, &node_id, &param, value) {
        return Ok(error_handler_response("Node not found in workflow"));
    }

    let workflow_name = graph
        .get("name")
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    let workflow_description = graph
        .get("description")
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    let updated_yaml = match loom_workflow_store::graph_json_to_workflow_yaml(
        &graph,
        workflow_name.as_deref(),
        workflow_description.as_deref(),
    ) {
        Ok(updated_yaml) => updated_yaml,
        Err(error) => return Ok(error_handler_response(error.to_string())),
    };

    if let Err(error) = store.save_workflow(&workflow_id, &updated_yaml) {
        return Ok(error_handler_response(error.to_string()));
    }

    let broadcast = workflow_updated_broadcast(&workflow_id, Some(&node_id), graph);
    Ok(handler_response(
        serde_json::json!({ "type": "success" }),
        vec![broadcast],
    ))
}

fn handle_overwrite_workflow(
    input: HookBridgeRuntimeInput,
    workflow_id: String,
    snapshot: JsonValue,
) -> HookBridgeResult<HookBridgeHandlerResult> {
    let store = loom_workflow_store::WorkflowStore::new(&input.workflow_root);
    let workflow_name = snapshot
        .get("name")
        .and_then(JsonValue::as_str)
        .unwrap_or(&workflow_id);
    let workflow_description = snapshot.get("description").and_then(JsonValue::as_str);
    let yaml = match loom_workflow_store::graph_json_to_workflow_yaml(
        &snapshot,
        Some(workflow_name),
        workflow_description,
    ) {
        Ok(yaml) => yaml,
        Err(error) => return Ok(error_handler_response(error.to_string())),
    };
    if let Err(error) = store.save_workflow(&workflow_id, &yaml) {
        return Ok(error_handler_response(error.to_string()));
    }

    let broadcast = workflow_overwritten_broadcast(&workflow_id, snapshot);
    Ok(handler_response(
        serde_json::json!({ "type": "success" }),
        vec![broadcast],
    ))
}

fn set_node_param(graph: &mut JsonValue, node_id: &str, param: &str, value: JsonValue) -> bool {
    let Some(nodes) = graph.get_mut("nodes").and_then(JsonValue::as_array_mut) else {
        return false;
    };

    for node in nodes {
        if node.get("id").and_then(JsonValue::as_str) != Some(node_id) {
            continue;
        }
        let Some(node_object) = node.as_object_mut() else {
            return false;
        };
        let data = node_object
            .entry("data")
            .or_insert_with(|| serde_json::json!({}));
        if !data.is_object() {
            *data = serde_json::json!({});
        }
        let data_object = data.as_object_mut().expect("data object");
        let params = data_object
            .entry("params")
            .or_insert_with(|| serde_json::json!({}));
        if !params.is_object() {
            *params = serde_json::json!({});
        }
        params
            .as_object_mut()
            .expect("params object")
            .insert(param.to_owned(), value);
        return true;
    }

    false
}

fn handler_response(response: JsonValue, broadcasts: Vec<JsonValue>) -> HookBridgeHandlerResult {
    HookBridgeHandlerResult {
        response,
        broadcasts,
    }
}

fn legacy_frontend_user_art_card(tool: &JsonValue) -> JsonValue {
    let compat = tool
        .get("metadata")
        .and_then(JsonValue::as_object)
        .and_then(|metadata| metadata.get("artloomCompat"))
        .and_then(JsonValue::as_object);
    let enabled = tool
        .get("enabled")
        .and_then(JsonValue::as_bool)
        .unwrap_or(true);
    let execution = compat
        .and_then(|compat| compat.get("execution"))
        .cloned()
        .or_else(|| tool.get("execution").cloned())
        .unwrap_or(JsonValue::Null);
    let icon_color = tool
        .get("icon")
        .cloned()
        .or_else(|| tool.get("iconColor").cloned())
        .or_else(|| compat.and_then(|compat| compat.get("icon")).cloned())
        .unwrap_or(JsonValue::Null);
    let execution_type = compat
        .and_then(|compat| compat.get("executionType"))
        .and_then(JsonValue::as_str)
        .or_else(|| tool.get("executionType").and_then(JsonValue::as_str))
        .or_else(|| tool.get("execution_type").and_then(JsonValue::as_str))
        .or_else(|| {
            tool.get("execution")
                .and_then(JsonValue::as_object)
                .and_then(|execution| execution.get("type"))
                .and_then(JsonValue::as_str)
        })
        .unwrap_or_default();
    let inputs = tool
        .get("inputs")
        .cloned()
        .unwrap_or_else(|| JsonValue::Array(Vec::new()));
    let outputs = execution
        .as_object()
        .and_then(|execution| execution.get("outputs"))
        .cloned()
        .or_else(|| tool.get("outputs").cloned())
        .unwrap_or_else(|| JsonValue::Array(Vec::new()));
    let auto_process = tool
        .get("autoProcess")
        .and_then(JsonValue::as_bool)
        .or_else(|| tool.get("auto_process").and_then(JsonValue::as_bool))
        .or_else(|| {
            tool.get("metadata")
                .and_then(JsonValue::as_object)
                .and_then(|metadata| metadata.get("artloomCompat"))
                .and_then(JsonValue::as_object)
                .and_then(|compat| compat.get("autoProcess"))
                .and_then(JsonValue::as_bool)
        })
        .unwrap_or(false);

    serde_json::json!({
        "id": tool
            .get("id")
            .cloned()
            .or_else(|| tool.get("art_id").cloned())
            .unwrap_or(JsonValue::Null),
        "name": tool
            .get("name")
            .cloned()
            .or_else(|| tool.get("label").cloned())
            .or_else(|| tool.get("id").cloned())
            .unwrap_or_else(|| JsonValue::String(String::new())),
        "description": tool
            .get("description")
            .cloned()
            .unwrap_or_else(|| JsonValue::String(String::new())),
        "category": "Adapter",
        "version": "1.0.0",
        "author": "User",
        "status": if enabled { "active" } else { "inactive" },
        "iconColor": icon_color,
        "downloads": 0,
        "owned": true,
        "executionType": execution_type,
        "execution": execution,
        "autoProcess": auto_process,
        "inputs": inputs,
        "outputs": outputs
    })
}

fn error_handler_response(message: impl Into<String>) -> HookBridgeHandlerResult {
    handler_response(
        serde_json::json!({
            "type": "error",
            "data": {
                "message": message.into()
            }
        }),
        Vec::new(),
    )
}

fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("loom-session-{nanos}")
}

#[must_use]
pub fn instantiate_workflow_broadcast(
    nodes: Vec<JsonValue>,
    edges: Vec<JsonValue>,
    mode: impl Into<String>,
    workflow_id: Option<String>,
) -> JsonValue {
    serde_json::json!({
        "method": "art_hook/instantiate",
        "params": {
            "mode": mode.into(),
            "workflow_id": workflow_id,
            "nodes": nodes,
            "edges": edges
        }
    })
}

#[must_use]
pub fn workflow_updated_broadcast(
    workflow_id: &str,
    node_id: Option<&str>,
    data: JsonValue,
) -> JsonValue {
    let mut params = serde_json::json!({
        "workflowId": workflow_id,
        "data": data
    });

    if let Some(node_id) = node_id {
        params["nodeId"] = serde_json::json!(node_id);
    }

    serde_json::json!({
        "method": "art_loom/workflow_updated",
        "params": params
    })
}

#[must_use]
pub fn workflow_overwritten_broadcast(workflow_id: &str, data: JsonValue) -> JsonValue {
    serde_json::json!({
        "method": "art_loom/workflow_updated",
        "params": {
            "workflowId": workflow_id,
            "overwrite": true,
            "data": data
        }
    })
}

#[must_use]
pub fn arts_updated_broadcast() -> JsonValue {
    serde_json::json!({
        "method": "art_loom/arts_updated",
        "params": {}
    })
}

#[must_use]
pub fn execute_art_node_success_response(
    node_id: &str,
    execution_result: JsonValue,
    processing_time_ms: u128,
) -> JsonValue {
    let output_text = extract_execution_text_content(&execution_result);
    serde_json::json!({
        "type": "success",
        "data": {
            "success": true,
            "node_id": node_id,
            "output_text": output_text,
            "processing_time_ms": processing_time_ms
        }
    })
}

#[must_use]
pub fn execute_art_node_image_success_response(
    node_id: &str,
    output_base64: &str,
    processing_time_ms: u64,
) -> JsonValue {
    serde_json::json!({
        "type": "success",
        "data": {
            "success": true,
            "node_id": node_id,
            "output_base64": output_base64,
            "processing_time_ms": processing_time_ms
        }
    })
}

#[must_use]
pub fn execute_art_node_error_response(message: impl Into<String>) -> JsonValue {
    serde_json::json!({
        "type": "error",
        "data": {
            "message": message.into()
        }
    })
}

#[must_use]
pub fn ocr_image_success_response(text: &str, width: u32, height: u32) -> JsonValue {
    serde_json::json!({
        "type": "success",
        "data": {
            "textBlocks": [
                {
                    "boxPoints": [
                        { "x": 0, "y": 0 },
                        { "x": width, "y": 0 },
                        { "x": width, "y": height },
                        { "x": 0, "y": height }
                    ],
                    "boxScore": 1.0,
                    "text": text,
                    "textScore": 1.0,
                    "colorHex": "#000000",
                    "bgColorHex": "#ffffff"
                }
            ],
            "scaleFactor": 1.0,
            "fullText": text,
            "width": width,
            "height": height
        }
    })
}

#[must_use]
pub fn ocr_image_error_response(message: impl Into<String>) -> JsonValue {
    serde_json::json!({
        "type": "error",
        "data": {
            "message": message.into()
        }
    })
}

#[must_use]
pub fn extract_execution_text_content(execution_result: &JsonValue) -> Option<String> {
    let text = execution_result
        .get("content")
        .or_else(|| {
            execution_result
                .get("result")
                .and_then(|result| result.get("content"))
        })
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(JsonValue::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|text| !text.is_empty());
    text
}

#[must_use]
pub fn ahrp_update_property_ack_response(request_id: &str, property_id: &str) -> JsonValue {
    serde_json::json!({
        "request_id": request_id,
        "status": "Success",
        "data": {
            "type": "property_ack",
            "property_id": property_id,
            "applied": true
        }
    })
}

#[must_use]
pub fn ahrp_process_base64_success_response(
    request_id: &str,
    data: &str,
    width: u64,
    height: u64,
    processing_time_ms: u128,
) -> JsonValue {
    serde_json::json!({
        "request_id": request_id,
        "status": "Success",
        "data": {
            "type": "result",
            "output": {
                "type": "base64",
                "data": data,
                "width": width,
                "height": height
            },
            "processing_time_ms": processing_time_ms
        }
    })
}

#[must_use]
pub fn ahrp_process_shared_memory_success_response(
    request_id: &str,
    handle: &str,
    size: usize,
    width: u64,
    height: u64,
    format: &str,
    processing_time_ms: u128,
) -> JsonValue {
    serde_json::json!({
        "request_id": request_id,
        "status": "Success",
        "data": {
            "type": "result",
            "output": {
                "type": "shared_memory",
                "handle": handle,
                "size": size,
                "width": width,
                "height": height,
                "format": format
            },
            "processing_time_ms": processing_time_ms
        }
    })
}

#[must_use]
pub fn ahrp_error_response(
    request_id: &str,
    status: &str,
    message: impl Into<String>,
) -> JsonValue {
    serde_json::json!({
        "request_id": request_id,
        "status": status,
        "error": message.into()
    })
}

#[must_use]
pub fn extract_ahrp_base64_output(execution_result: &JsonValue) -> Option<String> {
    let content = execution_result
        .get("content")
        .or_else(|| {
            execution_result
                .get("result")
                .and_then(|result| result.get("content"))
        })
        .and_then(JsonValue::as_array)?;

    for item in content {
        let item_type = item.get("type").and_then(JsonValue::as_str).unwrap_or("");
        if item_type == "image" {
            let data = item.get("data").and_then(JsonValue::as_str)?;
            let mime_type = item
                .get("mimeType")
                .or_else(|| item.get("mime_type"))
                .and_then(JsonValue::as_str)
                .unwrap_or("image/png");
            if let Some(output) = normalize_ahrp_base64_image(data, mime_type) {
                return Some(output);
            }
        }
        if item_type == "text" {
            let text = item.get("text").and_then(JsonValue::as_str)?;
            if let Some(output) = normalize_ahrp_base64_image(text, "image/png") {
                return Some(output);
            }
        }
    }

    None
}

fn normalize_ahrp_base64_image(value: &str, mime_type: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("data:image/") && trimmed.contains(";base64,") {
        return Some(trimmed.to_owned());
    }
    if looks_like_base64_payload(trimmed) {
        return Some(format!("data:{mime_type};base64,{trimmed}"));
    }
    None
}

fn looks_like_base64_payload(value: &str) -> bool {
    value.len() >= 8
        && !value.chars().any(char::is_whitespace)
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '-' | '_'))
}

#[must_use]
pub fn legacy_method_names() -> &'static [&'static str] {
    LEGACY_METHOD_NAMES
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("loom-hook-bridge-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp hook bridge root");
        root
    }

    #[test]
    fn parses_legacy_handshake_request() {
        let request =
            parse_request(r#"{"method":"handshake","params":{"client_version":"0.4.2"}}"#)
                .expect("parse handshake");

        assert_eq!(
            request,
            HookBridgeRequest::Handshake {
                client_version: "0.4.2".to_owned()
            }
        );
    }

    #[test]
    fn parses_legacy_update_workflow_node_request() {
        let request = parse_request(
            r#"{"method":"art_loom/update_workflow_node","params":{"workflow_id":"wf-1","node_id":"node-a","param":"strength","value":0.75}}"#,
        )
        .expect("parse node update");

        assert_eq!(
            request,
            HookBridgeRequest::UpdateWorkflowNode {
                workflow_id: "wf-1".to_owned(),
                node_id: "node-a".to_owned(),
                param: "strength".to_owned(),
                value: serde_json::json!(0.75),
            }
        );
    }

    #[test]
    fn parses_art_process_request_with_auxiliary_input_images() {
        let request = parse_request(
            r#"{"method":"art/process","params":{"request_id":"req-1","art_id":"custom-image-blend-script","input":{"type":"base64","data":"data:image/png;base64,aaa","width":1,"height":1,"format":"rgba8"},"params":{"mix_ratio":25,"reference":""},"input_images":{"reference":"data:image/png;base64,bbb"},"disabled_params":[]}}"#,
        )
        .expect("parse process request");

        assert_eq!(
            request,
            HookBridgeRequest::Process {
                request_id: "req-1".to_owned(),
                art_id: "custom-image-blend-script".to_owned(),
                input: serde_json::json!({
                    "type": "base64",
                    "data": "data:image/png;base64,aaa",
                    "width": 1,
                    "height": 1,
                    "format": "rgba8"
                }),
                params: BTreeMap::from([
                    ("mix_ratio".to_owned(), serde_json::json!(25)),
                    ("reference".to_owned(), serde_json::json!("")),
                ]),
                input_images: BTreeMap::from([(
                    "reference".to_owned(),
                    serde_json::json!("data:image/png;base64,bbb"),
                )]),
                disabled_params: Vec::new(),
            }
        );
    }

    #[test]
    fn parses_legacy_read_arthook_session_request() {
        let request =
            parse_request(r#"{"method":"read_arthook_session"}"#).expect("parse session request");

        assert_eq!(request, HookBridgeRequest::ReadArtHookSession);
    }

    #[test]
    fn instantiate_broadcast_uses_compat_method_name() {
        let message = instantiate_workflow_broadcast(
            vec![serde_json::json!({ "id": "node-a" })],
            vec![serde_json::json!({ "source": "node-a", "target": "node-b" })],
            "reference",
            Some("wf-1".to_owned()),
        );

        assert_eq!(message["method"], "art_hook/instantiate");
        assert_eq!(message["params"]["mode"], "reference");
        assert_eq!(message["params"]["workflow_id"], "wf-1");
        assert_eq!(message["params"]["nodes"][0]["id"], "node-a");
        assert_eq!(message["params"]["edges"][0]["target"], "node-b");
    }

    #[test]
    fn workflow_and_arts_broadcasts_use_legacy_method_names() {
        let workflow_message =
            workflow_updated_broadcast("wf-1", Some("node-a"), serde_json::json!({ "nodes": [] }));
        assert_eq!(workflow_message["method"], "art_loom/workflow_updated");
        assert_eq!(workflow_message["params"]["workflowId"], "wf-1");
        assert_eq!(workflow_message["params"]["nodeId"], "node-a");

        let arts_message = arts_updated_broadcast();
        assert_eq!(arts_message["method"], "art_loom/arts_updated");
        assert_eq!(arts_message["params"], serde_json::json!({}));
    }

    #[test]
    fn execute_art_node_success_response_extracts_mcp_text_content() {
        let response = execute_art_node_success_response(
            "node-mcp",
            serde_json::json!({
                "content": [
                    { "type": "text", "text": "first line" },
                    { "type": "text", "text": "second line" }
                ]
            }),
            7,
        );

        assert_eq!(response["type"], "success");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-mcp");
        assert_eq!(response["data"]["output_text"], "first line\nsecond line");
        assert_eq!(response["data"]["processing_time_ms"], 7);
    }

    #[test]
    fn extract_execution_text_content_reads_nested_result_content() {
        let text = extract_execution_text_content(&serde_json::json!({
            "result": {
                "content": [
                    { "type": "text", "text": "nested error" }
                ]
            }
        }))
        .expect("nested text content");

        assert_eq!(text, "nested error");
    }

    #[test]
    fn execute_art_node_image_success_response_uses_legacy_shape() {
        let response =
            execute_art_node_image_success_response("node-native", "data:image/png;base64,abc", 5);

        assert_eq!(response["type"], "success");
        assert_eq!(response["data"]["success"], true);
        assert_eq!(response["data"]["node_id"], "node-native");
        assert_eq!(
            response["data"]["output_base64"],
            "data:image/png;base64,abc"
        );
        assert_eq!(response["data"]["processing_time_ms"], 5);
    }

    #[test]
    fn execute_art_node_error_response_uses_legacy_shape() {
        let response = execute_art_node_error_response("tool not found");

        assert_eq!(response["type"], "error");
        assert_eq!(response["data"]["message"], "tool not found");
    }

    #[test]
    fn parses_legacy_ocr_image_request() {
        let request = parse_request(
            r#"{"method":"art_loom/ocr_image","params":{"image_base64":"data:image/png;base64,abc"}}"#,
        )
        .expect("parse ocr request");

        assert_eq!(
            request,
            HookBridgeRequest::OcrImage {
                image_base64: "data:image/png;base64,abc".to_owned()
            }
        );
    }

    #[test]
    fn parses_legacy_translate_text_request() {
        let request = parse_request(
            r#"{"method":"art_loom/translate_text","params":{"text":"hello","target_lang":"zh"}}"#,
        )
        .expect("parse translate request");

        assert_eq!(
            request,
            HookBridgeRequest::TranslateText {
                text: "hello".to_owned(),
                target_lang: "zh".to_owned(),
            }
        );
    }

    #[test]
    fn handler_answers_legacy_translate_text_shape() {
        let result = handle_request(
            HookBridgeRequest::TranslateText {
                text: "hello".to_owned(),
                target_lang: "zh".to_owned(),
            },
            HookBridgeRuntimeInput::empty_for_test(),
        )
        .expect("handle translate");

        assert_eq!(result.response["type"], "success");
        assert_eq!(result.response["data"]["translated_text"], "hello");
        assert_eq!(result.response["data"]["target_lang"], "zh");
        assert!(result.broadcasts.is_empty());
    }

    #[test]
    fn ocr_image_success_response_uses_legacy_shape() {
        let response = ocr_image_success_response("hello", 1, 1);

        assert_eq!(response["type"], "success");
        assert_eq!(response["data"]["fullText"], "hello");
        assert_eq!(response["data"]["width"], 1);
        assert_eq!(response["data"]["height"], 1);
        assert_eq!(response["data"]["scaleFactor"], 1.0);
        assert_eq!(response["data"]["textBlocks"][0]["text"], "hello");
        assert_eq!(response["data"]["textBlocks"][0]["colorHex"], "#000000");
        assert_eq!(response["data"]["textBlocks"][0]["bgColorHex"], "#ffffff");
    }

    #[test]
    fn ocr_image_error_response_uses_legacy_shape() {
        let response = ocr_image_error_response("OCR enhancement unavailable");

        assert_eq!(response["type"], "error");
        assert_eq!(response["data"]["message"], "OCR enhancement unavailable");
    }

    #[test]
    fn ahrp_update_property_ack_response_uses_legacy_shape() {
        let response = ahrp_update_property_ack_response("req-1", "strength");

        assert_eq!(response["request_id"], "req-1");
        assert_eq!(response["status"], "Success");
        assert_eq!(response["data"]["type"], "property_ack");
        assert_eq!(response["data"]["property_id"], "strength");
        assert_eq!(response["data"]["applied"], true);
    }

    #[test]
    fn ahrp_process_base64_success_response_uses_legacy_shape() {
        let response =
            ahrp_process_base64_success_response("req-2", "data:image/png;base64,abc", 2, 3, 9);

        assert_eq!(response["request_id"], "req-2");
        assert_eq!(response["status"], "Success");
        assert_eq!(response["data"]["type"], "result");
        assert_eq!(response["data"]["output"]["type"], "base64");
        assert_eq!(
            response["data"]["output"]["data"],
            "data:image/png;base64,abc"
        );
        assert_eq!(response["data"]["output"]["width"], 2);
        assert_eq!(response["data"]["output"]["height"], 3);
        assert_eq!(response["data"]["processing_time_ms"], 9);
    }

    #[test]
    fn ahrp_process_shared_memory_success_response_uses_legacy_shape() {
        let response = ahrp_process_shared_memory_success_response(
            "req-shm",
            "Loom_Buffer_1",
            4,
            1,
            1,
            "rgba8",
            11,
        );

        assert_eq!(response["request_id"], "req-shm");
        assert_eq!(response["status"], "Success");
        assert_eq!(response["data"]["type"], "result");
        assert_eq!(response["data"]["output"]["type"], "shared_memory");
        assert_eq!(response["data"]["output"]["handle"], "Loom_Buffer_1");
        assert_eq!(response["data"]["output"]["size"], 4);
        assert_eq!(response["data"]["output"]["width"], 1);
        assert_eq!(response["data"]["output"]["height"], 1);
        assert_eq!(response["data"]["output"]["format"], "rgba8");
        assert_eq!(response["data"]["processing_time_ms"], 11);
    }

    #[test]
    fn ahrp_error_response_uses_named_status() {
        let response = ahrp_error_response("req-3", "EngineError", "boom");

        assert_eq!(response["request_id"], "req-3");
        assert_eq!(response["status"], "EngineError");
        assert_eq!(response["error"], "boom");
    }

    #[test]
    fn extract_ahrp_base64_output_accepts_mcp_image_and_data_url_text() {
        let image = extract_ahrp_base64_output(&serde_json::json!({
            "content": [{ "type": "image", "data": "iVBORw0KGgo=" }]
        }))
        .expect("image output");
        assert_eq!(image, "data:image/png;base64,iVBORw0KGgo=");

        let text = extract_ahrp_base64_output(&serde_json::json!({
            "content": [{ "type": "text", "text": "data:image/png;base64,abc" }]
        }))
        .expect("text data url output");
        assert_eq!(text, "data:image/png;base64,abc");
    }

    #[test]
    fn legacy_method_catalog_includes_control_plane_surface() {
        assert_eq!(HOOK_BRIDGE_PORT, 19820);

        let methods = legacy_method_names();
        for method in [
            "handshake",
            "list_arts",
            "get_enabled_arts",
            "sync_user_arts",
            "art_loom/get_user_arts",
            "art_loom/sync_user_arts",
            "art_loom/get_capabilities",
            "read_arthook_session",
            "art_loom/translate_text",
            "art_loom/instantiate_workflow",
            "art_loom/update_workflow_node",
            "art_loom/overwrite_workflow",
            "art_loom/workflow_updated",
            "art_loom/arts_updated",
            "art_hook/instantiate",
        ] {
            assert!(methods.contains(&method), "missing legacy method {method}");
        }
    }

    #[test]
    fn handler_answers_legacy_handshake() {
        let result = handle_request(
            HookBridgeRequest::Handshake {
                client_version: "0.4.2".to_owned(),
            },
            HookBridgeRuntimeInput::empty_for_test(),
        )
        .expect("handle handshake");

        assert_eq!(result.response["type"], "handshake");
        assert_eq!(result.response["data"]["server_version"], "0.1.0");
        assert!(result.response["data"]["session_id"].as_str().is_some());
        assert!(result.broadcasts.is_empty());
    }

    #[test]
    fn handler_answers_read_arthook_session_with_empty_snapshot() {
        let result = handle_request(
            HookBridgeRequest::ReadArtHookSession,
            HookBridgeRuntimeInput::empty_for_test(),
        )
        .expect("handle session read");

        assert_eq!(result.response["type"], "success");
        assert!(result.response["data"]["stickers"]
            .as_array()
            .expect("stickers")
            .is_empty());
        assert!(result.response["data"]["links"]
            .as_array()
            .expect("links")
            .is_empty());
        assert_eq!(result.response["data"]["source"], "read_arthook_session");
    }

    #[test]
    fn handler_answers_legacy_settings_and_shortcuts() {
        let settings = handle_request(
            HookBridgeRequest::GetSettings,
            HookBridgeRuntimeInput::empty_for_test(),
        )
        .expect("handle settings");
        assert_eq!(settings.response["type"], "settings");
        assert_eq!(settings.response["data"]["general"]["theme"], "system");
        assert_eq!(
            settings.response["data"]["engine"]["python_interpreter"],
            "python.exe"
        );

        let shortcuts = handle_request(
            HookBridgeRequest::GetShortcuts,
            HookBridgeRuntimeInput::empty_for_test(),
        )
        .expect("handle shortcuts");
        assert_eq!(shortcuts.response["type"], "shortcuts");
        assert_eq!(shortcuts.response["data"][0]["id"], "capture");
        assert_eq!(shortcuts.response["data"][0]["keys"], "Ctrl+1");
        let shortcut_ids = shortcuts.response["data"]
            .as_array()
            .expect("shortcut array")
            .iter()
            .filter_map(|shortcut| shortcut.get("id").and_then(JsonValue::as_str))
            .collect::<Vec<_>>();
        assert_eq!(shortcut_ids.len(), 7);
        assert!(shortcut_ids.contains(&"copy_unit"));
        assert!(shortcut_ids.contains(&"paste_unit"));
        assert!(shortcut_ids.contains(&"save_image"));
        assert!(shortcut_ids.contains(&"toggle_ocr"));
        assert!(shortcut_ids.contains(&"toggle_translation"));

        let updated = handle_request(
            HookBridgeRequest::UpdateArtParam {
                art_id: "fixture-art".to_owned(),
                param_id: "strength".to_owned(),
                value: serde_json::json!(0.5),
            },
            HookBridgeRuntimeInput::empty_for_test(),
        )
        .expect("handle update art param");
        assert_eq!(updated.response["type"], "success");
        assert_eq!(updated.response["data"]["art_id"], "fixture-art");
        assert_eq!(updated.response["data"]["param_id"], "strength");

        let synced = handle_request(
            HookBridgeRequest::SyncShortcuts,
            HookBridgeRuntimeInput::empty_for_test(),
        )
        .expect("handle sync shortcuts");
        assert_eq!(synced.response["type"], "shortcuts");
        assert_eq!(synced.response["data"][0]["id"], "capture");
    }

    #[test]
    fn handler_answers_get_user_arts_with_legacy_frontend_cards() {
        let root = temp_root("get-user-arts");
        let result = handle_request(
            HookBridgeRequest::GetUserArts,
            HookBridgeRuntimeInput::new(
                vec![serde_json::json!({
                    "id": "compat-art",
                    "art_id": "compat-art",
                    "name": "Compat Art",
                    "label": "Compat Art",
                    "description": "ArtLoom registry alias fixture",
                    "icon": "#52c41a",
                    "enabled": true,
                    "execution_type": "cli_wrapper",
                    "execution": { "type": "cli_wrapper", "command": "echo", "args": ["ok"] },
                    "autoProcess": true,
                    "inputs": [{ "name": "image", "type": "image" }],
                    "outputs": [{ "name": "result", "type": "image" }],
                    "params": [{ "id": "strength", "default": 0.1 }]
                })],
                root.clone(),
            ),
        )
        .expect("handle get user arts");

        assert_eq!(result.response["type"], "success");
        assert_eq!(result.response["data"][0]["id"], "compat-art");
        assert_eq!(result.response["data"][0]["name"], "Compat Art");
        assert_eq!(
            result.response["data"][0]["description"],
            "ArtLoom registry alias fixture"
        );
        assert_eq!(result.response["data"][0]["category"], "Adapter");
        assert_eq!(result.response["data"][0]["version"], "1.0.0");
        assert_eq!(result.response["data"][0]["author"], "User");
        assert_eq!(result.response["data"][0]["status"], "active");
        assert_eq!(result.response["data"][0]["iconColor"], "#52c41a");
        assert_eq!(result.response["data"][0]["downloads"], 0);
        assert_eq!(result.response["data"][0]["owned"], true);
        assert_eq!(result.response["data"][0]["executionType"], "cli_wrapper");
        assert_eq!(result.response["data"][0]["autoProcess"], true);
        assert_eq!(result.response["data"][0]["inputs"][0]["name"], "image");
        assert_eq!(result.response["data"][0]["outputs"][0]["name"], "result");
        assert!(result.response["data"][0].get("art_id").is_none());
        assert!(result.broadcasts.is_empty());

        fs::remove_dir_all(root).expect("cleanup temp hook bridge root");
    }

    #[test]
    fn handler_answers_get_user_arts_from_daemon_legacy_metadata() {
        let root = temp_root("get-user-arts-legacy-metadata");
        let result = handle_request(
            HookBridgeRequest::GetUserArts,
            HookBridgeRuntimeInput::new(
                vec![serde_json::json!({
                    "id": "script-art",
                    "name": "Script Art",
                    "description": "Daemon normalized tool with ArtLoom metadata",
                    "enabled": true,
                    "execution_type": "python_art",
                    "execution": {
                        "type": "python_art",
                        "artId": "script-art",
                        "artPath": "C:/Arts/script-art"
                    },
                    "metadata": {
                        "artloomCompat": {
                            "icon": "#fa8c16",
                            "executionType": "script",
                            "execution": {
                                "artPath": "C:/Arts/script-art",
                                "outputs": [{ "name": "result", "type": "image" }]
                            },
                            "autoProcess": true
                        }
                    },
                    "inputs": [{ "name": "image", "type": "image" }],
                    "outputs": [{ "name": "normalized", "type": "image" }]
                })],
                root.clone(),
            ),
        )
        .expect("handle get user arts from metadata");

        assert_eq!(result.response["type"], "success");
        assert_eq!(result.response["data"][0]["iconColor"], "#fa8c16");
        assert_eq!(result.response["data"][0]["executionType"], "script");
        assert_eq!(
            result.response["data"][0]["execution"]["artPath"],
            "C:/Arts/script-art"
        );
        assert_eq!(result.response["data"][0]["outputs"][0]["name"], "result");
        assert_eq!(result.response["data"][0]["autoProcess"], true);
        assert!(result.broadcasts.is_empty());

        fs::remove_dir_all(root).expect("cleanup temp hook bridge root");
    }

    #[test]
    fn handler_instantiates_workflow_as_broadcast() {
        let root = temp_root("instantiate-broadcast");
        let result = handle_request(
            HookBridgeRequest::InstantiateWorkflow {
                nodes: vec![serde_json::json!({ "id": "prompt" })],
                edges: vec![serde_json::json!({
                    "source": "prompt",
                    "target": "image"
                })],
                mode: "reference".to_owned(),
                workflow_id: Some("wf-1".to_owned()),
            },
            HookBridgeRuntimeInput::new(Vec::new(), root.clone()),
        )
        .expect("handle instantiate");

        assert_eq!(result.response["type"], "success");
        assert_eq!(result.broadcasts[0]["method"], "art_hook/instantiate");
        assert_eq!(result.broadcasts[0]["params"]["mode"], "reference");
        assert_eq!(result.broadcasts[0]["params"]["workflow_id"], "wf-1");
        assert_eq!(result.broadcasts[0]["params"]["nodes"][0]["id"], "prompt");
        assert_eq!(
            result.broadcasts[0]["params"]["edges"][0]["target"],
            "image"
        );

        fs::remove_dir_all(root).expect("cleanup temp hook bridge root");
    }

    #[test]
    fn handler_instantiates_workflow_and_writes_hook_live_yaml() {
        let root = temp_root("instantiate-live");
        let store = loom_workflow_store::WorkflowStore::new(&root);

        let result = handle_request(
            HookBridgeRequest::InstantiateWorkflow {
                nodes: vec![serde_json::json!({
                    "id": "screenshot",
                    "type": "artNode",
                    "position": { "x": 24, "y": 48 },
                    "data": {
                        "artId": "hook.capture",
                        "label": "Hook Screenshot"
                    }
                })],
                edges: vec![],
                mode: "reference".to_owned(),
                workflow_id: Some("wf-from-hook".to_owned()),
            },
            HookBridgeRuntimeInput::new(Vec::new(), root.clone()),
        )
        .expect("handle instantiate");

        assert_eq!(result.response["type"], "success");
        assert_eq!(
            result.broadcasts[0]["params"]["workflow_id"],
            "wf-from-hook"
        );

        let yaml = store
            .load_workflow("hook-live")
            .expect("load hook live workflow");
        assert!(yaml.contains("name: Hook 实时工作流"));
        assert!(yaml.contains("id: screenshot"));
        assert!(yaml.contains("uses: hook.capture"));

        let listed = store.list_workflows().expect("list workflows");
        assert!(listed.iter().any(|workflow| workflow.id == "hook-live"));

        fs::remove_dir_all(root).expect("cleanup temp hook bridge root");
    }

    #[test]
    fn handler_updates_workflow_node_and_writes_yaml() {
        let root = temp_root("update-node");
        let store = loom_workflow_store::WorkflowStore::new(&root);
        store
            .save_workflow(
                "wf-1",
                "name: Flow\nnodes:\n  - id: prompt\n    uses: text.prompt\n    with:\n      prompt: old\n",
            )
            .expect("save workflow");

        let result = handle_request(
            HookBridgeRequest::UpdateWorkflowNode {
                workflow_id: "wf-1".to_owned(),
                node_id: "prompt".to_owned(),
                param: "prompt".to_owned(),
                value: serde_json::json!("new"),
            },
            HookBridgeRuntimeInput::new(Vec::new(), root.clone()),
        )
        .expect("handle workflow node update");

        assert_eq!(result.response["type"], "success");
        assert_eq!(result.broadcasts[0]["method"], "art_loom/workflow_updated");
        assert_eq!(result.broadcasts[0]["params"]["workflowId"], "wf-1");
        assert_eq!(result.broadcasts[0]["params"]["nodeId"], "prompt");

        let updated_yaml = store.load_workflow("wf-1").expect("load updated workflow");
        assert!(updated_yaml.contains("prompt: new"));

        fs::remove_dir_all(root).expect("cleanup temp hook bridge root");
    }
}
