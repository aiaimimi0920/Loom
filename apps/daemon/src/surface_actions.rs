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
    SurfaceInstanceMode, SurfacePackageManifest, SurfacePatch, SurfacePreviewCommit,
    SurfaceResultCommit, SURFACE_EVENT_ACTION_ACK, SURFACE_EVENT_ACTION_PROGRESS,
    SURFACE_EVENT_CONFIRMATION_REQUEST, SURFACE_EVENT_FAILURE, SURFACE_EVENT_PATCH,
    SURFACE_EVENT_PREVIEW, SURFACE_EVENT_RESULT, SURFACE_PROTOCOL_VERSION,
};
use loom_tool_registry::{framework::FrameworkRegistry, ToolDefinition, ToolRegistry};
use loom_workflow_runtime::execute_tool_with_workflows_timeout_and_cancellation;
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
/// How long a worker waits for an abandoned runner thread to finish before it gives up on
/// reclaiming it. A cancelled or timed-out job must not release its `Serial` lock while the runner
/// it started is still executing, but a thread cannot be stopped from the outside, so the wait has
/// to end somewhere. The runner is bounded by the same timeout as the worker's polling loop, so it
/// normally returns within a poll or two of the loop breaking.
const SURFACE_ACTION_RUNNER_REAP_MILLIS: u64 = 5_000;
/// How many times a submit re-reads the store after resolving a package before it gives up.
///
/// The resolve happens with no lock held, so the instance can be migrated to a different package
/// underneath it. That is rare and self-clearing, but a caller that retried forever would spin
/// against a migration loop, so the attempts are bounded and the last one reports a conflict.
const SURFACE_ACTION_PREPARE_ATTEMPTS: usize = 3;
/// How many Surface manifests are cached before the cache is dropped wholesale.
///
/// Entries are keyed by locked package identity, so an entry can never go stale — it can only pile
/// up as instances migrate to new Art versions. Clearing past the cap keeps the bound without
/// pretending to know which entry is the coldest.
const SURFACE_MANIFEST_CACHE_LIMIT: usize = 64;

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
    /// Surface manifests by locked package identity, so a burst of events against one instance parses
    /// the manifest once instead of once per event. Guarded by its own mutex: it must not be reachable
    /// only while the Surface store lock is held, which is the whole point of caching it.
    manifest_cache: Mutex<BTreeMap<String, Arc<SurfacePackageManifest>>>,
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
                // The runner is the only place that can hand the flag to a non-framework tool. Until it
                // did, a cancelled MCP or cloud action ran on to its timeout and its result was recorded
                // as if the caller still wanted it.
                execute_tool_with_workflows_timeout_and_cancellation(
                    &job.tool,
                    &servers,
                    &workflow_store,
                    &runner_registry,
                    arguments,
                    timeout,
                    job.cancellation.as_ref(),
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
            manifest_cache: Mutex::new(BTreeMap::new()),
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
            let (descriptor, event) = {
                let store = self.surface_instances.lock().map_err(|_| {
                    SurfaceStoreError::Conflict("Surface store is unavailable".into())
                })?;
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
                (instance.descriptor.clone(), event)
            };
            let action_id = event.action.as_deref().ok_or_else(|| {
                SurfaceStoreError::Invalid("Surface event has no declared action".into())
            })?;
            // Resolved with the store lock released: the resolve reads the installed package from disk.
            let (_, action) = self.resolve_action(&descriptor, action_id)?;
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

    /// Resolves the locked Art package for `descriptor` and picks `action_id` out of its Surface
    /// manifest.
    ///
    /// Callers must not hold the Surface store lock across this: the resolver reads the installed
    /// package from disk, so holding the store lock made every other Surface request — for any
    /// instance — queue behind one instance's package I/O.
    fn resolve_action(
        &self,
        descriptor: &loom_protocol::SurfaceInstanceDescriptor,
        action_id: &str,
    ) -> Result<(ToolDefinition, SurfaceActionDefinition), SurfaceStoreError> {
        let tool = (self.tool_resolver)(descriptor)?;
        let manifest = self.surface_manifest(descriptor, &tool)?;
        let action = manifest
            .actions
            .iter()
            .find(|action| action.id == action_id)
            .cloned()
            .ok_or_else(|| {
                SurfaceStoreError::Invalid(format!(
                    "Surface action {action_id} is not declared by the locked Art package"
                ))
            })?;
        Ok((tool, action))
    }

    /// Returns the Surface manifest of a resolved package, parsing it at most once per locked package
    /// identity.
    ///
    /// The key is `art_id`, `art_version` and `package_digest`, which together pin the package
    /// content, so a cached manifest cannot describe anything but the package the caller resolved. A
    /// poisoned cache is treated as a cache miss rather than an error: the manifest is still available
    /// from the tool, and a Surface action failing because a cache lock was poisoned would be worse
    /// than parsing it again.
    fn surface_manifest(
        &self,
        descriptor: &loom_protocol::SurfaceInstanceDescriptor,
        tool: &ToolDefinition,
    ) -> Result<Arc<SurfacePackageManifest>, SurfaceStoreError> {
        let key = format!(
            "{}@{}#{}",
            descriptor.art_id, descriptor.art_version, descriptor.package_digest
        );
        if let Ok(cache) = self.manifest_cache.lock() {
            if let Some(manifest) = cache.get(&key) {
                return Ok(Arc::clone(manifest));
            }
        }
        let manifest = tool
            .surface_manifest()
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
            .ok_or_else(|| SurfaceStoreError::Invalid("Art has no Surface manifest".into()))?;
        let manifest = Arc::new(manifest);
        if let Ok(mut cache) = self.manifest_cache.lock() {
            if cache.len() >= SURFACE_MANIFEST_CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(key, Arc::clone(&manifest));
        }
        Ok(manifest)
    }

    fn submit_internal(
        &self,
        instance_id: &str,
        event: SurfaceEvent,
        recovering: bool,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        let action_id = event
            .action
            .as_deref()
            .ok_or_else(|| {
                SurfaceStoreError::Invalid("Surface event has no declared action".into())
            })?
            .to_owned();
        let mut attempt = 0;
        let (tool, action, invocation, existing_ack, cancellation) = loop {
            attempt += 1;
            // Read the locked package, then let go of the store. Nothing is reserved or accepted yet,
            // so releasing the lock here costs only the re-read in the third phase below.
            let descriptor = {
                let store = self.surface_instances.lock().map_err(|_| {
                    SurfaceStoreError::Conflict("Surface store is unavailable".into())
                })?;
                let previous_ack = store.event_ack(instance_id, &event.event_id);
                if let Some(ack) = settled_ack(previous_ack.as_ref(), recovering) {
                    return Ok(ack);
                }
                store
                    .descriptor(instance_id)
                    .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?
            };
            // Resolved with no lock held: the resolver reads the installed package from disk, and the
            // manifest parse behind it is pure CPU.
            let (tool, action) = self.resolve_action(&descriptor, &action_id)?;

            let mut store = self
                .surface_instances
                .lock()
                .map_err(|_| SurfaceStoreError::Conflict("Surface store is unavailable".into()))?;
            let instance = store
                .get(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            if !same_locked_package(&instance.descriptor, &descriptor) {
                // The instance migrated to a different package while its manifest was being read, so
                // the action definition in hand may not be the one the instance now declares.
                drop(store);
                if attempt >= SURFACE_ACTION_PREPARE_ATTEMPTS {
                    return Err(SurfaceStoreError::Conflict(
                        "Surface instance kept changing packages while its action was prepared"
                            .to_owned(),
                    ));
                }
                continue;
            }
            // Re-read the ack under the second lock: another submit of the same event may have been
            // accepted while the package was resolving, and that ack is the one the caller must see.
            let previous_ack = store.event_ack(instance_id, &event.event_id);
            if let Some(ack) = settled_ack(previous_ack.as_ref(), recovering) {
                return Ok(ack);
            }
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
            break (tool, action, invocation, ack, cancellation);
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

/// Returns the ack a submit should hand straight back, if there is one.
///
/// Submitting an event is idempotent: an event that already has an ack does not run a second time.
/// Recovery is the exception — it deliberately re-submits the acks that never reached a terminal
/// state, which is what the three statuses below are.
///
/// A submit calls this twice, once on each side of the package resolve, because a concurrent submit
/// of the same event may have been accepted while this one held no lock.
fn settled_ack(previous: Option<&SurfaceActionAck>, recovering: bool) -> Option<SurfaceActionAck> {
    let existing = previous?;
    if recovering
        && matches!(
            &existing.status,
            SurfaceActionStatus::Queued
                | SurfaceActionStatus::Interrupted
                | SurfaceActionStatus::CancelRequested
        )
    {
        return None;
    }
    Some(existing.clone())
}

/// Whether two readings of one instance's descriptor still name the same locked Art package.
///
/// Only the three fields that pin package content are compared. The rest of the descriptor carries
/// counters — `generation`, `surface_revision`, `preview_revision`, `result_revision` — that move on
/// ordinary traffic such as a snapshot, so comparing whole descriptors would report a package change
/// on nearly every concurrent event and turn the retry into a spin.
fn same_locked_package(
    left: &loom_protocol::SurfaceInstanceDescriptor,
    right: &loom_protocol::SurfaceInstanceDescriptor,
) -> bool {
    left.art_id == right.art_id
        && left.art_version == right.art_version
        && left.package_digest == right.package_digest
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

/// Owns the release of an action's reservation for as long as the job body runs, and reports a
/// panic in that body as a failure the caller can see.
///
/// `request_executor::worker_loop` catches the unwind from a worker handler, so a panic inside a
/// Surface action job neither crashes the daemon nor reaches any of the bookkeeping below the panic
/// site. Without this guard a `RejectWhileRunning` action stayed reserved for the rest of the
/// daemon's life — every later invocation of that `instance:action` pair answered "is already
/// running" — and the last persisted ack was the `Running` one, so Hook waited on a request that
/// could never resolve.
struct SurfaceActionJobGuard<'a> {
    job: &'a SurfaceActionJob,
    surface_instances: &'a SharedSurfaceInstanceStore,
    hook_bridge: &'a SharedHookBridgeRuntime,
    coordinator: &'a Arc<Mutex<SurfaceActionCoordinator>>,
    /// Set once the body has persisted a terminal ack of its own. A guard that drops while this is
    /// still false was dropped by an unwind.
    settled: bool,
}

impl<'a> SurfaceActionJobGuard<'a> {
    fn new(
        job: &'a SurfaceActionJob,
        surface_instances: &'a SharedSurfaceInstanceStore,
        hook_bridge: &'a SharedHookBridgeRuntime,
        coordinator: &'a Arc<Mutex<SurfaceActionCoordinator>>,
    ) -> Self {
        Self {
            job,
            surface_instances,
            hook_bridge,
            coordinator,
            settled: false,
        }
    }

    fn settle(&mut self) {
        self.settled = true;
    }
}

impl Drop for SurfaceActionJobGuard<'_> {
    fn drop(&mut self) {
        if !self.settled {
            eprintln!(
                "loom surface action {} panicked while handling request {}",
                self.job.action.id, self.job.ack.request_id
            );
            // Every store access on this path tolerates a poisoned mutex, because the panic may
            // well have happened while this thread was holding one. A destructor that panicked
            // during an unwind would abort the process.
            finish_failed(
                self.job,
                execution_error(
                    "surface_action_panicked",
                    "Surface action handling panicked before it reached a terminal state",
                ),
                self.surface_instances,
                self.hook_bridge,
            );
        }
        release_reservation(
            self.coordinator,
            &self.job.event.instance_id,
            &self.job.action,
            Some(&self.job.ack.request_id),
        );
    }
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
    // Declared after the serial lock so it drops first: the reservation is released before the
    // lock that serializes the action, never the other way around.
    let mut guard = SurfaceActionJobGuard::new(&job, surface_instances, hook_bridge, coordinator);
    if !is_latest(coordinator, &job) || job.cancellation.load(Ordering::Acquire) {
        finish_cancelled(&job, surface_instances, hook_bridge);
        guard.settle();
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
    let mut runner_thread = None;
    let result = match spawn {
        Err(error) => Err(execution_error(
            "surface_action_runner_failed",
            format!("Surface action runner could not start: {error}"),
        )),
        Ok(thread) => {
            runner_thread = Some(thread);
            loop {
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
            }
        }
    };
    let abandoned_runner = cancelled || timed_out;
    // The runner can observe cancellation and return before this worker's polling loop
    // observes the token. Re-check it after receiving the result so that a cooperative
    // runner cannot race a late success commit over an accepted cancellation.
    if cancelled
        || (!timed_out && job.cancellation.load(Ordering::Acquire))
        || !is_latest(coordinator, &job)
    {
        finish_cancelled(&job, surface_instances, hook_bridge);
        guard.settle();
    } else {
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
                finish_failed(&job, error, surface_instances, hook_bridge);
                if timed_out {
                    broadcast_progress(hook_bridge, &job, None, "timeout");
                }
            }
        }
        guard.settle();
    }
    // The ack is out, so Hook is no longer waiting. Reclaim the runner before `guard` releases the
    // reservation and `_serial_guard` releases the serial lock, so a `Serial` follow-up cannot
    // start while the runner this job spawned is still executing.
    reap_runner_thread(
        runner_thread,
        &result_rx,
        &job.ack.request_id,
        abandoned_runner,
        Duration::from_millis(SURFACE_ACTION_RUNNER_REAP_MILLIS),
    );
}

/// Waits for a spawned runner thread to finish, then joins it.
///
/// The join handle used to be dropped as soon as the polling loop broke, which detached the thread.
/// On a timeout or a cancellation that meant the worker released its `Serial` lock and its
/// reservation while the runner was still running, so the next invocation of the same action ran
/// concurrently with the abandoned one — the opposite of what `Serial` promises — and nothing ever
/// reported a runner that ignored its budget.
///
/// The runner sends its result immediately before returning, so the result channel doubles as the
/// "thread is about to finish" signal and the join that follows it does not block. `grace` bounds
/// the wait because a thread cannot be stopped from the outside; when it runs out the handle is
/// dropped and the thread is left to finish on its own, which is reported rather than hidden.
fn reap_runner_thread(
    thread: Option<std::thread::JoinHandle<()>>,
    result_rx: &mpsc::Receiver<Result<Value, SurfaceExecutionError>>,
    request_id: &str,
    abandoned: bool,
    grace: Duration,
) {
    let Some(thread) = thread else {
        return;
    };
    if abandoned {
        let deadline = Instant::now() + grace;
        loop {
            let now = Instant::now();
            if now >= deadline {
                eprintln!(
                    "loom surface action runner {} outlived its budget by more than {} ms and was abandoned",
                    request_id,
                    grace.as_millis()
                );
                return;
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(SURFACE_ACTION_POLL_MILLIS));
            match result_rx.recv_timeout(wait) {
                // A late result is discarded: the terminal ack for this request has already been
                // decided and sent. All that matters here is that the thread is finishing.
                Ok(_) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => continue,
            }
        }
    }
    let _ = thread.join();
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
) {
    let mut cancelled = job.ack.clone();
    cancelled.status = SurfaceActionStatus::Cancelled;
    persist_ack(surface_instances, &cancelled, true);
    broadcast_ack(hook_bridge, &cancelled);
}

/// Records a failure and persists the `Failed` ack for it.
///
/// Shared by the normal error path and by `SurfaceActionJobGuard::drop`, so a panic produces the
/// same three observable effects as a returned error: a recorded failure on the instance, a terminal
/// ack, and a failure broadcast.
fn finish_failed(
    job: &SurfaceActionJob,
    error: SurfaceExecutionError,
    surface_instances: &SharedSurfaceInstanceStore,
    hook_bridge: &SharedHookBridgeRuntime,
) {
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
        surface_tool_at("1.0.0", digest)
    }

    fn surface_tool_at(version: &str, digest: &str) -> ToolDefinition {
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
                "packageSecurity": { "version": version },
                "artPackage": {
                    "version": version,
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

    fn panic_guard_job(
        tool: ToolDefinition,
        instance_id: &str,
        attachment_id: &str,
        event: &SurfaceEvent,
        action: SurfaceActionDefinition,
        ack: SurfaceActionAck,
        cancellation: Arc<AtomicBool>,
    ) -> SurfaceActionJob {
        SurfaceActionJob {
            event: event.clone(),
            ack: ack.clone(),
            action: action.clone(),
            tool,
            invocation: SurfaceActionInvocation {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.to_owned(),
                attachment_id: attachment_id.to_owned(),
                request_id: ack.request_id,
                event_id: event.event_id.clone(),
                action_id: action.id,
                event_class: event.class.clone(),
                generation: event.generation,
                base_revision: event.base_revision,
                payload: event.payload.clone(),
                authoritative_state: json!({"value": 0}),
            },
            cancellation,
        }
    }

    #[test]
    fn a_panicking_job_body_releases_the_reservation_and_persists_a_failed_ack() {
        let root = temp_root("panic-guard");
        let digest = "e".repeat(64);
        let tool = surface_tool(&digest);
        let (_registry, instances, _resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool.clone(), "hook-node:panic");
        let mut action = tool
            .surface_manifest()
            .expect("read Surface manifest")
            .expect("Surface manifest")
            .actions
            .remove(0);
        // `RejectWhileRunning` is the mode a leaked reservation breaks permanently: the key stays in
        // the coordinator and every later invocation of the pair answers "is already running".
        action.concurrency = SurfaceActionConcurrency::RejectWhileRunning;
        let event = fixture_event(&instance_id, &attachment_id, "event:panic-1", json!({}));
        let coordinator = Arc::new(Mutex::new(SurfaceActionCoordinator::default()));
        let cancellation =
            reserve_action(&coordinator, &instance_id, &action, &event).expect("reserve action");
        let ack = instances
            .lock()
            .expect("lock Surface store")
            .accept_event(&instance_id, event.clone())
            .expect("accept event");
        let job = panic_guard_job(
            tool,
            &instance_id,
            &attachment_id,
            &event,
            action.clone(),
            ack,
            cancellation,
        );

        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = SurfaceActionJobGuard::new(&job, &instances, &hook_bridge, &coordinator);
            panic!("surface action body exploded");
        }));
        std::panic::set_hook(previous_hook);
        assert!(outcome.is_err(), "the guarded body panicked");

        assert!(
            coordinator
                .lock()
                .expect("lock coordinator")
                .reject_reservations
                .is_empty(),
            "an unwind must not leave the action reserved"
        );
        // A second invocation of the same pair is accepted again, which is what the leaked key used
        // to make impossible for the rest of the daemon's life.
        let next_event = fixture_event(&instance_id, &attachment_id, "event:panic-2", json!({}));
        reserve_action(&coordinator, &instance_id, &action, &next_event)
            .expect("reserve the action again after the panic");

        let persisted = instances
            .lock()
            .expect("lock Surface store")
            .event_ack(&instance_id, &event.event_id)
            .expect("persisted ack");
        assert_eq!(persisted.status, SurfaceActionStatus::Failed);
        assert_eq!(
            persisted.error.expect("failure error").code,
            "surface_action_panicked"
        );
        let record = instances
            .lock()
            .expect("lock Surface store")
            .get(&instance_id)
            .expect("instance record");
        assert!(
            record.pending_events.is_empty(),
            "the panicked event is no longer pending"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_settled_job_guard_releases_without_synthesizing_a_failure() {
        let root = temp_root("settled-guard");
        let digest = "f".repeat(64);
        let tool = surface_tool(&digest);
        let (_registry, instances, _resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, tool.clone(), "hook-node:settled");
        let mut action = tool
            .surface_manifest()
            .expect("read Surface manifest")
            .expect("Surface manifest")
            .actions
            .remove(0);
        action.concurrency = SurfaceActionConcurrency::RejectWhileRunning;
        let event = fixture_event(&instance_id, &attachment_id, "event:settled-1", json!({}));
        let coordinator = Arc::new(Mutex::new(SurfaceActionCoordinator::default()));
        let cancellation =
            reserve_action(&coordinator, &instance_id, &action, &event).expect("reserve action");
        let ack = instances
            .lock()
            .expect("lock Surface store")
            .accept_event(&instance_id, event.clone())
            .expect("accept event");
        let job = panic_guard_job(
            tool,
            &instance_id,
            &attachment_id,
            &event,
            action,
            ack,
            cancellation,
        );
        {
            let mut guard =
                SurfaceActionJobGuard::new(&job, &instances, &hook_bridge, &coordinator);
            finish_cancelled(&job, &instances, &hook_bridge);
            guard.settle();
        }
        let persisted = instances
            .lock()
            .expect("lock Surface store")
            .event_ack(&instance_id, &event.event_id)
            .expect("persisted ack");
        assert_eq!(
            persisted.status,
            SurfaceActionStatus::Cancelled,
            "a settled guard leaves the terminal ack the body already wrote"
        );
        assert!(coordinator
            .lock()
            .expect("lock coordinator")
            .reject_reservations
            .is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_abandoned_runner_thread_is_joined_when_it_finishes_and_given_up_on_when_it_does_not() {
        // A runner that finishes shortly after the worker stopped waiting: the result channel is the
        // signal, and the join that follows it must actually run to completion.
        let (result_tx, result_rx) = mpsc::sync_channel::<Result<Value, SurfaceExecutionError>>(1);
        let ran_to_completion = Arc::new(AtomicBool::new(false));
        let thread_flag = Arc::clone(&ran_to_completion);
        let thread = std::thread::Builder::new()
            .name("loom-surface-runner-test-late".to_owned())
            .spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                let _ = result_tx.send(Ok(json!({})));
                thread_flag.store(true, Ordering::SeqCst);
            })
            .expect("spawn a late runner");
        reap_runner_thread(
            Some(thread),
            &result_rx,
            "request:late",
            true,
            Duration::from_millis(2_000),
        );
        assert!(
            ran_to_completion.load(Ordering::SeqCst),
            "a reclaimed runner is joined, not merely observed"
        );

        // A runner that ignores its budget cannot be stopped, so the wait has to end. It must last
        // at least the grace window and no longer than that plus a poll.
        let (result_tx, result_rx) = mpsc::sync_channel::<Result<Value, SurfaceExecutionError>>(1);
        let release = Arc::new(AtomicBool::new(false));
        let thread_release = Arc::clone(&release);
        let thread = std::thread::Builder::new()
            .name("loom-surface-runner-test-stuck".to_owned())
            .spawn(move || {
                while !thread_release.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let _ = result_tx.send(Ok(json!({})));
            })
            .expect("spawn a stuck runner");
        let started = Instant::now();
        reap_runner_thread(
            Some(thread),
            &result_rx,
            "request:stuck",
            true,
            Duration::from_millis(120),
        );
        let waited = started.elapsed();
        release.store(true, Ordering::SeqCst);
        assert!(
            waited >= Duration::from_millis(120),
            "the grace window is honoured: {waited:?}"
        );
        assert!(
            waited < Duration::from_secs(2),
            "giving up on a stuck runner is bounded: {waited:?}"
        );

        // The normal path already holds the result, so no grace window is spent at all.
        let (result_tx, result_rx) = mpsc::sync_channel::<Result<Value, SurfaceExecutionError>>(1);
        let ran_to_completion = Arc::new(AtomicBool::new(false));
        let thread_flag = Arc::clone(&ran_to_completion);
        let thread = std::thread::Builder::new()
            .name("loom-surface-runner-test-done".to_owned())
            .spawn(move || {
                let _ = result_tx.send(Ok(json!({})));
                thread_flag.store(true, Ordering::SeqCst);
            })
            .expect("spawn a finished runner");
        let started = Instant::now();
        reap_runner_thread(
            Some(thread),
            &result_rx,
            "request:done",
            false,
            Duration::from_millis(2_000),
        );
        assert!(ran_to_completion.load(Ordering::SeqCst));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "a runner that already returned is joined immediately"
        );
    }

    /// A runner for tests that only exercise the submit prologue. The job never has to produce a
    /// result, so failing immediately keeps the worker out of the way of the assertions.
    fn unused_runner() -> Arc<SurfaceActionRunner> {
        Arc::new(|_job| {
            Err(execution_error(
                "test_runner_unused",
                "this test does not exercise the runner",
            ))
        })
    }

    #[test]
    fn a_burst_of_events_reuses_one_parsed_surface_manifest() {
        let root = temp_root("manifest-cache");
        let digest = "a".repeat(64);
        let (tool_registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, surface_tool(&digest), "hook-node:cache");
        let resolves = Arc::new(AtomicUsize::new(0));
        let resolver_resolves = Arc::clone(&resolves);
        let resolver: Arc<SurfaceToolResolver> = Arc::new(move |descriptor| {
            resolver_resolves.fetch_add(1, Ordering::SeqCst);
            let tool = tool_registry
                .get_tool(&descriptor.art_id)
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
                .ok_or_else(|| SurfaceStoreError::NotFound(descriptor.art_id.clone()))?;
            validate_locked_tool(descriptor, &tool)?;
            Ok(tool)
        });
        let executor = SurfaceActionExecutor::new_with_components(
            resolver,
            instances,
            resources,
            hook_bridge,
            unused_runner(),
            1,
            8,
        )
        .expect("build Surface action executor");
        for index in 0..3 {
            executor
                .submit(
                    &instance_id,
                    fixture_event(
                        &instance_id,
                        &attachment_id,
                        &format!("event:cache-{index}"),
                        json!({}),
                    ),
                )
                .expect("submit Surface event");
        }
        assert_eq!(
            resolves.load(Ordering::SeqCst),
            3,
            "the package is still resolved per submit, because that is what re-checks installation and trust"
        );
        let cache = executor
            .manifest_cache
            .lock()
            .expect("lock the manifest cache");
        assert_eq!(
            cache.len(),
            1,
            "three events against one locked package parse one manifest"
        );
    }

    #[test]
    fn a_package_migration_during_resolve_makes_the_submit_prepare_again() {
        let root = temp_root("migrate-under-resolve");
        let first_digest = "a".repeat(64);
        let second_digest = "b".repeat(64);
        let (tool_registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(
                &root,
                surface_tool_at("1.0.0", &first_digest),
                "hook-node:migrate",
            );
        let resolves = Arc::new(AtomicUsize::new(0));
        let resolver_resolves = Arc::clone(&resolves);
        let resolver_instances = Arc::clone(&instances);
        let migrated_digest = second_digest.clone();
        let resolver: Arc<SurfaceToolResolver> = Arc::new(move |descriptor| {
            let tool = tool_registry
                .get_tool(&descriptor.art_id)
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
                .ok_or_else(|| SurfaceStoreError::NotFound(descriptor.art_id.clone()))?;
            validate_locked_tool(descriptor, &tool)?;
            // On the first resolve only, migrate the instance to a different package behind the
            // caller's back. That is exactly the race the re-validation exists for: this resolver
            // runs with no store lock held, so the descriptor it was handed can go stale.
            if resolver_resolves.fetch_add(1, Ordering::SeqCst) == 0 {
                tool_registry
                    .save_tool(surface_tool_at("2.0.0", &migrated_digest))
                    .expect("publish the migrated Art package");
                resolver_instances
                    .lock()
                    .expect("lock Surface store")
                    .migrate_instance(
                        &descriptor.instance_id,
                        None,
                        "2.0.0",
                        &migrated_digest,
                        1,
                        json!({"value": 0}),
                    )
                    .expect("migrate the instance mid-resolve");
            }
            Ok(tool)
        });
        let executor = SurfaceActionExecutor::new_with_components(
            resolver,
            Arc::clone(&instances),
            resources,
            hook_bridge,
            unused_runner(),
            1,
            8,
        )
        .expect("build Surface action executor");
        // The migration bumps the instance generation, so the event this test submits is stale by the
        // time the second prepare reaches `accept_event`. That rejection is the point: it can only
        // come from a prepare that re-read the store after the resolve. A prepare that trusted its
        // first reading would have accepted the event against the package the instance has left.
        let error = executor
            .submit(
                &instance_id,
                fixture_event(&instance_id, &attachment_id, "event:migrate", json!({})),
            )
            .expect_err("the migrated instance rejects an event from the old generation");
        assert_eq!(
            resolves.load(Ordering::SeqCst),
            2,
            "the first prepare is discarded and the submit resolves the package the instance moved to"
        );
        assert!(
            matches!(&error, SurfaceStoreError::Conflict(message) if message.contains("stale")),
            "expected a staleness conflict from the fresh instance, got {error:?}"
        );
        assert_eq!(
            instances
                .lock()
                .expect("lock Surface store")
                .descriptor(&instance_id)
                .expect("instance is still there")
                .package_digest,
            second_digest,
            "the second prepare saw the package the instance ended up on"
        );
        let cache = executor
            .manifest_cache
            .lock()
            .expect("lock the manifest cache");
        assert_eq!(
            cache.len(),
            2,
            "each locked package identity gets its own manifest entry"
        );
    }

    /// Loom's first wire-size budget, and the reason `loom_perf` exists. It measures everything Loom
    /// pushes to a Hook client for one completed declarative action: the queued and succeeded acks,
    /// the committed patch, and the formal result. A regression that re-sends a whole snapshot where
    /// a patch would do, or serialises the same payload several times into one message, shows up
    /// here as a multiple of the budget rather than as a silent slowdown on a remote device.
    ///
    /// The ceiling is deliberately loose. It is not a benchmark of Loom's best case and it should
    /// not be tightened onto the measured number, because a legitimate new field would then break
    /// the build for a few dozen bytes.
    #[test]
    fn one_surface_action_stays_within_its_response_byte_budget() {
        // Measured at 1,665 bytes on 2026-08-22. The budget is about three times that, so ordinary
        // growth of the envelope is fine and an order-of-magnitude regression is not.
        const BUDGET_BYTES: u64 = 5_120;

        let root = temp_root("perf-response-bytes");
        let digest = "e".repeat(64);
        let (tool_registry, instances, resources, hook_bridge, instance_id, attachment_id) =
            setup_action_fixture(&root, surface_tool(&digest), "hook-node:perf");
        let (rx, _subscription) = register_hook_bridge_subscription(
            &hook_bridge.lock().expect("lock bridge").broadcast_hub,
            loom_protocol::SURFACE_EVENT_METHODS
                .iter()
                .map(|method| (*method).to_owned())
                .collect(),
        );
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |_job| {
            Ok(json!({
                "surfaceAction": {
                    "protocolVersion": SURFACE_PROTOCOL_VERSION,
                    "patches": [{
                        "operations": [{
                            "op": "set",
                            "nodeId": "refresh",
                            "path": "/props/label",
                            "value": "done"
                        }],
                        "statePatch": {"value": 1}
                    }],
                    "result": {
                        "outputs": {"value": {"kind": "value", "value": 1}},
                        "statePatch": {"value": 1}
                    }
                }
            }))
        });
        let executor = SurfaceActionExecutor::new_with_runner(
            tool_registry,
            Arc::clone(&instances),
            resources,
            Arc::clone(&hook_bridge),
            runner,
            1,
            4,
        )
        .expect("start executor");
        executor
            .submit(
                &instance_id,
                fixture_event(&instance_id, &attachment_id, "event:perf-1", json!({})),
            )
            .expect("queue action");

        let mut broadcast_bytes = 0u64;
        let mut saw_patch = false;
        let mut saw_result = false;
        let mut saw_success = false;
        for _ in 0..8 {
            let message = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("Surface action broadcast");
            broadcast_bytes += message.len() as u64;
            let parsed: Value = serde_json::from_str(&message).expect("broadcast JSON");
            match parsed["method"].as_str() {
                Some(SURFACE_EVENT_PATCH) => saw_patch = true,
                Some(SURFACE_EVENT_RESULT) => saw_result = true,
                Some(SURFACE_EVENT_ACTION_ACK) if parsed["params"]["status"] == "succeeded" => {
                    saw_success = true;
                }
                _ => {}
            }
            if saw_patch && saw_result && saw_success {
                break;
            }
        }
        assert!(
            saw_patch && saw_result && saw_success,
            "the action has to run to completion before its wire cost means anything"
        );
        loom_perf::assert_within(
            "surface_action_response_bytes",
            "bytes",
            broadcast_bytes,
            BUDGET_BYTES,
        );

        drop(executor);
        let _ = std::fs::remove_dir_all(root);
    }
}
