// Cached immutable Hook canvas and coherent runtime overlay projection.
fn hook_canvas_overlay_revision(
    snapshot: &hook_canvas::HookCanvasSnapshot,
    statuses: &HashMap<String, HookCanvasRuntimeNodeState>,
) -> Option<String> {
    let mut tokens = snapshot
        .nodes
        .iter()
        .filter_map(|node| {
            statuses.get(&node.id).map(|state| {
                format!(
                    "{}:{}:{:?}:{:?}:{:?}:{:?}",
                    node.id,
                    state.status,
                    state.error_message,
                    state.preview_cache_token,
                    state.selected_result_index,
                    state
                        .result_candidates
                        .iter()
                        .map(|candidate| (&candidate.index, &candidate.image_url))
                        .collect::<Vec<_>>()
                )
            })
        })
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }
    tokens.sort();
    let mut hasher = DefaultHasher::new();
    for token in tokens {
        token.hash(&mut hasher);
    }
    Some(format!("{:016x}", hasher.finish()))
}

fn apply_hook_canvas_runtime_overlays(
    document: &mut hook_canvas::HookCanvasDocument,
    statuses: &HashMap<String, HookCanvasRuntimeNodeState>,
) {
    for node in &mut document.snapshot.nodes {
        let Some(state) = statuses.get(&node.id) else {
            continue;
        };
        node.status = state.status.clone();
        node.error_message = state.error_message.clone();
        node.result_candidates = state.result_candidates.clone();
        node.selected_result_index = state.selected_result_index;
    }
    let preview_overrides = statuses
        .iter()
        .filter_map(|(node_id, state)| {
            state.preview_data_url.as_ref().map(|data_url| {
                (
                    node_id.clone(),
                    data_url.clone(),
                    state.preview_cache_token.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    for (node_id, data_url, cache_token) in preview_overrides {
        document.override_preview_source(
            &node_id,
            hook_canvas::HookCanvasPreviewSource::DataUrl(data_url),
            cache_token.as_deref(),
        );
    }
    if let Some(overlay_revision) = hook_canvas_overlay_revision(&document.snapshot, statuses) {
        document.snapshot.revision =
            format!("{}-rt-{overlay_revision}", document.snapshot.revision);
    }
}

// The base cache and overlay cache each keep just the currently active document.
#[derive(Default)]
struct ActiveHookCanvasCache {
    base: hook_canvas::HookCanvasDocumentCache,
    overlay: Option<(
        Arc<hook_canvas::HookCanvasDocument>,
        HashMap<String, HookCanvasRuntimeNodeState>,
        Arc<hook_canvas::HookCanvasDocument>,
    )>,
}
static ACTIVE_HOOK_CANVAS_CACHE: OnceLock<Mutex<ActiveHookCanvasCache>> = OnceLock::new();

fn load_active_hook_canvas_document() -> Result<Arc<hook_canvas::HookCanvasDocument>> {
    let mut cache = ACTIVE_HOOK_CANVAS_CACHE
        .get_or_init(|| Mutex::new(ActiveHookCanvasCache::default()))
        .lock()
        .map_err(|_| anyhow::anyhow!("Hook canvas cache lock poisoned"))?;
    let live = hook_live_workflow_snapshots()
        .lock()
        .map_err(|_| anyhow::anyhow!("Hook live snapshot lock poisoned"))?;
    let base = if let Some(snapshot) = live.get(HOOK_LIVE_WORKFLOW_ID) {
        cache.base.serialized(
            &snapshot.source_path,
            &snapshot.bytes,
            &snapshot.root,
            &snapshot.updated_at,
        )
    } else {
        cache.base.read(&hook_session_path())?
    };
    drop(live);
    let statuses = hook_canvas_runtime_statuses()
        .lock()
        .map_err(|_| anyhow::anyhow!("Hook canvas status lock poisoned"))?;
    if statuses.is_empty() {
        cache.overlay = None;
        return Ok(base);
    }
    if let Some((prior_base, prior_statuses, document)) = cache.overlay.as_ref() {
        if Arc::ptr_eq(prior_base, &base) && prior_statuses == &*statuses {
            return Ok(Arc::clone(document));
        }
    }
    let mut document = (*base).clone();
    apply_hook_canvas_runtime_overlays(&mut document, &statuses);
    let document = Arc::new(document);
    cache.overlay = Some((base, statuses.clone(), Arc::clone(&document)));
    Ok(document)
}
