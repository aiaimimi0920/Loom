//! Multi-device Otter Sync live execution tracking, reusable workflow history archive,
//! and two-level Host Device -> App Sessions hierarchy helpers.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use loom_protocol::HookEvent;
use loom_workflow_store::WorkflowStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    broadcast_otter_bridge_json, invalid_request, path_id, path_id_with_suffix, structured_error,
    unix_time_millis, ParsedHttpRequest, SharedOtterBridgeRuntime,
};

const HISTORY_FILE_REL: &str = "otter-sync/workflow-history.json";
const MAX_HISTORY_ENTRIES: usize = 200;
const MAX_WORKFLOW_DATA_BYTES: usize = 512 * 1024;
const MAX_TRACKED_DEVICES: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtterDeviceSyncState {
    pub device_id: String,
    #[serde(default)]
    pub execution_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_node_id: Option<String>,
    #[serde(default)]
    pub node_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    pub updated_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportOtterSyncRequest {
    execution_state: String,
    #[serde(default)]
    workflow_id: Option<String>,
    #[serde(default)]
    workflow_name: Option<String>,
    #[serde(default)]
    active_node_id: Option<String>,
    #[serde(default)]
    node_count: usize,
    #[serde(default)]
    progress: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedOtterWorkflow {
    pub entry_id: String,
    pub workflow_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_device_id: Option<String>,
    pub data: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub archived_at_ms: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryDocument {
    #[serde(default)]
    entries: Vec<ArchivedOtterWorkflow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveWorkflowRequest {
    workflow_id: String,
    name: String,
    #[serde(default)]
    source_device_id: Option<String>,
    data: String,
    #[serde(default)]
    tags: Vec<String>,
}

type LiveSyncMap = BTreeMap<(PathBuf, String), OtterDeviceSyncState>;

fn live_sync_map() -> &'static Mutex<LiveSyncMap> {
    static MAP: OnceLock<Mutex<LiveSyncMap>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn history_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn history_path(control_plane_root: &Path) -> PathBuf {
    control_plane_root.join(HISTORY_FILE_REL)
}

fn load_history(control_plane_root: &Path) -> Result<HistoryDocument> {
    match fs::read(history_path(control_plane_root)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(HistoryDocument::default())
        }
        Err(error) => Err(error.into()),
    }
}

fn save_history(control_plane_root: &Path, doc: &HistoryDocument) -> Result<()> {
    let path = history_path(control_plane_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut bytes = serde_json::to_vec_pretty(doc)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn device_sync_state(root: &Path, device_id: &str) -> Option<OtterDeviceSyncState> {
    live_sync_map()
        .lock()
        .ok()
        .and_then(|map| map.get(&(root.to_owned(), device_id.to_owned())).cloned())
}

fn live_sync_snapshot(root: &Path) -> Vec<OtterDeviceSyncState> {
    live_sync_map()
        .lock()
        .map(|map| {
            map.iter()
                .filter(|((scope, _), _)| scope == root)
                .map(|(_, state)| state.clone())
                .collect()
        })
        .unwrap_or_default()
}

pub fn summary_snapshot(control_plane_root: &Path) -> Value {
    let devices = live_sync_snapshot(control_plane_root);
    let _guard = history_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let history_count = load_history(control_plane_root)
        .map(|doc| doc.entries.len())
        .unwrap_or(0);
    json!({
        "devices": devices,
        "archivedWorkflowCount": history_count,
    })
}

pub fn restore_history_entry(
    control_plane_root: &Path,
    entry_id: &str,
    workflow_store: &WorkflowStore,
    hook_bridge: &SharedOtterBridgeRuntime,
) -> std::result::Result<Value, (u16, String)> {
    let entry = {
        let _guard = history_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let doc = load_history(control_plane_root)
            .map_err(|e| (500, format!("read workflow history failed: {e}")))?;
        doc.entries
            .into_iter()
            .find(|e| e.entry_id == entry_id)
            .ok_or_else(|| (404, format!("archived workflow `{entry_id}` not found")))?
    };
    let saved = workflow_store
        .save_workflow(&entry.workflow_id, &entry.data)
        .map_err(|e| (400, e.to_string()))?;
    broadcast_otter_bridge_json(
        hook_bridge,
        HookEvent {
            protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
            method: "loom.hook.workflow.restored".to_owned(),
            params: json!({
                "entryId": &entry.entry_id,
                "workflowId": &entry.workflow_id,
                "name": &entry.name,
            }),
        },
    );
    Ok(json!({
        "restored": true,
        "entryId": entry.entry_id,
        "workflow": saved,
    }))
}

pub fn route(
    request: &ParsedHttpRequest,
    route_path: &str,
    authenticated_device_id: Option<&str>,
    admin_authenticated: bool,
    control_plane_root: &Path,
    workflow_store: &WorkflowStore,
    hook_bridge: &SharedOtterBridgeRuntime,
) -> Option<Result<(u16, String)>> {
    if !route_path.starts_with("/v1/otter-sync/") {
        return None;
    }
    if !admin_authenticated && authenticated_device_id.is_none() {
        return Some(structured_error(401, json!({ "code": "unauthorized" })));
    }
    match (request.method.as_str(), route_path) {
        ("GET", "/v1/otter-sync/devices") => {
            let devices = live_sync_snapshot(control_plane_root);
            Some(
                serde_json::to_string(&json!({ "devices": devices }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/otter-sync/devices/", "/state").is_some() =>
        {
            let device_id =
                path_id_with_suffix(path, "/v1/otter-sync/devices/", "/state").expect("checked");
            if let Some(actor) = authenticated_device_id {
                if !admin_authenticated && actor != device_id {
                    return Some(structured_error(
                        403,
                        json!({ "code": "otter_sync_device_mismatch" }),
                    ));
                }
            }
            let report = match serde_json::from_str::<ReportOtterSyncRequest>(&request.body) {
                Ok(r) => r,
                Err(e) => return Some(invalid_request(e.to_string())),
            };
            if !matches!(
                report.execution_state.as_str(),
                "idle" | "running" | "paused" | "failed"
            ) {
                return Some(invalid_request(
                    "executionState must be idle, running, paused, or failed",
                ));
            }
            let state = OtterDeviceSyncState {
                device_id: device_id.to_owned(),
                execution_state: report.execution_state,
                workflow_id: report.workflow_id,
                workflow_name: report.workflow_name,
                active_node_id: report.active_node_id,
                node_count: report.node_count,
                progress: report.progress,
                updated_at_ms: unix_time_millis(),
            };
            if let Ok(mut map) = live_sync_map().lock() {
                let key = (control_plane_root.to_owned(), device_id.to_owned());
                if map.len() >= MAX_TRACKED_DEVICES && !map.contains_key(&key) {
                    if let Some(oldest) = map
                        .iter()
                        .min_by_key(|(_, v)| v.updated_at_ms)
                        .map(|(k, _)| k.clone())
                    {
                        map.remove(&oldest);
                    }
                }
                map.insert(key, state.clone());
            }
            Some(
                serde_json::to_string(&json!({ "state": state }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("GET", "/v1/otter-sync/history") => {
            let _guard = history_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let doc = match load_history(control_plane_root) {
                Ok(doc) => doc,
                Err(error) => return Some(Err(error)),
            };
            let summaries = doc
                .entries
                .iter()
                .map(|e| {
                    json!({
                        "entryId": e.entry_id,
                        "workflowId": e.workflow_id,
                        "name": e.name,
                        "sourceDeviceId": e.source_device_id,
                        "tags": e.tags,
                        "archivedAtMs": e.archived_at_ms,
                    })
                })
                .collect::<Vec<_>>();
            Some(
                serde_json::to_string(&json!({ "entries": summaries }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("POST", "/v1/otter-sync/history/archive") => {
            let req = match serde_json::from_str::<ArchiveWorkflowRequest>(&request.body) {
                Ok(req) => req,
                Err(e) => return Some(invalid_request(e.to_string())),
            };
            if req.workflow_id.trim().is_empty()
                || req.name.trim().is_empty()
                || req.data.trim().is_empty()
            {
                return Some(invalid_request("workflowId, name, and data are required"));
            }
            if req.data.len() > MAX_WORKFLOW_DATA_BYTES {
                return Some(structured_error(
                    413,
                    json!({ "code": "workflow_archive_too_large" }),
                ));
            }
            let _guard = history_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut doc = match load_history(control_plane_root) {
                Ok(doc) => doc,
                Err(error) => return Some(Err(error)),
            };
            while doc.entries.len() >= MAX_HISTORY_ENTRIES {
                doc.entries.remove(0);
            }
            let entry = ArchivedOtterWorkflow {
                entry_id: format!("wf-hist-{}", Uuid::new_v4()),
                workflow_id: req.workflow_id.trim().to_owned(),
                name: req.name.trim().to_owned(),
                source_device_id: authenticated_device_id
                    .map(str::to_owned)
                    .or(req.source_device_id),
                data: req.data,
                tags: req.tags,
                archived_at_ms: unix_time_millis(),
            };
            doc.entries.push(entry.clone());
            if let Err(error) = save_history(control_plane_root, &doc) {
                return Some(Err(error));
            }
            Some(
                serde_json::to_string(&json!({ "entry": entry }))
                    .map(|s| (201, s))
                    .map_err(Into::into),
            )
        }
        ("POST", path)
            if path_id_with_suffix(path, "/v1/otter-sync/history/", "/restore").is_some() =>
        {
            if !admin_authenticated {
                return Some(structured_error(
                    403,
                    json!({ "code": "otter_sync_admin_required" }),
                ));
            }
            let entry_id =
                path_id_with_suffix(path, "/v1/otter-sync/history/", "/restore").expect("checked");
            match restore_history_entry(control_plane_root, entry_id, workflow_store, hook_bridge) {
                Ok(val) => Some(
                    serde_json::to_string(&val)
                        .map(|s| (200, s))
                        .map_err(Into::into),
                ),
                Err((status, message)) => Some(structured_error(
                    status,
                    json!({ "code": "workflow_restore_failed", "message": message }),
                )),
            }
        }
        ("DELETE", path) if path_id(path, "/v1/otter-sync/history/").is_some() => {
            if !admin_authenticated {
                return Some(structured_error(
                    403,
                    json!({ "code": "otter_sync_admin_required" }),
                ));
            }
            let entry_id = path_id(path, "/v1/otter-sync/history/").expect("checked");
            let _guard = history_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut doc = match load_history(control_plane_root) {
                Ok(doc) => doc,
                Err(error) => return Some(Err(error)),
            };
            let before = doc.entries.len();
            doc.entries.retain(|e| e.entry_id != entry_id);
            if doc.entries.len() == before {
                return Some(structured_error(404, json!({ "code": "entry_not_found" })));
            }
            if let Err(error) = save_history(control_plane_root, &doc) {
                return Some(Err(error));
            }
            Some(
                serde_json::to_string(&json!({ "deleted": true, "entryId": entry_id }))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_states_are_isolated_by_control_plane_root() {
        let a = PathBuf::from(Uuid::new_v4().to_string());
        let b = PathBuf::from(Uuid::new_v4().to_string());
        let state = OtterDeviceSyncState {
            device_id: "same-device".into(),
            execution_state: "running".into(),
            workflow_id: Some("private-workflow".into()),
            workflow_name: None,
            active_node_id: None,
            node_count: 1,
            progress: None,
            updated_at_ms: 1,
        };
        let key = (a.clone(), state.device_id.clone());
        live_sync_map().lock().unwrap().insert(key.clone(), state);
        assert!(device_sync_state(&a, "same-device").is_some());
        assert!(device_sync_state(&b, "same-device").is_none());
        assert_eq!(live_sync_snapshot(&a).len(), 1);
        assert!(live_sync_snapshot(&b).is_empty());
        live_sync_map().lock().unwrap().remove(&key);
    }
}
