use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use loom_protocol::{
    validate_surface_protocol, SurfaceActionAck, SurfaceActionCancelRequest,
    SurfaceActionConcurrency, SurfaceActionDefinition, SurfaceActionInvocation,
    SurfaceActionProgress, SurfaceActionResponse, SurfaceActionStatus, SurfaceConfirmationDecision,
    SurfaceConfirmationRequest, SurfaceEvent, SurfaceExecutionError, SurfaceExecutionFailure,
    SurfaceInstanceMode, SurfacePatch, SurfacePreviewCommit, SurfaceResultCommit,
    SURFACE_EVENT_ACTION_ACK, SURFACE_EVENT_ACTION_PROGRESS, SURFACE_EVENT_CONFIRMATION_REQUEST,
    SURFACE_EVENT_FAILURE, SURFACE_EVENT_PATCH, SURFACE_EVENT_PREVIEW, SURFACE_EVENT_RESULT,
    SURFACE_PROTOCOL_VERSION,
};
use loom_tool_registry::{framework::FrameworkRegistry, ToolDefinition, ToolRegistry};
use loom_workflow_runtime::execute_tool_with_workflows_timeout;
use loom_workflow_store::WorkflowStore;
use serde_json::{json, Value};

use super::request_executor::{BoundedRequestExecutor, SubmitError};
use super::surface_resources::{SharedSurfaceResourceStore, SurfaceResourceStoreError};
use super::surface_store::{
    SharedSurfaceInstanceStore, SurfaceConfirmationResolution, SurfaceStoreError,
};
use super::{broadcast_hook_bridge_json, SharedHookBridgeRuntime, SharedMcpServerStore};

const SURFACE_ACTION_WORKERS: usize = 4;
const SURFACE_ACTION_QUEUE_CAPACITY: usize = 64;
const DEFAULT_SURFACE_ACTION_TIMEOUT_MILLIS: u64 = 30_000;
const MAX_SURFACE_ACTION_TIMEOUT_MILLIS: u64 = 120_000;
const SURFACE_ACTION_POLL_MILLIS: u64 = 20;

pub(crate) type SharedSurfaceActionExecutor = Arc<SurfaceActionExecutor>;

#[derive(Clone)]
struct SurfaceActionJob {
    event: SurfaceEvent,
    ack: SurfaceActionAck,
    action: SurfaceActionDefinition,
    tool: ToolDefinition,
    invocation: SurfaceActionInvocation,
    cancellation: Arc<AtomicBool>,
}

#[derive(Default)]
struct SurfaceActionCoordinator {
    latest_requests: BTreeMap<String, String>,
    reject_reservations: BTreeSet<String>,
    serial_locks: BTreeMap<String, Arc<Mutex<()>>>,
    cancellation_tokens: BTreeMap<String, Arc<AtomicBool>>,
}

type SurfaceActionRunner =
    dyn Fn(&SurfaceActionJob) -> Result<Value, SurfaceExecutionError> + Send + Sync + 'static;
type SurfaceToolResolver = dyn Fn(&loom_protocol::SurfaceInstanceDescriptor) -> Result<ToolDefinition, SurfaceStoreError>
    + Send
    + Sync
    + 'static;

pub(crate) struct SurfaceActionExecutor {
    queue: BoundedRequestExecutor<SurfaceActionJob>,
    coordinator: Arc<Mutex<SurfaceActionCoordinator>>,
    surface_instances: SharedSurfaceInstanceStore,
    tool_resolver: Arc<SurfaceToolResolver>,
    hook_bridge: SharedHookBridgeRuntime,
}

impl SurfaceActionExecutor {
    pub(crate) fn new(
        mcp_servers: SharedMcpServerStore,
        tool_registry: ToolRegistry,
        workflow_store: WorkflowStore,
        framework_registry: FrameworkRegistry,
        control_plane_root: PathBuf,
        surface_instances: SharedSurfaceInstanceStore,
        surface_resources: SharedSurfaceResourceStore,
        hook_bridge: SharedHookBridgeRuntime,
    ) -> std::io::Result<Self> {
        let runner_registry = tool_registry.clone();
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
            let servers = mcp_servers
                .lock()
                .map_err(|_| execution_error("mcp_store_unavailable", "MCP store is unavailable"))?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            let timeout_millis = job
                .action
                .timeout_ms
                .unwrap_or(DEFAULT_SURFACE_ACTION_TIMEOUT_MILLIS)
                .clamp(1, MAX_SURFACE_ACTION_TIMEOUT_MILLIS);
            let timeout = Duration::from_millis(timeout_millis);
            let arguments = json!({ "surfaceAction": &job.invocation });
            if matches!(
                &job.tool.execution,
                loom_tool_registry::ToolExecution::FrameworkArt { .. }
            ) {
                loom_tool_registry::execute_tool_with_timeout_and_cancellation(
                    &job.tool,
                    &servers,
                    arguments,
                    timeout,
                    job.cancellation.as_ref(),
                )
                .map_err(|error| {
                    execution_error("surface_action_execution_failed", error.to_string())
                })
            } else {
                execute_tool_with_workflows_timeout(
                    &job.tool,
                    &servers,
                    &workflow_store,
                    &runner_registry,
                    arguments,
                    timeout,
                )
                .map_err(|error| {
                    execution_error("surface_action_execution_failed", error.to_string())
                })
            }
        });
        let resolver_registry = tool_registry;
        let resolver: Arc<SurfaceToolResolver> = Arc::new(move |descriptor| {
            loom_tool_registry::install::resolve_installed_art_package(
                &control_plane_root,
                &descriptor.art_id,
                &descriptor.art_version,
                &descriptor.package_digest,
                &resolver_registry,
                &framework_registry,
            )
            .map_err(|error| SurfaceStoreError::Conflict(error.to_string()))
        });
        Self::new_with_components(
            resolver,
            surface_instances,
            surface_resources,
            hook_bridge,
            runner,
            SURFACE_ACTION_WORKERS,
            SURFACE_ACTION_QUEUE_CAPACITY,
        )
    }

    #[cfg(test)]
    fn new_with_runner(
        tool_registry: ToolRegistry,
        surface_instances: SharedSurfaceInstanceStore,
        surface_resources: SharedSurfaceResourceStore,
        hook_bridge: SharedHookBridgeRuntime,
        runner: Arc<SurfaceActionRunner>,
        workers: usize,
        queue_capacity: usize,
    ) -> std::io::Result<Self> {
        let resolver: Arc<SurfaceToolResolver> = Arc::new(move |descriptor| {
            let tool = tool_registry
                .get_tool(&descriptor.art_id)
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
                .ok_or_else(|| SurfaceStoreError::NotFound(descriptor.art_id.clone()))?;
            validate_locked_tool(descriptor, &tool)?;
            Ok(tool)
        });
        Self::new_with_components(
            resolver,
            surface_instances,
            surface_resources,
            hook_bridge,
            runner,
            workers,
            queue_capacity,
        )
    }

    fn new_with_components(
        tool_resolver: Arc<SurfaceToolResolver>,
        surface_instances: SharedSurfaceInstanceStore,
        surface_resources: SharedSurfaceResourceStore,
        hook_bridge: SharedHookBridgeRuntime,
        runner: Arc<SurfaceActionRunner>,
        workers: usize,
        queue_capacity: usize,
    ) -> std::io::Result<Self> {
        let coordinator = Arc::new(Mutex::new(SurfaceActionCoordinator::default()));
        let worker_coordinator = Arc::clone(&coordinator);
        let worker_instances = Arc::clone(&surface_instances);
        let worker_resources = Arc::clone(&surface_resources);
        let worker_bridge = Arc::clone(&hook_bridge);
        let queue = BoundedRequestExecutor::new(
            "loom-surface-action",
            workers,
            queue_capacity,
            move |job| {
                execute_surface_action_job(
                    job,
                    &worker_instances,
                    &worker_resources,
                    &worker_bridge,
                    &worker_coordinator,
                    &runner,
                );
            },
        )?;
        Ok(Self {
            queue,
            coordinator,
            surface_instances,
            tool_resolver,
            hook_bridge,
        })
    }

    pub(crate) fn submit(
        &self,
        instance_id: &str,
        event: SurfaceEvent,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        self.submit_internal(instance_id, event, false)
    }

    pub(crate) fn confirm(
        &self,
        decision: SurfaceConfirmationDecision,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        let resolution = self
            .surface_instances
            .lock()
            .map_err(|_| SurfaceStoreError::Conflict("Surface store is unavailable".into()))?
            .resolve_confirmation(decision)?;
        match resolution {
            SurfaceConfirmationResolution::Approved { event, ack } => {
                broadcast_ack(&self.hook_bridge, &ack);
                let instance_id = event.instance_id.clone();
                self.submit_internal(&instance_id, event, true)
            }
            SurfaceConfirmationResolution::Rejected { ack }
            | SurfaceConfirmationResolution::Expired { ack } => {
                broadcast_ack(&self.hook_bridge, &ack);
                Ok(ack)
            }
        }
    }

    pub(crate) fn cancel(
        &self,
        request: SurfaceActionCancelRequest,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        let (event, action) = {
            let store = self
                .surface_instances
                .lock()
                .map_err(|_| SurfaceStoreError::Conflict("Surface store is unavailable".into()))?;
            let instance = store
                .get(&request.instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(request.instance_id.clone()))?;
            let ack = instance
                .event_acks
                .values()
                .find(|ack| ack.request_id == request.request_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(request.request_id.clone()))?;
            let event = instance
                .pending_events
                .iter()
                .find(|event| event.event_id == ack.event_id)
                .cloned()
                .ok_or_else(|| {
                    SurfaceStoreError::Conflict(
                        "Surface action is no longer pending or running".to_owned(),
                    )
                })?;
            let tool = (self.tool_resolver)(&instance.descriptor)?;
            let action_id = event.action.as_deref().ok_or_else(|| {
                SurfaceStoreError::Invalid("Surface event has no declared action".into())
            })?;
            let action = tool
                .surface_manifest()
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
                .and_then(|manifest| {
                    manifest
                        .actions
                        .into_iter()
                        .find(|action| action.id == action_id)
                })
                .ok_or_else(|| {
                    SurfaceStoreError::Invalid(format!(
                        "Surface action {action_id} is not declared"
                    ))
                })?;
            (event, action)
        };
        if !action.cancelable {
            return Err(SurfaceStoreError::Conflict(format!(
                "Surface action {} is not cancelable",
                action.id
            )));
        }
        let (_, ack) = self
            .surface_instances
            .lock()
            .map_err(|_| SurfaceStoreError::Conflict("Surface store is unavailable".into()))?
            .request_cancel(request)?;
        if let Ok(state) = self.coordinator.lock() {
            if let Some(token) = state.cancellation_tokens.get(&ack.request_id) {
                token.store(true, Ordering::Release);
            }
        }
        debug_assert_eq!(event.event_id, ack.event_id);
        broadcast_ack(&self.hook_bridge, &ack);
        Ok(ack)
    }

    fn submit_internal(
        &self,
        instance_id: &str,
        event: SurfaceEvent,
        recovering: bool,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        let (tool, action, invocation, existing_ack, cancellation) = {
            let mut store = self
                .surface_instances
                .lock()
                .map_err(|_| SurfaceStoreError::Conflict("Surface store is unavailable".into()))?;
            let previous_ack = store.event_ack(instance_id, &event.event_id);
            if let Some(existing) = previous_ack.as_ref() {
                if !recovering
                    || !matches!(
                        &existing.status,
                        SurfaceActionStatus::Queued
                            | SurfaceActionStatus::Interrupted
                            | SurfaceActionStatus::CancelRequested
                    )
                {
                    return Ok(existing.clone());
                }
            }
            let instance = store
                .get(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            let tool = (self.tool_resolver)(&instance.descriptor)?;
            let manifest = tool
                .surface_manifest()
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
                .ok_or_else(|| SurfaceStoreError::Invalid("Art has no Surface manifest".into()))?;
            let action_id = event.action.as_deref().ok_or_else(|| {
                SurfaceStoreError::Invalid("Surface event has no declared action".into())
            })?;
            let action = manifest
                .actions
                .into_iter()
                .find(|action| action.id == action_id)
                .ok_or_else(|| {
                    SurfaceStoreError::Invalid(format!(
                        "Surface action {action_id} is not declared by the locked Art package"
                    ))
                })?;
            let already_confirmed = recovering
                && previous_ack
                    .as_ref()
                    .is_some_and(|ack| ack.status == SurfaceActionStatus::Queued);
            if action.confirmation && !already_confirmed {
                let (ack, confirmation) =
                    store.await_confirmation(instance_id, event.clone(), action.risk.clone())?;
                drop(store);
                broadcast_confirmation(&self.hook_bridge, &confirmation);
                broadcast_ack(&self.hook_bridge, &ack);
                return Ok(ack);
            }
            let cancellation = reserve_action(&self.coordinator, instance_id, &action, &event)?;
            let ack = match previous_ack {
                Some(ack) => ack,
                None => match store.accept_event(instance_id, event.clone()) {
                    Ok(ack) => ack,
                    Err(error) => {
                        let request_id = request_id_for_event(&event.event_id);
                        release_reservation(
                            &self.coordinator,
                            instance_id,
                            &action,
                            Some(&request_id),
                        );
                        return Err(error);
                    }
                },
            };
            if ack.status == SurfaceActionStatus::CancelRequested {
                cancellation.store(true, Ordering::Release);
            }
            let invocation = SurfaceActionInvocation {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.to_owned(),
                attachment_id: event.attachment_id.clone(),
                request_id: ack.request_id.clone(),
                event_id: event.event_id.clone(),
                action_id: action.id.clone(),
                event_class: event.class.clone(),
                generation: event.generation,
                base_revision: event.base_revision,
                payload: event.payload.clone(),
                authoritative_state: instance.authoritative_state,
            };
            (tool, action, invocation, ack, cancellation)
        };

        let job = SurfaceActionJob {
            event,
            ack: existing_ack.clone(),
            action: action.clone(),
            tool,
            invocation,
            cancellation,
        };
        match self.queue.try_submit(job) {
            Ok(()) => Ok(existing_ack),
            Err(SubmitError::Full(job)) | Err(SubmitError::Closed(job)) => {
                release_reservation(
                    &self.coordinator,
                    instance_id,
                    &job.action,
                    Some(&job.ack.request_id),
                );
                let error = execution_error(
                    "surface_action_queue_full",
                    "Surface action executor is unavailable or full",
                );
                let failed = SurfaceActionAck {
                    status: SurfaceActionStatus::Failed,
                    error: Some(error),
                    ..job.ack
                };
                persist_ack(&self.surface_instances, &failed, true);
                broadcast_ack(&self.hook_bridge, &failed);
                Err(SurfaceStoreError::Conflict(
                    "Surface action executor is unavailable or full".into(),
                ))
            }
        }
    }

    pub(crate) fn recover_pending(&self) {
        let expired = self
            .surface_instances
            .lock()
            .ok()
            .and_then(|mut store| store.expire_confirmations().ok())
            .unwrap_or_default();
        for ack in expired {
            broadcast_ack(&self.hook_bridge, &ack);
        }
        let pending = self
            .surface_instances
            .lock()
            .map(|store| store.pending_events())
            .unwrap_or_default();
        for event in pending {
            let instance_id = event.instance_id.clone();
            if self.submit_internal(&instance_id, event, true).is_err() {
                continue;
            }
        }
    }
}

fn broadcast_confirmation(
    hook_bridge: &SharedHookBridgeRuntime,
    confirmation: &SurfaceConfirmationRequest,
) {
    broadcast_hook_bridge_json(
        hook_bridge,
        json!({
            "method": SURFACE_EVENT_CONFIRMATION_REQUEST,
            "params": confirmation,
        }),
    );
}

#[cfg(test)]
fn validate_locked_tool(
    descriptor: &loom_protocol::SurfaceInstanceDescriptor,
    tool: &ToolDefinition,
) -> Result<(), SurfaceStoreError> {
    if super::art_version_from_tool(tool) != descriptor.art_version {
        return Err(SurfaceStoreError::Conflict(
            "Surface instance Art version is no longer active".into(),
        ));
    }
    let digest = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("digest"))
        .and_then(Value::as_str)
        .map(|digest| digest.strip_prefix("sha256:").unwrap_or(digest))
        .unwrap_or_default();
    if !digest.eq_ignore_ascii_case(&descriptor.package_digest) {
        return Err(SurfaceStoreError::Conflict(
            "Surface instance package digest is no longer active".into(),
        ));
    }
    Ok(())
}

fn action_key(instance_id: &str, action_id: &str) -> String {
    format!("{instance_id}:{action_id}")
}

fn reserve_action(
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
    instance_id: &str,
    action: &SurfaceActionDefinition,
    event: &SurfaceEvent,
) -> Result<Arc<AtomicBool>, SurfaceStoreError> {
    let key = action_key(instance_id, &action.id);
    let request_id = request_id_for_event(&event.event_id);
    let mut state = coordinator
        .lock()
        .map_err(|_| SurfaceStoreError::Conflict("Surface action coordinator failed".into()))?;
    match &action.concurrency {
        SurfaceActionConcurrency::ReplaceLatest | SurfaceActionConcurrency::Coalesce => {
            if let Some(previous) = state.latest_requests.insert(key, request_id.clone()) {
                if let Some(token) = state.cancellation_tokens.get(&previous) {
                    token.store(true, Ordering::Release);
                }
            }
        }
        SurfaceActionConcurrency::RejectWhileRunning => {
            if !state.reject_reservations.insert(key) {
                return Err(SurfaceStoreError::Conflict(format!(
                    "Surface action {} is already running",
                    action.id
                )));
            }
        }
        SurfaceActionConcurrency::Serial | SurfaceActionConcurrency::Parallel => {}
    }
    let cancellation = Arc::new(AtomicBool::new(false));
    state
        .cancellation_tokens
        .insert(request_id, Arc::clone(&cancellation));
    Ok(cancellation)
}

fn release_reservation(
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
    instance_id: &str,
    action: &SurfaceActionDefinition,
    request_id: Option<&str>,
) {
    let Ok(mut state) = coordinator.lock() else {
        return;
    };
    let key = action_key(instance_id, &action.id);
    if action.concurrency == SurfaceActionConcurrency::RejectWhileRunning {
        state.reject_reservations.remove(&key);
    }
    if matches!(
        &action.concurrency,
        SurfaceActionConcurrency::ReplaceLatest | SurfaceActionConcurrency::Coalesce
    ) && request_id.is_some_and(|request_id| {
        state
            .latest_requests
            .get(&key)
            .is_some_and(|latest| latest == request_id)
    }) {
        state.latest_requests.remove(&key);
    }
    if let Some(request_id) = request_id {
        state.cancellation_tokens.remove(request_id);
    }
}

fn serial_lock(
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
    job: &SurfaceActionJob,
) -> Option<Arc<Mutex<()>>> {
    if job.action.concurrency != SurfaceActionConcurrency::Serial {
        return None;
    }
    let mut state = coordinator.lock().ok()?;
    Some(Arc::clone(
        state
            .serial_locks
            .entry(action_key(&job.event.instance_id, &job.action.id))
            .or_insert_with(|| Arc::new(Mutex::new(()))),
    ))
}

fn is_latest(coordinator: &Arc<Mutex<SurfaceActionCoordinator>>, job: &SurfaceActionJob) -> bool {
    if !matches!(
        &job.action.concurrency,
        SurfaceActionConcurrency::ReplaceLatest | SurfaceActionConcurrency::Coalesce
    ) {
        return true;
    }
    coordinator
        .lock()
        .ok()
        .and_then(|state| {
            state
                .latest_requests
                .get(&action_key(&job.event.instance_id, &job.action.id))
                .cloned()
        })
        .is_some_and(|latest| latest == job.ack.request_id)
}

fn execute_surface_action_job(
    job: SurfaceActionJob,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
    hook_bridge: &SharedHookBridgeRuntime,
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
    runner: &Arc<SurfaceActionRunner>,
) {
    let serial = serial_lock(coordinator, &job);
    let _serial_guard = serial.as_ref().and_then(|lock| lock.lock().ok());
    if !is_latest(coordinator, &job) || job.cancellation.load(Ordering::Acquire) {
        finish_cancelled(&job, surface_instances, hook_bridge, coordinator);
        return;
    }

    let mut running = job.ack.clone();
    running.status = SurfaceActionStatus::Running;
    persist_ack(surface_instances, &running, false);
    broadcast_ack(hook_bridge, &running);
    broadcast_progress(hook_bridge, &job, Some(0.0), "running");

    let timeout_millis = job
        .action
        .timeout_ms
        .unwrap_or(DEFAULT_SURFACE_ACTION_TIMEOUT_MILLIS)
        .clamp(1, MAX_SURFACE_ACTION_TIMEOUT_MILLIS);
    let timeout = Duration::from_millis(timeout_millis);
    let deadline = Instant::now() + timeout;
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let runner_job = job.clone();
    let runner = Arc::clone(runner);
    let spawn = std::thread::Builder::new()
        .name(format!("loom-surface-runner-{}", job.ack.request_id))
        .spawn(move || {
            let _ = result_tx.send(runner(&runner_job));
        });
    let mut timed_out = false;
    let mut cancelled = false;
    let result = match spawn {
        Err(error) => Err(execution_error(
            "surface_action_runner_failed",
            format!("Surface action runner could not start: {error}"),
        )),
        Ok(_runner_thread) => loop {
            if job.cancellation.load(Ordering::Acquire) || !is_latest(coordinator, &job) {
                cancelled = true;
                break Err(execution_error(
                    "surface_action_cancelled",
                    "Surface action was cancelled",
                ));
            }
            let now = Instant::now();
            if now >= deadline {
                timed_out = true;
                job.cancellation.store(true, Ordering::Release);
                break Err(execution_error(
                    "surface_action_timeout",
                    format!("Surface action exceeded its {timeout_millis} ms budget"),
                ));
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(SURFACE_ACTION_POLL_MILLIS));
            match result_rx.recv_timeout(wait) {
                Ok(result) => break result,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    break Err(execution_error(
                        "surface_action_runner_failed",
                        "Surface action runner stopped without a result",
                    ))
                }
            }
        },
    };
    // The runner can observe cancellation and return before this worker's polling loop
    // observes the token. Re-check it after receiving the result so that a cooperative
    // runner cannot race a late success commit over an accepted cancellation.
    if cancelled
        || (!timed_out && job.cancellation.load(Ordering::Acquire))
        || !is_latest(coordinator, &job)
    {
        finish_cancelled(&job, surface_instances, hook_bridge, coordinator);
        return;
    }
    let result = result.and_then(parse_surface_action_response);

    match result.and_then(|response| {
        apply_action_response(
            &job,
            response,
            surface_instances,
            surface_resources,
            hook_bridge,
        )
    }) {
        Ok(()) => {
            let mut succeeded = job.ack.clone();
            succeeded.status = SurfaceActionStatus::Succeeded;
            persist_ack(surface_instances, &succeeded, true);
            broadcast_progress(hook_bridge, &job, Some(1.0), "succeeded");
            broadcast_ack(hook_bridge, &succeeded);
        }
        Err(error) => {
            let failure = SurfaceExecutionFailure {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: job.event.instance_id.clone(),
                request_id: job.ack.request_id.clone(),
                generation: job.event.generation,
                error: error.clone(),
                last_successful_result_revision: None,
            };
            if let Ok(mut store) = surface_instances.lock() {
                let _ = store.record_failure(&job.event.instance_id, failure.clone());
            }
            let mut failed = job.ack.clone();
            failed.status = SurfaceActionStatus::Failed;
            failed.error = Some(error);
            persist_ack(surface_instances, &failed, true);
            broadcast_ack(hook_bridge, &failed);
            broadcast_hook_bridge_json(
                hook_bridge,
                json!({
                    "method": SURFACE_EVENT_FAILURE,
                    "params": {
                        "hookNodeId": hook_node_id(surface_instances, &job.event.instance_id, &job.event.attachment_id),
                        "failure": failure,
                    }
                }),
            );
            if timed_out {
                broadcast_progress(hook_bridge, &job, None, "timeout");
            }
        }
    }
    release_reservation(
        coordinator,
        &job.event.instance_id,
        &job.action,
        Some(&job.ack.request_id),
    );
}

fn parse_surface_action_response(
    value: Value,
) -> Result<SurfaceActionResponse, SurfaceExecutionError> {
    let payload = value.get("surfaceAction").cloned().ok_or_else(|| {
        execution_error(
            "surface_action_response_missing",
            "Art output has no surfaceAction response",
        )
    })?;
    let response = serde_json::from_value::<SurfaceActionResponse>(payload).map_err(|error| {
        execution_error(
            "surface_action_response_invalid",
            format!("Surface action response is invalid: {error}"),
        )
    })?;
    validate_surface_protocol(&response.protocol_version)
        .map_err(|error| execution_error("surface_action_protocol_invalid", error.to_string()))?;
    Ok(response)
}

fn apply_action_response(
    job: &SurfaceActionJob,
    response: SurfaceActionResponse,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(), SurfaceExecutionError> {
    let response = broker_action_resource_uploads(response, surface_resources)?;
    validate_action_response_resources(&response, surface_resources)?;
    for update in response.patches {
        let target_attachments = {
            let store = surface_instances.lock().map_err(|_| {
                execution_error("surface_store_unavailable", "Surface store is unavailable")
            })?;
            let instance = store.get(&job.event.instance_id).ok_or_else(|| {
                execution_error("surface_instance_missing", "Surface instance was removed")
            })?;
            if let Some(attachment_id) = update.attachment_id.as_ref() {
                vec![attachment_id.clone()]
            } else if instance.descriptor.instance_mode == SurfaceInstanceMode::Shared {
                instance
                    .attachments
                    .values()
                    .filter(|attachment| attachment.snapshot.is_some())
                    .map(|attachment| attachment.descriptor.attachment_id.clone())
                    .collect::<Vec<_>>()
            } else {
                vec![job.event.attachment_id.clone()]
            }
        };
        for (target_index, target_attachment) in target_attachments.into_iter().enumerate() {
            let mut target_update = update.clone();
            if target_index > 0 && !target_update.resource_leases.is_empty() {
                let mut resources = surface_resources.lock().map_err(|_| {
                    execution_error(
                        "surface_resource_store_unavailable",
                        "Surface resource store is unavailable",
                    )
                })?;
                target_update.resource_leases = target_update
                    .resource_leases
                    .iter()
                    .map(|lease| resources.duplicate_loom_resource_lease(lease))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(resource_execution_error)?;
            }
            let (patch, hook_node_id) = {
                let mut store = surface_instances.lock().map_err(|_| {
                    execution_error("surface_store_unavailable", "Surface store is unavailable")
                })?;
                let instance = store.get(&job.event.instance_id).ok_or_else(|| {
                    execution_error("surface_instance_missing", "Surface instance was removed")
                })?;
                if instance.descriptor.generation != job.event.generation {
                    return Err(execution_error(
                        "surface_action_stale_generation",
                        "Surface action completed for a stale generation",
                    ));
                }
                let attachment = instance
                    .attachments
                    .get(&target_attachment)
                    .ok_or_else(|| {
                        execution_error(
                            "surface_attachment_missing",
                            "Surface attachment was removed",
                        )
                    })?;
                let base_revision = attachment
                    .snapshot
                    .as_ref()
                    .ok_or_else(|| {
                        execution_error("surface_snapshot_missing", "Surface is not mounted")
                    })?
                    .revision;
                let patch = SurfacePatch {
                    protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                    instance_id: job.event.instance_id.clone(),
                    attachment_id: target_attachment.clone(),
                    base_revision,
                    revision: base_revision.saturating_add(1),
                    operations: target_update.operations,
                    state_patch: target_update.state_patch,
                    resources: target_update.resources,
                    resource_leases: target_update.resource_leases,
                };
                let hook_node_id = attachment.descriptor.hook_node_id.clone();
                store
                    .apply_patch(&job.event.instance_id, patch.clone())
                    .map_err(store_execution_error)?;
                (patch, hook_node_id)
            };
            broadcast_hook_bridge_json(
                hook_bridge,
                json!({
                    "method": SURFACE_EVENT_PATCH,
                    "params": {
                        "hookNodeId": hook_node_id,
                        "patch": patch,
                        "generation": job.event.generation,
                    }
                }),
            );
        }
    }

    if let Some(preview) = response.preview {
        let (commit, hook_nodes) = {
            let mut store = surface_instances.lock().map_err(|_| {
                execution_error("surface_store_unavailable", "Surface store is unavailable")
            })?;
            let instance = store.get(&job.event.instance_id).ok_or_else(|| {
                execution_error("surface_instance_missing", "Surface instance was removed")
            })?;
            let commit = SurfacePreviewCommit {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: job.event.instance_id.clone(),
                request_id: job.ack.request_id.clone(),
                generation: job.event.generation,
                preview_revision: instance.descriptor.preview_revision.saturating_add(1),
                port_id: preview.port_id,
                value: preview.value,
            };
            let hook_nodes = instance
                .attachments
                .values()
                .map(|attachment| attachment.descriptor.hook_node_id.clone())
                .collect::<Vec<_>>();
            store
                .commit_preview(&job.event.instance_id, commit.clone())
                .map_err(store_execution_error)?;
            (commit, hook_nodes)
        };
        for hook_node_id in hook_nodes {
            broadcast_hook_bridge_json(
                hook_bridge,
                json!({
                    "method": SURFACE_EVENT_PREVIEW,
                    "params": { "hookNodeId": hook_node_id, "commit": &commit }
                }),
            );
        }
    }

    if let Some(result) = response.result {
        let (commit, hook_nodes) = {
            let mut store = surface_instances.lock().map_err(|_| {
                execution_error("surface_store_unavailable", "Surface store is unavailable")
            })?;
            let instance = store.get(&job.event.instance_id).ok_or_else(|| {
                execution_error("surface_instance_missing", "Surface instance was removed")
            })?;
            let commit = SurfaceResultCommit {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: job.event.instance_id.clone(),
                request_id: job.ack.request_id.clone(),
                generation: job.event.generation,
                result_revision: instance.descriptor.result_revision.saturating_add(1),
                outputs: result.outputs,
                state_patch: result.state_patch,
            };
            let hook_nodes = instance
                .attachments
                .values()
                .map(|attachment| attachment.descriptor.hook_node_id.clone())
                .collect::<Vec<_>>();
            store
                .commit_result(&job.event.instance_id, commit.clone())
                .map_err(store_execution_error)?;
            (commit, hook_nodes)
        };
        for hook_node_id in hook_nodes {
            broadcast_hook_bridge_json(
                hook_bridge,
                json!({
                    "method": SURFACE_EVENT_RESULT,
                    "params": { "hookNodeId": hook_node_id, "commit": &commit }
                }),
            );
        }
    }
    Ok(())
}

fn broker_action_resource_uploads(
    mut response: SurfaceActionResponse,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<SurfaceActionResponse, SurfaceExecutionError> {
    let uploads = std::mem::take(&mut response.resource_uploads);
    if uploads.is_empty() {
        return Ok(response);
    }
    if uploads.len() > 32 {
        return Err(execution_error(
            "surface_resource_upload_limit",
            "Surface action returned more than 32 resource uploads",
        ));
    }
    if response.patches.is_empty() {
        return Err(execution_error(
            "surface_resource_patch_required",
            "Surface action resource uploads require at least one patch",
        ));
    }
    let mut aliases = BTreeMap::new();
    let mut leases = Vec::new();
    let mut total_bytes = 0_usize;
    let mut store = surface_resources.lock().map_err(|_| {
        execution_error(
            "surface_resource_store_unavailable",
            "Surface resource store is unavailable",
        )
    })?;
    for upload in uploads {
        if upload.id.is_empty()
            || upload.id.len() > 160
            || !upload
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(execution_error(
                "surface_resource_upload_id_invalid",
                format!("Surface resource upload id `{}` is invalid", upload.id),
            ));
        }
        let alias = format!("surface-upload:{}", upload.id);
        if aliases.contains_key(&alias) {
            return Err(execution_error(
                "surface_resource_upload_duplicate",
                format!("Surface resource upload `{}` is duplicated", upload.id),
            ));
        }
        let bytes = BASE64.decode(upload.data_base64.trim()).map_err(|_| {
            execution_error(
                "surface_resource_upload_base64_invalid",
                format!(
                    "Surface resource upload `{}` is not valid Base64",
                    upload.id
                ),
            )
        })?;
        total_bytes = total_bytes.checked_add(bytes.len()).ok_or_else(|| {
            execution_error(
                "surface_resource_upload_limit",
                "Surface resource upload size overflowed",
            )
        })?;
        if total_bytes > super::surface_resources::MAX_SURFACE_RESOURCE_BYTES {
            return Err(execution_error(
                "surface_resource_upload_limit",
                "Surface action resource uploads exceed the 16 MiB request budget",
            ));
        }
        let lease = store
            .register(
                upload.kind,
                &upload.mime,
                &bytes,
                upload.width,
                upload.height,
                upload.lease_millis,
            )
            .map_err(resource_execution_error)?;
        aliases.insert(alias, lease.resource.resource_id.clone());
        leases.push(lease);
    }
    drop(store);

    let mut value = serde_json::to_value(&response).map_err(|error| {
        execution_error(
            "surface_resource_upload_resolution_failed",
            format!("serialize Surface action response: {error}"),
        )
    })?;
    replace_surface_resource_aliases(&mut value, &aliases);
    let mut response = serde_json::from_value::<SurfaceActionResponse>(value).map_err(|error| {
        execution_error(
            "surface_resource_upload_resolution_failed",
            format!("deserialize Surface action response: {error}"),
        )
    })?;
    let first_patch = response
        .patches
        .first_mut()
        .expect("resource uploads require a patch");
    for lease in leases {
        first_patch.resources.push(lease.resource.clone());
        first_patch.resource_leases.push(lease);
    }
    Ok(response)
}

fn replace_surface_resource_aliases(value: &mut Value, aliases: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(resource_id) = aliases.get(text) {
                *text = resource_id.clone();
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_surface_resource_aliases(value, aliases);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                replace_surface_resource_aliases(value, aliases);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn validate_action_response_resources(
    response: &SurfaceActionResponse,
    surface_resources: &SharedSurfaceResourceStore,
) -> Result<(), SurfaceExecutionError> {
    let mut store = surface_resources.lock().map_err(|_| {
        execution_error(
            "surface_resource_store_unavailable",
            "Surface resource store is unavailable",
        )
    })?;
    for update in &response.patches {
        store
            .validate_references(&update.resources, &update.resource_leases)
            .map_err(resource_execution_error)?;
    }
    if let Some(preview) = &response.preview {
        if let loom_protocol::SurfacePortValue::Resource { resource } = &preview.value {
            store
                .validate_descriptor(resource)
                .map_err(resource_execution_error)?;
        }
    }
    if let Some(result) = &response.result {
        for output in result.outputs.values() {
            if let loom_protocol::SurfacePortValue::Resource { resource } = output {
                store
                    .validate_descriptor(resource)
                    .map_err(resource_execution_error)?;
            }
        }
    }
    Ok(())
}

fn request_id_for_event(event_id: &str) -> String {
    format!(
        "request:{}",
        event_id.strip_prefix("event:").unwrap_or(event_id)
    )
}

fn finish_cancelled(
    job: &SurfaceActionJob,
    surface_instances: &SharedSurfaceInstanceStore,
    hook_bridge: &SharedHookBridgeRuntime,
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
) {
    let mut cancelled = job.ack.clone();
    cancelled.status = SurfaceActionStatus::Cancelled;
    persist_ack(surface_instances, &cancelled, true);
    broadcast_ack(hook_bridge, &cancelled);
    release_reservation(
        coordinator,
        &job.event.instance_id,
        &job.action,
        Some(&job.ack.request_id),
    );
}

fn persist_ack(
    surface_instances: &SharedSurfaceInstanceStore,
    ack: &SurfaceActionAck,
    remove_pending: bool,
) {
    if let Ok(mut store) = surface_instances.lock() {
        let _ = store.update_event_ack(ack.clone(), remove_pending);
    }
}

fn broadcast_ack(hook_bridge: &SharedHookBridgeRuntime, ack: &SurfaceActionAck) {
    broadcast_hook_bridge_json(
        hook_bridge,
        json!({ "method": SURFACE_EVENT_ACTION_ACK, "params": ack }),
    );
}

fn broadcast_progress(
    hook_bridge: &SharedHookBridgeRuntime,
    job: &SurfaceActionJob,
    value: Option<f64>,
    stage: &str,
) {
    if !job.action.progress {
        return;
    }
    let progress = SurfaceActionProgress {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: job.event.instance_id.clone(),
        request_id: job.ack.request_id.clone(),
        generation: job.event.generation,
        value,
        stage: Some(stage.to_owned()),
        message_key: None,
    };
    broadcast_hook_bridge_json(
        hook_bridge,
        json!({ "method": SURFACE_EVENT_ACTION_PROGRESS, "params": progress }),
    );
}

fn hook_node_id(
    surface_instances: &SharedSurfaceInstanceStore,
    instance_id: &str,
    attachment_id: &str,
) -> Option<String> {
    surface_instances
        .lock()
        .ok()
        .and_then(|store| store.get(instance_id))
        .and_then(|instance| {
            instance
                .attachments
                .get(attachment_id)
                .map(|attachment| attachment.descriptor.hook_node_id.clone())
        })
}

fn store_execution_error(error: SurfaceStoreError) -> SurfaceExecutionError {
    SurfaceExecutionError {
        code: error.code().to_owned(),
        message: error.to_string(),
        detail: None,
    }
}

fn resource_execution_error(error: SurfaceResourceStoreError) -> SurfaceExecutionError {
    SurfaceExecutionError {
        code: error.code().to_owned(),
        message: error.to_string(),
        detail: None,
    }
}

fn execution_error(code: impl Into<String>, message: impl Into<String>) -> SurfaceExecutionError {
    SurfaceExecutionError {
        code: code.into(),
        message: message.into(),
        detail: None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use loom_protocol::{
        SurfaceEventClass, SurfaceHostCapabilities, SurfaceInstancePersistence, SurfaceNode,
        SurfaceRuntimeKind, SurfaceSnapshot,
    };
    use loom_tool_registry::{ToolDefinition, ToolExecution};

    use super::*;
    use crate::surface_resources::SurfaceResourceStore;
    use crate::surface_store::SurfaceInstanceStore;
    use crate::{register_hook_bridge_subscription, HookBridgeRuntime};

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "loom-surface-actions-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn surface_tool(digest: &str) -> ToolDefinition {
        ToolDefinition {
            id: "surface-action-test".to_owned(),
            name: "Surface Action Test".to_owned(),
            description: "Surface action executor fixture".to_owned(),
            enabled: true,
            execution: ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
            inputs: Vec::new(),
            outputs: Vec::new(),
            params: Vec::new(),
            metadata: Some(json!({
                "dependencies": { "framework": "process" },
                "packageSecurity": { "version": "1.0.0" },
                "artPackage": {
                    "version": "1.0.0",
                    "digest": digest,
                    "dir": "unused"
                },
                "capabilities": {
                    "surface": {
                        "protocolVersion": SURFACE_PROTOCOL_VERSION,
                        "apiVersion": "1.0",
                        "variants": [{
                            "runtime": "declarative",
                            "entry": "surface/main.json"
                        }],
                        "requiredNodes": ["column", "text", "button"],
                        "actions": [{
                            "id": "refresh_price",
                            "risk": "low",
                            "offlinePolicy": "reject",
                            "concurrency": "serial",
                            "idempotent": true,
                            "confirmation": false,
                            "cancelable": false,
                            "timeoutMs": 5000,
                            "progress": true
                        }]
                    }
                }
            })),
        }
    }

    fn host_capabilities() -> SurfaceHostCapabilities {
        SurfaceHostCapabilities {
            api_version: "1.0".to_owned(),
            runtimes: vec![SurfaceRuntimeKind::Declarative],
            nodes: vec!["column".to_owned(), "text".to_owned(), "button".to_owned()],
            transports: Vec::new(),
            capabilities: Vec::new(),
            input: Default::default(),
        }
    }

    fn setup_action_fixture(
        root: &Path,
        tool: ToolDefinition,
        hook_node_id: &str,
    ) -> (
        ToolRegistry,
        SharedSurfaceInstanceStore,
        SharedSurfaceResourceStore,
        SharedHookBridgeRuntime,
        String,
        String,
    ) {
        let digest = tool
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/artPackage/digest"))
            .and_then(Value::as_str)
            .expect("fixture package digest")
            .to_owned();
        let tool_registry = ToolRegistry::new(root.join("tools"));
        tool_registry
            .save_tool(tool)
            .expect("save Surface fixture tool");
        let instances = Arc::new(Mutex::new(
            SurfaceInstanceStore::new(root.join("surface-instances.json"))
                .expect("open Surface store"),
        ));
        let (instance_id, attachment_id) = {
            let mut store = instances.lock().expect("lock Surface store");
            let instance = store
                .create(
                    "surface-action-test",
                    "1.0.0",
                    &digest,
                    1,
                    SurfaceInstancePersistence::Persistent,
                    loom_protocol::SurfaceInstanceMode::Independent,
                )
                .expect("create instance");
            let attachment = store
                .attach(
                    &instance.descriptor.instance_id,
                    hook_node_id,
                    "device-000-local",
                    Some(host_capabilities()),
                )
                .expect("attach Surface instance");
            store
                .put_snapshot(
                    &instance.descriptor.instance_id,
                    SurfaceSnapshot {
                        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                        instance_id: instance.descriptor.instance_id.clone(),
                        attachment_id: attachment.descriptor.attachment_id.clone(),
                        art_id: instance.descriptor.art_id,
                        art_version: instance.descriptor.art_version,
                        revision: 1,
                        runtime: SurfaceRuntimeKind::Declarative,
                        entry_resource_id: None,
                        view_id: None,
                        scene: SurfaceNode {
                            id: "root".to_owned(),
                            node_type: "column".to_owned(),
                            children: vec![SurfaceNode {
                                id: "refresh".to_owned(),
                                node_type: "button".to_owned(),
                                events: BTreeMap::from([(
                                    "click".to_owned(),
                                    "refresh_price".to_owned(),
                                )]),
                                ..SurfaceNode::default()
                            }],
                            ..SurfaceNode::default()
                        },
                        authoritative_state: json!({"value": 0}),
                        resources: Vec::new(),
                        resource_leases: Vec::new(),
                    },
                )
                .expect("mount Surface snapshot");
            (
                instance.descriptor.instance_id,
                attachment.descriptor.attachment_id,
            )
        };
        let resources = Arc::new(Mutex::new(
            SurfaceResourceStore::new(root.join("surface-resources"))
                .expect("open Surface resource store"),
        ));
        let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
        (
            tool_registry,
            instances,
            resources,
            hook_bridge,
            instance_id,
            attachment_id,
        )
    }

    fn fixture_event(
        instance_id: &str,
        attachment_id: &str,
        event_id: &str,
        payload: Value,
    ) -> SurfaceEvent {
        SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.to_owned(),
            attachment_id: attachment_id.to_owned(),
            event_id: event_id.to_owned(),
            node_id: "refresh".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh_price".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload,
        }
    }

    #[test]
    fn declared_action_executes_once_and_commits_patch_and_formal_result() {
        let root = temp_root("commit");
        let digest = "a".repeat(64);
        let tool_registry = ToolRegistry::new(root.join("tools"));
        tool_registry
            .save_tool(surface_tool(&digest))
            .expect("save Surface tool");
        let instances = Arc::new(Mutex::new(
            SurfaceInstanceStore::new(root.join("surface-instances.json"))
                .expect("open Surface store"),
        ));
        let (instance_id, attachment_id) = {
            let mut store = instances.lock().expect("lock Surface store");
            let instance = store
                .create(
                    "surface-action-test",
                    "1.0.0",
                    &digest,
                    1,
                    SurfaceInstancePersistence::Persistent,
                    loom_protocol::SurfaceInstanceMode::Independent,
                )
                .expect("create instance");
            let attachment = store
                .attach(
                    &instance.descriptor.instance_id,
                    "hook-node:test",
                    "device-000-local",
                    Some(host_capabilities()),
                )
                .expect("attach");
            store
                .put_snapshot(
                    &instance.descriptor.instance_id,
                    SurfaceSnapshot {
                        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                        instance_id: instance.descriptor.instance_id.clone(),
                        attachment_id: attachment.descriptor.attachment_id.clone(),
                        art_id: instance.descriptor.art_id,
                        art_version: instance.descriptor.art_version,
                        revision: 1,
                        runtime: loom_protocol::SurfaceRuntimeKind::Declarative,
                        entry_resource_id: None,
                        view_id: None,
                        scene: SurfaceNode {
                            id: "root".to_owned(),
                            node_type: "column".to_owned(),
                            children: vec![
                                SurfaceNode {
                                    id: "price".to_owned(),
                                    node_type: "text".to_owned(),
                                    props: json!({"text": "100"}),
                                    ..SurfaceNode::default()
                                },
                                SurfaceNode {
                                    id: "refresh".to_owned(),
                                    node_type: "button".to_owned(),
                                    events: BTreeMap::from([(
                                        "click".to_owned(),
                                        "refresh_price".to_owned(),
                                    )]),
                                    ..SurfaceNode::default()
                                },
                            ],
                            ..SurfaceNode::default()
                        },
                        authoritative_state: json!({"price": 100}),
                        resources: Vec::new(),
                        resource_leases: Vec::new(),
                    },
                )
                .expect("mount snapshot");
            (
                instance.descriptor.instance_id,
                attachment.descriptor.attachment_id,
            )
        };
        let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
        let (rx, _subscription) = register_hook_bridge_subscription(
            &hook_bridge.lock().expect("lock bridge").broadcast_hub,
            loom_protocol::SURFACE_EVENT_METHODS
                .iter()
                .map(|method| (*method).to_owned())
                .collect(),
        );
        let executions = Arc::new(AtomicUsize::new(0));
        let runner_executions = Arc::clone(&executions);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
            runner_executions.fetch_add(1, Ordering::SeqCst);
            assert_eq!(job.invocation.action_id, "refresh_price");
            assert_eq!(job.invocation.authoritative_state["price"], 100);
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "patches": [{
                        "operations": [{
                            "op": "set",
                            "nodeId": "price",
                            "path": "/props/text",
                            "value": "101"
                        }],
                        "statePatch": {"price": 101}
                    }],
                    "result": {
                        "outputs": {
                            "price": {"kind": "value", "value": 101}
                        },
                        "statePatch": {"price": 101}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            tool_registry,
            Arc::clone(&instances),
            Arc::new(Mutex::new(
                SurfaceResourceStore::new(root.join("surface-resources"))
                    .expect("open Surface resource store"),
            )),
            Arc::clone(&hook_bridge),
            runner,
            1,
            4,
        )
        .expect("start executor");
        let event = SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            attachment_id,
            event_id: "event:refresh-1".to_owned(),
            node_id: "refresh".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh_price".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload: json!({}),
        };
        let queued = executor
            .submit(&instance_id, event.clone())
            .expect("queue action");
        assert_eq!(queued.status, SurfaceActionStatus::Queued);

        let mut saw_patch = false;
        let mut saw_result = false;
        let mut saw_success = false;
        for _ in 0..8 {
            let message = rx
                .recv_timeout(Duration::from_secs(2))
                .expect("Surface action broadcast");
            let message: Value = serde_json::from_str(&message).expect("broadcast JSON");
            match message["method"].as_str() {
                Some(SURFACE_EVENT_PATCH) => saw_patch = true,
                Some(SURFACE_EVENT_RESULT) => saw_result = true,
                Some(SURFACE_EVENT_ACTION_ACK) if message["params"]["status"] == "succeeded" => {
                    saw_success = true;
                }
                _ => {}
            }
            if saw_patch && saw_result && saw_success {
                break;
            }
        }
        assert!(saw_patch && saw_result && saw_success);
        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("instance record");
        assert!(record.pending_events.is_empty());
        assert_eq!(record.authoritative_state["price"], 101);
        assert_eq!(
            record
                .attachments
                .values()
                .next()
                .and_then(|attachment| attachment.snapshot.as_ref())
                .expect("snapshot")
                .scene
                .children[0]
                .props["text"],
            "101"
        );
        assert_eq!(
            record.latest_result.expect("formal result").outputs["price"],
            loom_protocol::SurfacePortValue::Value { value: json!(101) }
        );

        let duplicate = executor
            .submit(&instance_id, event)
            .expect("deduplicate completed action");
        assert_eq!(duplicate.status, SurfaceActionStatus::Succeeded);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn shared_instance_patch_fans_out_to_every_mounted_attachment() {
        let root = temp_root("shared-fanout");
        let digest = "f".repeat(64);
        let tool = surface_tool(&digest);
        let instances = Arc::new(Mutex::new(
            SurfaceInstanceStore::new(root.join("surface-instances.json"))
                .expect("open Surface store"),
        ));
        let (instance_id, attachment_ids) = {
            let mut store = instances.lock().expect("lock Surface store");
            let instance = store
                .create(
                    "surface-action-test",
                    "1.0.0",
                    &digest,
                    1,
                    SurfaceInstancePersistence::Persistent,
                    SurfaceInstanceMode::Shared,
                )
                .expect("create shared instance");
            let mut attachment_ids = Vec::new();
            for suffix in ["one", "two"] {
                let attachment = store
                    .attach(
                        &instance.descriptor.instance_id,
                        &format!("hook-node:{suffix}"),
                        "device-000-local",
                        Some(host_capabilities()),
                    )
                    .expect("attach shared Surface");
                store
                    .put_snapshot(
                        &instance.descriptor.instance_id,
                        SurfaceSnapshot {
                            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                            instance_id: instance.descriptor.instance_id.clone(),
                            attachment_id: attachment.descriptor.attachment_id.clone(),
                            art_id: instance.descriptor.art_id.clone(),
                            art_version: instance.descriptor.art_version.clone(),
                            revision: 1,
                            runtime: SurfaceRuntimeKind::Declarative,
                            entry_resource_id: None,
                            view_id: None,
                            scene: SurfaceNode {
                                id: "root".to_owned(),
                                node_type: "column".to_owned(),
                                children: vec![SurfaceNode {
                                    id: "status".to_owned(),
                                    node_type: "text".to_owned(),
                                    props: json!({"text": "idle"}),
                                    ..SurfaceNode::default()
                                }],
                                ..SurfaceNode::default()
                            },
                            authoritative_state: json!({"status": "idle"}),
                            resources: Vec::new(),
                            resource_leases: Vec::new(),
                        },
                    )
                    .expect("mount shared snapshot");
                attachment_ids.push(attachment.descriptor.attachment_id);
            }
            (instance.descriptor.instance_id, attachment_ids)
        };
        let resources = Arc::new(Mutex::new(
            SurfaceResourceStore::new(root.join("surface-resources"))
                .expect("open Surface resources"),
        ));
        let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
        let (rx, _subscription) = register_hook_bridge_subscription(
            &hook_bridge.lock().expect("lock bridge").broadcast_hub,
            loom_protocol::SURFACE_EVENT_METHODS
                .iter()
                .map(|method| (*method).to_owned())
                .collect(),
        );
        let event = fixture_event(
            &instance_id,
            &attachment_ids[0],
            "event:shared-refresh",
            Value::Null,
        );
        let action = tool
            .surface_manifest()
            .expect("parse Surface manifest")
            .expect("Surface manifest")
            .actions
            .into_iter()
            .find(|action| action.id == "refresh_price")
            .expect("refresh action");
        let job = SurfaceActionJob {
            invocation: SurfaceActionInvocation {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                attachment_id: attachment_ids[0].clone(),
                request_id: "request:shared-refresh".to_owned(),
                event_id: event.event_id.clone(),
                action_id: action.id.clone(),
                event_class: event.class.clone(),
                generation: 0,
                base_revision: 1,
                payload: Value::Null,
                authoritative_state: json!({"status": "idle"}),
            },
            ack: SurfaceActionAck {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                event_id: event.event_id.clone(),
                request_id: "request:shared-refresh".to_owned(),
                accepted: true,
                status: SurfaceActionStatus::Running,
                error: None,
            },
            event,
            action,
            tool,
            cancellation: Arc::new(AtomicBool::new(false)),
        };
        let response = serde_json::from_value::<SurfaceActionResponse>(json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "patches": [{
                "operations": [{
                    "op": "set",
                    "nodeId": "status",
                    "path": "/props/text",
                    "value": "ready"
                }],
                "statePatch": {"status": "ready"}
            }]
        }))
        .expect("shared action response");
        apply_action_response(&job, response, &instances, &resources, &hook_bridge)
            .expect("apply shared action response");

        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("shared instance");
        for attachment_id in &attachment_ids {
            let snapshot = record.attachments[attachment_id]
                .snapshot
                .as_ref()
                .expect("shared snapshot");
            assert_eq!(snapshot.revision, 2);
            assert_eq!(snapshot.scene.children[0].props["text"], "ready");
        }
        let mut hook_nodes = BTreeSet::new();
        for _ in 0..2 {
            let message: Value = serde_json::from_str(
                &rx.recv_timeout(Duration::from_secs(1))
                    .expect("shared patch broadcast"),
            )
            .expect("shared patch JSON");
            assert_eq!(message["method"], SURFACE_EVENT_PATCH);
            hook_nodes.insert(
                message["params"]["hookNodeId"]
                    .as_str()
                    .expect("Hook node id")
                    .to_owned(),
            );
        }
        assert_eq!(
            hook_nodes,
            BTreeSet::from(["hook-node:one".to_owned(), "hook-node:two".to_owned()])
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn action_response_accepts_only_host_brokered_resource_references() {
        let root = temp_root("resource-broker");
        let resources = Arc::new(Mutex::new(
            SurfaceResourceStore::new(root.join("surface-resources"))
                .expect("open Surface resource store"),
        ));
        let forged: SurfaceActionResponse = serde_json::from_value(json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "patches": [{
                "resources": [{
                    "resourceId": format!("sha256:{}", "a".repeat(64)),
                    "kind": "image",
                    "mime": "image/png",
                    "size": 4
                }]
            }]
        }))
        .expect("parse forged action response");
        let error = validate_action_response_resources(&forged, &resources)
            .expect_err("reject resource not registered by Loom");
        assert_eq!(error.code, "surface_resource_not_found");

        let lease = resources
            .lock()
            .expect("lock Surface resource store")
            .register(
                loom_protocol::SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"brokered",
                None,
                None,
                None,
            )
            .expect("register brokered resource");
        let accepted: SurfaceActionResponse = serde_json::from_value(json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "patches": [{
                "resources": [lease.resource],
                "resourceLeases": [lease]
            }]
        }))
        .expect("parse brokered action response");
        validate_action_response_resources(&accepted, &resources)
            .expect("accept Loom-issued resource descriptor and lease");

        let upload: SurfaceActionResponse = serde_json::from_value(json!({
            "protocolVersion": SURFACE_PROTOCOL_VERSION,
            "resourceUploads": [{
                "id": "chart",
                "kind": "image",
                "mime": "image/png",
                "dataBase64": BASE64.encode(b"prototype-image"),
                "width": 1,
                "height": 1
            }],
            "patches": [{
                "operations": [{
                    "op": "set",
                    "nodeId": "chart",
                    "path": "/props/resourceId",
                    "value": "surface-upload:chart"
                }]
            }]
        }))
        .expect("parse resource upload response");
        let brokered = broker_action_resource_uploads(upload, &resources)
            .expect("broker package resource upload");
        assert_eq!(brokered.patches[0].resources.len(), 1);
        assert_eq!(brokered.patches[0].resource_leases.len(), 1);
        let brokered_json = serde_json::to_value(&brokered).expect("brokered response JSON");
        let resource_id = brokered_json["patches"][0]["operations"][0]["value"]
            .as_str()
            .expect("resolved resource id");
        assert!(resource_id.starts_with("sha256:"));
        assert_ne!(resource_id, "surface-upload:chart");
        validate_action_response_resources(&brokered, &resources)
            .expect("accept brokered resource upload response");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn confirmed_action_never_executes_before_device_bound_host_approval() {
        let root = temp_root("confirmation");
        let digest = "b".repeat(64);
        let tool_registry = ToolRegistry::new(root.join("tools"));
        let mut tool = surface_tool(&digest);
        let metadata = tool.metadata.as_mut().expect("Surface metadata");
        metadata["capabilities"]["surface"]["actions"][0]["confirmation"] = json!(true);
        metadata["capabilities"]["surface"]["actions"][0]["risk"] = json!("high");
        tool_registry
            .save_tool(tool)
            .expect("save confirmed Surface tool");
        let instances = Arc::new(Mutex::new(
            SurfaceInstanceStore::new(root.join("surface-instances.json"))
                .expect("open Surface store"),
        ));
        let (instance_id, attachment_id) = {
            let mut store = instances.lock().expect("lock Surface store");
            let instance = store
                .create(
                    "surface-action-test",
                    "1.0.0",
                    &digest,
                    1,
                    SurfaceInstancePersistence::Persistent,
                    loom_protocol::SurfaceInstanceMode::Independent,
                )
                .expect("create instance");
            let attachment = store
                .attach(
                    &instance.descriptor.instance_id,
                    "hook-node:confirmation",
                    "device-000-local",
                    Some(host_capabilities()),
                )
                .expect("attach");
            store
                .put_snapshot(
                    &instance.descriptor.instance_id,
                    SurfaceSnapshot {
                        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                        instance_id: instance.descriptor.instance_id.clone(),
                        attachment_id: attachment.descriptor.attachment_id.clone(),
                        art_id: instance.descriptor.art_id,
                        art_version: instance.descriptor.art_version,
                        revision: 1,
                        runtime: SurfaceRuntimeKind::Declarative,
                        entry_resource_id: None,
                        view_id: None,
                        scene: SurfaceNode {
                            id: "root".to_owned(),
                            node_type: "column".to_owned(),
                            children: vec![SurfaceNode {
                                id: "refresh".to_owned(),
                                node_type: "button".to_owned(),
                                events: BTreeMap::from([(
                                    "click".to_owned(),
                                    "refresh_price".to_owned(),
                                )]),
                                ..SurfaceNode::default()
                            }],
                            ..SurfaceNode::default()
                        },
                        authoritative_state: json!({"price": 100}),
                        resources: Vec::new(),
                        resource_leases: Vec::new(),
                    },
                )
                .expect("mount snapshot");
            (
                instance.descriptor.instance_id,
                attachment.descriptor.attachment_id,
            )
        };
        let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(root.join("workflows"))));
        let (rx, _subscription) = register_hook_bridge_subscription(
            &hook_bridge.lock().expect("lock bridge").broadcast_hub,
            loom_protocol::SURFACE_EVENT_METHODS
                .iter()
                .map(|method| (*method).to_owned())
                .collect(),
        );
        let executions = Arc::new(AtomicUsize::new(0));
        let runner_executions = Arc::clone(&executions);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |_| {
            runner_executions.fetch_add(1, Ordering::SeqCst);
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {
                        "outputs": {"ok": {"kind": "value", "value": true}}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            tool_registry,
            Arc::clone(&instances),
            Arc::new(Mutex::new(
                SurfaceResourceStore::new(root.join("surface-resources"))
                    .expect("open Surface resource store"),
            )),
            Arc::clone(&hook_bridge),
            runner,
            1,
            4,
        )
        .expect("start executor");
        let event = SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            attachment_id: attachment_id.clone(),
            event_id: "event:confirmed-1".to_owned(),
            node_id: "refresh".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh_price".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload: json!({"symbol": "MSFT"}),
        };
        let awaiting = executor
            .submit(&instance_id, event)
            .expect("request Host confirmation");
        assert_eq!(awaiting.status, SurfaceActionStatus::AwaitingConfirmation);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let pending = instances
            .lock()
            .expect("lock Surface store")
            .pending_confirmations();
        assert_eq!(pending.len(), 1);
        assert!(instances
            .lock()
            .expect("lock Surface store")
            .pending_events()
            .is_empty());
        let confirmation_push: Value = serde_json::from_str(
            &rx.recv_timeout(Duration::from_secs(1))
                .expect("confirmation broadcast"),
        )
        .expect("confirmation JSON");
        assert_eq!(
            confirmation_push["method"],
            SURFACE_EVENT_CONFIRMATION_REQUEST
        );
        assert_eq!(confirmation_push["params"]["risk"], "high");

        let approved = executor
            .confirm(SurfaceConfirmationDecision {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                confirmation_id: pending[0].confirmation_id.clone(),
                instance_id: instance_id.clone(),
                attachment_id: attachment_id.clone(),
                device_id: "device-000-local".to_owned(),
                approved: true,
            })
            .expect("approve confirmed Surface action");
        assert_eq!(approved.status, SurfaceActionStatus::Queued);
        for _ in 0..20 {
            if executions.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(executions.load(Ordering::SeqCst), 1);

        let rejected_event = SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            attachment_id: attachment_id.clone(),
            event_id: "event:confirmed-2".to_owned(),
            node_id: "refresh".to_owned(),
            event: "click".to_owned(),
            action: Some("refresh_price".to_owned()),
            class: SurfaceEventClass::Discrete,
            generation: 0,
            base_revision: 1,
            payload: json!({}),
        };
        let awaiting_rejection = executor
            .submit(&instance_id, rejected_event)
            .expect("request second confirmation");
        assert_eq!(
            awaiting_rejection.status,
            SurfaceActionStatus::AwaitingConfirmation
        );
        let pending = instances
            .lock()
            .expect("lock Surface store")
            .pending_confirmations();
        let rejected = executor
            .confirm(SurfaceConfirmationDecision {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                confirmation_id: pending[0].confirmation_id.clone(),
                instance_id,
                attachment_id,
                device_id: "device-000-local".to_owned(),
                approved: false,
            })
            .expect("reject confirmed Surface action");
        assert_eq!(rejected.status, SurfaceActionStatus::Cancelled);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn action_timeout_is_terminal_and_late_runner_result_never_commits() {
        let root = temp_root("timeout");
        let digest = "c".repeat(64);
        let mut tool = surface_tool(&digest);
        tool.metadata.as_mut().expect("Surface metadata")["capabilities"]["surface"]["actions"]
            [0]["timeoutMs"] = json!(40);
        let (registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool, "hook-node:timeout");
        let runner_finished = Arc::new(AtomicBool::new(false));
        let worker_finished = Arc::clone(&runner_finished);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |_| {
            std::thread::sleep(Duration::from_millis(180));
            worker_finished.store(true, Ordering::Release);
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {
                        "outputs": {"value": {"kind": "value", "value": "late"}},
                        "statePatch": {"value": "late"}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            registry,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            runner,
            1,
            4,
        )
        .expect("start timeout executor");
        let queued = executor
            .submit(
                &instance_id,
                fixture_event(&instance_id, &attachment_id, "event:timeout", Value::Null),
            )
            .expect("queue timed action");
        let started = Instant::now();
        loop {
            let record = instances
                .lock()
                .expect("lock Surface store")
                .get(&instance_id)
                .expect("Surface instance");
            let ack = record
                .event_acks
                .values()
                .find(|ack| ack.request_id == queued.request_id)
                .expect("timed action ack");
            if ack.status == SurfaceActionStatus::Failed {
                assert_eq!(
                    ack.error.as_ref().expect("timeout error").code,
                    "surface_action_timeout"
                );
                assert!(record.pending_events.is_empty());
                assert!(record.latest_result.is_none());
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(1));
            drop(record);
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(220));
        assert!(runner_finished.load(Ordering::Acquire));
        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("Surface instance after late result");
        assert_eq!(record.authoritative_state["value"], 0);
        assert!(record.latest_result.is_none());
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_cancel_is_device_bound_and_stops_a_cancelable_action() {
        let root = temp_root("cancel");
        let digest = "d".repeat(64);
        let mut tool = surface_tool(&digest);
        let action = &mut tool.metadata.as_mut().expect("Surface metadata")["capabilities"]
            ["surface"]["actions"][0];
        action["cancelable"] = json!(true);
        action["timeoutMs"] = json!(2000);
        let (registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool, "hook-node:cancel");
        let runner_started = Arc::new(AtomicBool::new(false));
        let worker_started = Arc::clone(&runner_started);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
            worker_started.store(true, Ordering::Release);
            while !job.cancellation.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {"outputs": {"value": {"kind": "value", "value": "late"}}}
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            registry,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            runner,
            1,
            4,
        )
        .expect("start cancellation executor");
        let queued = executor
            .submit(
                &instance_id,
                fixture_event(&instance_id, &attachment_id, "event:cancel", Value::Null),
            )
            .expect("queue cancelable action");
        for _ in 0..100 {
            if runner_started.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(runner_started.load(Ordering::Acquire));
        let wrong_device = executor.cancel(SurfaceActionCancelRequest {
            protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.clone(),
            request_id: queued.request_id.clone(),
            device_id: "device:other".to_owned(),
        });
        assert!(matches!(wrong_device, Err(SurfaceStoreError::Invalid(_))));
        let requested = executor
            .cancel(SurfaceActionCancelRequest {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                request_id: queued.request_id.clone(),
                device_id: "device-000-local".to_owned(),
            })
            .expect("cancel action from owning device");
        assert_eq!(requested.status, SurfaceActionStatus::CancelRequested);
        let started = Instant::now();
        loop {
            let record = instances
                .lock()
                .expect("lock Surface store")
                .get(&instance_id)
                .expect("Surface instance");
            let ack = record
                .event_acks
                .values()
                .find(|ack| ack.request_id == queued.request_id)
                .expect("cancelled action ack");
            if ack.status == SurfaceActionStatus::Cancelled {
                assert!(record.pending_events.is_empty());
                assert!(record.latest_result.is_none());
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(1));
            drop(record);
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replace_latest_cancels_the_old_request_and_only_commits_the_new_result() {
        let root = temp_root("replace-latest");
        let digest = "e".repeat(64);
        let mut tool = surface_tool(&digest);
        let action = &mut tool.metadata.as_mut().expect("Surface metadata")["capabilities"]
            ["surface"]["actions"][0];
        action["concurrency"] = json!("replace_latest");
        action["timeoutMs"] = json!(2000);
        let (registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool, "hook-node:replace");
        let first_started = Arc::new(AtomicBool::new(false));
        let worker_first_started = Arc::clone(&first_started);
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
            let sequence = job
                .invocation
                .payload
                .get("sequence")
                .and_then(Value::as_u64)
                .expect("event sequence");
            if sequence == 1 {
                worker_first_started.store(true, Ordering::Release);
                while !job.cancellation.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "result": {
                        "outputs": {"value": {"kind": "value", "value": sequence}},
                        "statePatch": {"value": sequence}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            registry,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            runner,
            2,
            4,
        )
        .expect("start replace-latest executor");
        let first = executor
            .submit(
                &instance_id,
                fixture_event(
                    &instance_id,
                    &attachment_id,
                    "event:replace-1",
                    json!({"sequence": 1}),
                ),
            )
            .expect("queue first action");
        for _ in 0..100 {
            if first_started.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(first_started.load(Ordering::Acquire));
        let second = executor
            .submit(
                &instance_id,
                fixture_event(
                    &instance_id,
                    &attachment_id,
                    "event:replace-2",
                    json!({"sequence": 2}),
                ),
            )
            .expect("queue replacement action");
        let started = Instant::now();
        loop {
            let record = instances
                .lock()
                .expect("lock Surface store")
                .get(&instance_id)
                .expect("Surface instance");
            let first_ack = record
                .event_acks
                .values()
                .find(|ack| ack.request_id == first.request_id)
                .expect("first ack");
            let second_ack = record
                .event_acks
                .values()
                .find(|ack| ack.request_id == second.request_id)
                .expect("second ack");
            if first_ack.status == SurfaceActionStatus::Cancelled
                && second_ack.status == SurfaceActionStatus::Succeeded
            {
                assert_eq!(record.authoritative_state["value"], 2);
                assert_eq!(
                    record
                        .latest_result
                        .as_ref()
                        .expect("latest result")
                        .outputs["value"],
                    loom_protocol::SurfacePortValue::Value { value: json!(2) }
                );
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            drop(record);
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }
}
