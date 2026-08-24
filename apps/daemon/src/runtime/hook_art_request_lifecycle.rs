// Hook canvas persistence errors and Hook Art request reservation and completion.
#[derive(Debug)]
enum HookCanvasPersistError {
    SnapshotUnavailable,
    SessionUnavailable(String),
    UnsupportedDocument(String),
    RevisionConflict { expected: u64, current: u64 },
    NodeUnavailable(String),
    WriteFailed(String),
}

impl HookCanvasPersistError {
    fn code(&self) -> &'static str {
        match self {
            Self::SnapshotUnavailable => "hook_live_snapshot_unavailable",
            Self::SessionUnavailable(_) => "hook_session_unavailable",
            Self::UnsupportedDocument(_) => "hook_session_schema_unsupported",
            Self::RevisionConflict { .. } => "hook_session_revision_conflict",
            Self::NodeUnavailable(_) => "hook_session_node_unavailable",
            Self::WriteFailed(_) => "hook_session_write_failed",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::SnapshotUnavailable => {
                "Hook live snapshot is unavailable; refresh Hook before retrying".to_owned()
            }
            Self::SessionUnavailable(message)
            | Self::UnsupportedDocument(message)
            | Self::NodeUnavailable(message)
            | Self::WriteFailed(message) => message.clone(),
            Self::RevisionConflict { expected, current } => format!(
                "Hook session changed after Loom read it (expected revision {expected}, current {current}); refresh the canvas and retry"
            ),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct HookCanvasRuntimeNodeState {
    status: String,
    error_message: Option<String>,
    preview_data_url: Option<String>,
    preview_cache_token: Option<String>,
    result_candidates: Vec<hook_canvas::HookCanvasResultCandidate>,
    selected_result_index: Option<usize>,
}

#[derive(Default)]
struct HookCanvasPersistPatch {
    param_updates: Vec<(String, Value)>,
}

static HOOK_LIVE_WORKFLOW_SNAPSHOTS: OnceLock<Mutex<HashMap<String, HookLiveWorkflowSnapshot>>> =
    OnceLock::new();
static HOOK_CANVAS_RUNTIME_STATUSES: OnceLock<Mutex<HashMap<String, HookCanvasRuntimeNodeState>>> =
    OnceLock::new();
static HOOK_ART_REQUESTS: OnceLock<Mutex<HookArtRequestState>> = OnceLock::new();

fn hook_live_workflow_snapshots() -> &'static Mutex<HashMap<String, HookLiveWorkflowSnapshot>> {
    HOOK_LIVE_WORKFLOW_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hook_canvas_runtime_statuses() -> &'static Mutex<HashMap<String, HookCanvasRuntimeNodeState>> {
    HOOK_CANVAS_RUNTIME_STATUSES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hook_art_requests() -> &'static Mutex<HookArtRequestState> {
    HOOK_ART_REQUESTS.get_or_init(|| Mutex::new(HookArtRequestState::default()))
}

fn is_hook_live_workflow_id(workflow_id: &str) -> bool {
    workflow_id == HOOK_LIVE_WORKFLOW_ID
}

fn release_shared_image_handles(
    shared_images: &SharedImageStoreHandle,
    handles: impl IntoIterator<Item = String>,
) {
    let handles = handles.into_iter().collect::<BTreeSet<_>>();
    if handles.is_empty() {
        return;
    }
    let mut store = match shared_images.lock() {
        Ok(store) => store,
        Err(poisoned) => poisoned.into_inner(),
    };
    for handle in handles {
        store.release(&handle);
    }
}

fn clear_hook_canvas_runtime_state(shared_images: Option<&SharedImageStoreHandle>) {
    if let Ok(mut snapshots) = hook_live_workflow_snapshots().lock() {
        snapshots.clear();
    }
    if let Ok(mut statuses) = hook_canvas_runtime_statuses().lock() {
        statuses.clear();
    }
    let mut resource_handles = BTreeSet::new();
    if let Ok(mut requests) = hook_art_requests().lock() {
        for entry in requests.active_by_request.values() {
            entry.cancellation.store(true, Ordering::Release);
            resource_handles.extend(entry.live_resource_handles.iter().cloned());
        }
        for entry in requests.terminal_by_request.values() {
            resource_handles.extend(entry.live_resource_handles.iter().cloned());
        }
        requests.active_by_request.clear();
        requests.active_by_node.clear();
        requests.latest_generation_by_node.clear();
        requests.preview_revision_by_node.clear();
        requests.result_revision_by_node.clear();
        requests.terminal_by_request.clear();
        requests.terminal_order.clear();
    }
    if let Some(shared_images) = shared_images {
        release_shared_image_handles(shared_images, resource_handles);
    }
}

enum HookArtReservation {
    Execute(Arc<AtomicBool>),
    Replay(String),
    Reject(String),
}

fn hook_art_request_fingerprint(request: &HookArtExecuteRequest) -> String {
    serde_json::to_string(request).expect("Hook Art execution requests must serialize")
}

fn reserve_hook_art_request(
    request: &HookArtExecuteRequest,
    shared_images: &SharedImageStoreHandle,
) -> HookArtReservation {
    let mut state = match hook_art_requests().lock() {
        Ok(state) => state,
        Err(_) => {
            return HookArtReservation::Reject(hook_protocol_failure_json(
                &request.request_id,
                "request_state_unavailable",
                "lock Hook Art request state",
            ))
        }
    };
    let request_scope = HookArtRequestScope::new(request.device_id.as_deref(), &request.request_id);
    let node_scope = HookArtNodeScope::new(request.device_id.as_deref(), &request.node_id);
    let request_fingerprint = hook_art_request_fingerprint(request);
    if let Some(terminal) = state.terminal_by_request.get(&request_scope) {
        if terminal.node_id == request.node_id
            && terminal.generation == request.generation
            && terminal.request_fingerprint == request_fingerprint
        {
            if terminal
                .result_resource_handles
                .is_subset(&terminal.live_resource_handles)
            {
                return HookArtReservation::Replay(terminal.response.clone());
            }
            return HookArtReservation::Reject(hook_protocol_failure_json(
                &request.request_id,
                "request_resources_released",
                "the cached Art result contained shared-memory resources that were already released",
            ));
        }
        return HookArtReservation::Reject(hook_protocol_failure_json(
            &request.request_id,
            "request_id_conflict",
            "requestId was already used for a different Art execution",
        ));
    }
    if let Some(active) = state.active_by_request.get(&request_scope) {
        if active.node_id == request.node_id
            && active.generation == request.generation
            && active.request_fingerprint == request_fingerprint
        {
            return HookArtReservation::Replay(hook_protocol_response_json(
                &request.request_id,
                active.status.clone(),
                json!({ "nodeId": active.node_id, "generation": active.generation }),
                None,
            ));
        }
        return HookArtReservation::Reject(hook_protocol_failure_json(
            &request.request_id,
            "request_id_conflict",
            "requestId is active for a different Art execution",
        ));
    }

    let latest_generation = state
        .latest_generation_by_node
        .get(&node_scope)
        .copied()
        .unwrap_or(0);
    if request.generation < latest_generation {
        let has_active = state.active_by_node.contains_key(&node_scope);
        if has_active {
            return HookArtReservation::Reject(hook_protocol_failure_json(
                &request.request_id,
                "stale_generation",
                format!(
                    "generation {} is older than the current generation {latest_generation}",
                    request.generation
                ),
            ));
        }
    }
    let mut superseded_handles = BTreeSet::new();
    if let Some(previous_request_id) = state.active_by_node.get(&node_scope).cloned() {
        let previous_request_scope =
            HookArtRequestScope::new(request.device_id.as_deref(), &previous_request_id);
        if let Some(previous) = state.active_by_request.get_mut(&previous_request_scope) {
            if request.generation == previous.generation {
                return HookArtReservation::Reject(hook_protocol_failure_json(
                    &request.request_id,
                    "generation_conflict",
                    "another request already owns this node generation",
                ));
            }
            previous.cancellation.store(true, Ordering::Release);
            superseded_handles.append(&mut previous.live_resource_handles);
        }
    }

    let cancellation = Arc::new(AtomicBool::new(false));
    state
        .latest_generation_by_node
        .insert(node_scope.clone(), request.generation);
    state
        .active_by_node
        .insert(node_scope, request.request_id.clone());
    state.active_by_request.insert(
        request_scope,
        HookArtRequestEntry {
            node_id: request.node_id.clone(),
            generation: request.generation,
            request_fingerprint,
            cancellation: Arc::clone(&cancellation),
            status: HookRequestStatus::Running,
            device_id: request.device_id.clone(),
            resource_handles: BTreeSet::new(),
            live_resource_handles: BTreeSet::new(),
            result_resource_handles: BTreeSet::new(),
        },
    );
    drop(state);
    release_shared_image_handles(shared_images, superseded_handles);
    HookArtReservation::Execute(cancellation)
}

fn register_hook_art_resource_handles(
    request: &HookArtExecuteRequest,
    handles: &BTreeSet<String>,
    formal_result: bool,
) -> bool {
    if handles.is_empty() {
        return true;
    }
    let Ok(mut state) = hook_art_requests().lock() else {
        return false;
    };
    let request_scope = HookArtRequestScope::new(request.device_id.as_deref(), &request.request_id);
    let node_scope = HookArtNodeScope::new(request.device_id.as_deref(), &request.node_id);
    let owns_node =
        state.active_by_node.get(&node_scope).map(String::as_str) == Some(&request.request_id);
    let Some(entry) = state.active_by_request.get_mut(&request_scope) else {
        return false;
    };
    if !owns_node
        || entry.node_id != request.node_id
        || entry.generation != request.generation
        || entry.cancellation.load(Ordering::Acquire)
    {
        return false;
    }
    entry.resource_handles.extend(handles.iter().cloned());
    entry.live_resource_handles.extend(handles.iter().cloned());
    if formal_result {
        entry
            .result_resource_handles
            .extend(handles.iter().cloned());
    }
    true
}

fn hook_art_request_is_current(
    request_id: &str,
    node_id: &str,
    generation: u64,
    device_id: Option<&str>,
) -> bool {
    hook_art_requests().lock().ok().is_some_and(|state| {
        let request_scope = HookArtRequestScope::new(device_id, request_id);
        let node_scope = HookArtNodeScope::new(device_id, node_id);
        state
            .active_by_request
            .get(&request_scope)
            .is_some_and(|entry| {
                entry.node_id == node_id
                    && entry.generation == generation
                    && !entry.cancellation.load(Ordering::Acquire)
                    && state.active_by_node.get(&node_scope).map(String::as_str) == Some(request_id)
            })
    })
}

fn next_hook_art_preview_revision(
    request_id: &str,
    node_id: &str,
    generation: u64,
    device_id: Option<&str>,
) -> Option<u64> {
    let mut state = hook_art_requests().lock().ok()?;
    let request_scope = HookArtRequestScope::new(device_id, request_id);
    let node_scope = HookArtNodeScope::new(device_id, node_id);
    let current = state
        .active_by_request
        .get(&request_scope)
        .is_some_and(|entry| {
            entry.node_id == node_id
                && entry.generation == generation
                && !entry.cancellation.load(Ordering::Acquire)
                && state.active_by_node.get(&node_scope).map(String::as_str) == Some(request_id)
        });
    if !current {
        return None;
    }
    let revision = state
        .preview_revision_by_node
        .entry(node_scope)
        .or_default();
    *revision = revision.saturating_add(1);
    Some(*revision)
}

fn next_hook_art_result_revision(
    request_id: &str,
    node_id: &str,
    generation: u64,
    device_id: Option<&str>,
) -> Option<u64> {
    let mut state = hook_art_requests().lock().ok()?;
    let request_scope = HookArtRequestScope::new(device_id, request_id);
    let node_scope = HookArtNodeScope::new(device_id, node_id);
    let current = state
        .active_by_request
        .get(&request_scope)
        .is_some_and(|entry| {
            entry.node_id == node_id
                && entry.generation == generation
                && !entry.cancellation.load(Ordering::Acquire)
                && state.active_by_node.get(&node_scope).map(String::as_str) == Some(request_id)
        });
    if !current {
        return None;
    }
    let revision = state.result_revision_by_node.entry(node_scope).or_default();
    *revision = revision.saturating_add(1);
    Some(*revision)
}

fn finish_hook_art_request(
    request_id: &str,
    node_id: &str,
    generation: u64,
    device_id: Option<&str>,
    request_fingerprint: String,
    terminal_status: HookRequestStatus,
    response: String,
    shared_images: &SharedImageStoreHandle,
) {
    let Ok(mut state) = hook_art_requests().lock() else {
        return;
    };
    let request_scope = HookArtRequestScope::new(device_id, request_id);
    let node_scope = HookArtNodeScope::new(device_id, node_id);
    let matches = state
        .active_by_request
        .get(&request_scope)
        .is_some_and(|entry| entry.node_id == node_id && entry.generation == generation);
    if !matches {
        return;
    }
    let Some(mut active) = state.active_by_request.remove(&request_scope) else {
        return;
    };
    if state.active_by_node.get(&node_scope).map(String::as_str) == Some(request_id) {
        state.active_by_node.remove(&node_scope);
    }
    let mut released_handles = BTreeSet::new();
    if terminal_status != HookRequestStatus::Succeeded {
        released_handles.append(&mut active.live_resource_handles);
    }
    state.terminal_by_request.insert(
        request_scope.clone(),
        HookArtTerminalEntry {
            node_id: node_id.to_owned(),
            generation,
            request_fingerprint,
            response,
            resource_handles: active.resource_handles,
            live_resource_handles: active.live_resource_handles,
            result_resource_handles: active.result_resource_handles,
        },
    );
    state.terminal_order.push_back(request_scope);
    while state.terminal_order.len() > MAX_HOOK_ART_TERMINAL_REQUESTS {
        if let Some(expired) = state.terminal_order.pop_front() {
            if let Some(mut entry) = state.terminal_by_request.remove(&expired) {
                released_handles.append(&mut entry.live_resource_handles);
            }
        }
    }
    drop(state);
    release_shared_image_handles(shared_images, released_handles);
}
