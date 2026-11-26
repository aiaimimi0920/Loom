//! Intra-workflow cross-end terminal node relay (`Loom -> Otter Terminal -> Loom`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use loom_workflow_runtime::{
    TerminalNodeRelayRequest, WorkflowRuntimeError, WorkflowRuntimeResult, WorkflowTerminalRelay,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{invalid_request, structured_error, unix_time_millis, ParsedHttpRequest};

pub const HOOK_EVENT_WORKFLOW_NODE_RELAY_REQUEST: &str = "loom.hook.workflow.node_relay.request";
pub const HOOK_METHOD_WORKFLOW_NODE_RELAY_RESULT: &str = "loom.hook.workflow.node_relay.result";
const MAX_PENDING_TERMINAL_RELAYS: usize = 64;
const DEFAULT_TERMINAL_RELAY_TIMEOUT: Duration = Duration::from_secs(30);
const WORKER_HEARTBEAT_TTL_MS: u64 = 60_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingTerminalRelayDescriptor {
    pub relay_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub uses: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub arguments: Value,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRelayResultSubmission {
    pub relay_id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalWorkerHeartbeat {
    device_id: String,
}

struct RelaySlot {
    // None is a socket-owned relay, never exposed to the HTTP worker queue.
    http_scope: Option<PathBuf>,
    descriptor: PendingTerminalRelayDescriptor,
    outcome: Option<std::result::Result<Value, String>>,
}

#[derive(Default)]
struct RelayHubState {
    slots: BTreeMap<String, RelaySlot>,
    worker_heartbeats: BTreeMap<(PathBuf, String), u64>,
}

struct RelayHub {
    state: Mutex<RelayHubState>,
    signal: Condvar,
}

fn global_relay_hub() -> &'static RelayHub {
    static HUB: OnceLock<RelayHub> = OnceLock::new();
    HUB.get_or_init(|| RelayHub {
        state: Mutex::new(RelayHubState::default()),
        signal: Condvar::new(),
    })
}

struct PendingRelayGuard {
    relay_id: String,
}

impl Drop for PendingRelayGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = global_relay_hub().state.lock() {
            state.slots.remove(&self.relay_id);
        }
    }
}

fn record_terminal_worker_heartbeat(root: &Path, device_id: &str) {
    let trimmed = device_id.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut state) = global_relay_hub().state.lock() {
        let now = unix_time_millis();
        state
            .worker_heartbeats
            .retain(|_, seen| now.saturating_sub(*seen) <= WORKER_HEARTBEAT_TTL_MS);
        state
            .worker_heartbeats
            .insert((root.to_path_buf(), trimmed.to_owned()), now);
    }
}

fn has_active_http_relay_worker(root: &Path, preferred_device_id: Option<&str>) -> bool {
    let Ok(state) = global_relay_hub().state.lock() else {
        return false;
    };
    let now = unix_time_millis();
    state
        .worker_heartbeats
        .iter()
        .any(|((scope, device), seen)| {
            scope == root
                && preferred_device_id.is_none_or(|target| target == device)
                && now.saturating_sub(*seen) <= WORKER_HEARTBEAT_TTL_MS
        })
}

pub(crate) fn try_handle_websocket_relay_result(
    raw_text: &str,
    issued_relays: &std::collections::HashSet<String>,
) -> bool {
    let Ok(envelope) = serde_json::from_str::<Value>(raw_text) else {
        return false;
    };
    if envelope.get("method").and_then(Value::as_str)
        != Some(HOOK_METHOD_WORKFLOW_NODE_RELAY_RESULT)
    {
        return false;
    }
    let payload = envelope.get("params").cloned().unwrap_or(envelope);
    if let Ok(submission) = serde_json::from_value::<TerminalRelayResultSubmission>(payload) {
        if issued_relays.contains(&submission.relay_id) {
            let _ = complete_relay_slot(submission, None, None);
        }
    }
    true
}

fn complete_relay_slot(
    submission: TerminalRelayResultSubmission,
    http_scope: Option<&Path>,
    device_id: Option<&str>,
) -> std::result::Result<(), &'static str> {
    let hub = global_relay_hub();
    let mut state = hub.state.lock().map_err(|_| "relay_hub_unavailable")?;
    let Some(slot) = state.slots.get_mut(submission.relay_id.trim()) else {
        return Err("relay_not_found");
    };
    if slot.http_scope.as_deref() != http_scope
        || device_id.is_some_and(|caller| slot.descriptor.device_id.as_deref() != Some(caller))
    {
        return Err("relay_not_found");
    }
    if slot.outcome.is_some() {
        return Err("relay_already_completed");
    }
    let failed = submission
        .status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("failed") || s.eq_ignore_ascii_case("error"))
        || submission.error.is_some();
    slot.outcome = Some(if failed {
        Err(submission
            .error
            .unwrap_or_else(|| "terminal node execution failed".to_owned()))
    } else {
        Ok(submission
            .output
            .unwrap_or_else(|| json!({ "content": [] })))
    });
    hub.signal.notify_all();
    Ok(())
}

pub(crate) struct DaemonOtterWorkflowRelay<'a> {
    http_scope: Option<&'a Path>,
    shutting_down: Option<&'a std::sync::atomic::AtomicBool>,
    default_device: Option<String>,
    ws_emitter: Option<&'a mut dyn FnMut(String)>,
}

impl<'a> DaemonOtterWorkflowRelay<'a> {
    pub(crate) fn for_http(
        control_plane_root: &'a Path,
        shutting_down: &'a std::sync::atomic::AtomicBool,
    ) -> Self {
        Self {
            http_scope: Some(control_plane_root),
            shutting_down: Some(shutting_down),
            default_device: None,
            ws_emitter: None,
        }
    }

    pub(crate) fn for_otter_ws(
        default_device: Option<String>,
        ws_emitter: &'a mut dyn FnMut(String),
    ) -> Self {
        Self {
            http_scope: None,
            shutting_down: None,
            default_device,
            ws_emitter: Some(ws_emitter),
        }
    }
}

impl WorkflowTerminalRelay for DaemonOtterWorkflowRelay<'_> {
    fn has_connected_terminal(&self, preferred_device_id: Option<&str>) -> bool {
        if self.ws_emitter.is_some() {
            return preferred_device_id
                .is_none_or(|target| self.default_device.as_deref() == Some(target));
        }
        self.http_scope
            .is_some_and(|root| has_active_http_relay_worker(root, preferred_device_id))
    }

    fn default_device_id(&self) -> Option<String> {
        if let Some(device) = &self.default_device {
            return Some(device.clone());
        }
        let hub = global_relay_hub();
        if let Ok(state) = hub.state.lock() {
            let now = unix_time_millis();
            if let Some((id, _)) = state.worker_heartbeats.iter().find(|((root, _), seen)| {
                self.http_scope == Some(root.as_path())
                    && now.saturating_sub(**seen) <= WORKER_HEARTBEAT_TTL_MS
            }) {
                return Some(id.1.clone());
            }
        }
        None
    }

    fn execute_on_terminal(
        &mut self,
        request: TerminalNodeRelayRequest<'_>,
    ) -> WorkflowRuntimeResult<Value> {
        if self
            .shutting_down
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            return Err(WorkflowRuntimeError::Cancelled);
        }
        if !self.has_connected_terminal(request.device_id) {
            return Err(WorkflowRuntimeError::TerminalUnavailable {
                workflow_id: request.workflow_id.to_owned(),
                node_id: request.node_id.to_owned(),
            });
        }
        let relay_id = format!("relay-{}", Uuid::new_v4());
        let descriptor = PendingTerminalRelayDescriptor {
            relay_id: relay_id.clone(),
            workflow_id: request.workflow_id.to_owned(),
            node_id: request.node_id.to_owned(),
            uses: request.uses.to_owned(),
            device_id: request
                .device_id
                .map(str::to_owned)
                .or_else(|| self.default_device_id()),
            arguments: request.arguments,
            created_at_ms: unix_time_millis(),
        };

        let hub = global_relay_hub();
        {
            let mut state =
                hub.state
                    .lock()
                    .map_err(|_| WorkflowRuntimeError::TerminalRelayFailed {
                        workflow_id: request.workflow_id.to_owned(),
                        node_id: request.node_id.to_owned(),
                        message: "terminal relay hub lock poisoned".to_owned(),
                    })?;
            if state.slots.len() >= MAX_PENDING_TERMINAL_RELAYS {
                return Err(WorkflowRuntimeError::ResourceLimit {
                    workflow_id: request.workflow_id.to_owned(),
                    reason: "pending terminal relay queue is full".to_owned(),
                });
            }
            state.slots.insert(
                relay_id.clone(),
                RelaySlot {
                    http_scope: self.http_scope.map(Path::to_path_buf),
                    descriptor: descriptor.clone(),
                    outcome: None,
                },
            );
        }
        let _guard = PendingRelayGuard {
            relay_id: relay_id.clone(),
        };

        let event_payload = json!({
            "protocolVersion": loom_protocol::HOOK_PROTOCOL_VERSION,
            "event": HOOK_EVENT_WORKFLOW_NODE_RELAY_REQUEST,
            "data": &descriptor,
        });

        if let Some(emitter) = self.ws_emitter.as_deref_mut() {
            emitter(serde_json::to_string(&event_payload).unwrap_or_default());
        }

        let budget = request.timeout.unwrap_or(DEFAULT_TERMINAL_RELAY_TIMEOUT);
        let deadline = Instant::now() + budget;
        let mut state =
            hub.state
                .lock()
                .map_err(|_| WorkflowRuntimeError::TerminalRelayFailed {
                    workflow_id: request.workflow_id.to_owned(),
                    node_id: request.node_id.to_owned(),
                    message: "terminal relay hub lock poisoned".to_owned(),
                })?;
        loop {
            if self
                .shutting_down
                .is_some_and(|flag| flag.load(Ordering::Acquire))
                || request
                    .cancellation
                    .is_some_and(|token| token.load(Ordering::Acquire))
            {
                return Err(WorkflowRuntimeError::Cancelled);
            }
            if let Some(outcome) = state
                .slots
                .get(&relay_id)
                .and_then(|slot| slot.outcome.clone())
            {
                return outcome.map_err(|message| WorkflowRuntimeError::TerminalRelayFailed {
                    workflow_id: request.workflow_id.to_owned(),
                    node_id: request.node_id.to_owned(),
                    message,
                });
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(WorkflowRuntimeError::Timeout);
            }
            let step = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(50));
            let (next, _) = hub.signal.wait_timeout(state, step).map_err(|_| {
                WorkflowRuntimeError::TerminalRelayFailed {
                    workflow_id: request.workflow_id.to_owned(),
                    node_id: request.node_id.to_owned(),
                    message: "terminal relay wait failed".to_owned(),
                }
            })?;
            state = next;
        }
    }
}

pub(crate) fn route(
    request: &ParsedHttpRequest,
    route_path: &str,
    authenticated_device_id: Option<&str>,
    admin_authenticated: bool,
    control_plane_root: &Path,
) -> Option<Result<(u16, String)>> {
    if !route_path.starts_with("/v1/otter/relay/") {
        return None;
    }
    if !admin_authenticated && authenticated_device_id.is_none() {
        return Some(structured_error(
            401,
            json!({ "code": "unauthorized", "message": "terminal relay requires authentication" }),
        ));
    }
    match (request.method.as_str(), route_path) {
        ("POST", "/v1/otter/relay/register") => {
            let parsed = match serde_json::from_str::<TerminalWorkerHeartbeat>(&request.body) {
                Ok(parsed) => parsed,
                Err(error) => return Some(invalid_request(error.to_string())),
            };
            let effective_device = authenticated_device_id.unwrap_or(&parsed.device_id);
            if effective_device.trim().is_empty() {
                return Some(invalid_request("deviceId is required"));
            }
            record_terminal_worker_heartbeat(control_plane_root, effective_device);
            Some(Ok((
                200,
                serde_json::to_string(&json!({ "registered": true, "deviceId": effective_device }))
                    .unwrap_or_default(),
            )))
        }
        ("GET", "/v1/otter/relay/pending") => {
            let query_device = request.query_parameter("deviceId");
            let effective_device = authenticated_device_id.or(query_device.as_deref());
            if let Some(dev) = effective_device {
                record_terminal_worker_heartbeat(control_plane_root, dev);
            }
            let items = global_relay_hub()
                .state
                .lock()
                .map(|state| {
                    state
                        .slots
                        .values()
                        .filter(|slot| slot.http_scope.as_deref() == Some(control_plane_root))
                        .filter(|slot| slot.outcome.is_none())
                        .filter(|slot| {
                            match (effective_device, slot.descriptor.device_id.as_deref()) {
                                (Some(caller), Some(target)) => caller == target,
                                _ => true,
                            }
                        })
                        .map(|slot| slot.descriptor.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(Ok((
                200,
                serde_json::to_string(&json!({ "relays": items })).unwrap_or_default(),
            )))
        }
        ("POST", "/v1/otter/relay/results") => {
            let submission =
                match serde_json::from_str::<TerminalRelayResultSubmission>(&request.body) {
                    Ok(submission) => submission,
                    Err(error) => return Some(invalid_request(error.to_string())),
                };
            match complete_relay_slot(
                submission,
                Some(control_plane_root),
                authenticated_device_id,
            ) {
                Ok(()) => Some(Ok((
                    200,
                    serde_json::to_string(&json!({ "accepted": true })).unwrap_or_default(),
                ))),
                Err(code) => Some(structured_error(
                    404,
                    json!({ "code": code, "message": "pending terminal relay not found" }),
                )),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "terminal_relay_tests.rs"]
mod tests;
