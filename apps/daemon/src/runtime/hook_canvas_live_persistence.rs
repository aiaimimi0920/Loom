// Hook Art cancellation and resource release plus live canvas persistence.

fn cancel_hook_art_request(
    request: &HookArtCancelRequest,
    shared_images: &SharedImageStoreHandle,
) -> String {
    let Ok(mut state) = hook_art_requests().lock() else {
        return hook_protocol_failure_json(
            &request.request_id,
            "request_state_unavailable",
            "lock Hook Art request state",
        );
    };
    let request_scope = HookArtRequestScope::new(request.device_id.as_deref(), &request.request_id);
    if !state.active_by_request.contains_key(&request_scope)
        && state
            .active_by_request
            .keys()
            .any(|scope| scope.request_id == request.request_id)
    {
        return hook_protocol_failure_json(
            &request.request_id,
            "cancellation_device_mismatch",
            "deviceId does not own the active request",
        );
    }
    let Some(entry) = state.active_by_request.get_mut(&request_scope) else {
        return hook_protocol_failure_json(
            &request.request_id,
            "request_not_found",
            "no active Art request matches requestId",
        );
    };
    if entry.node_id != request.node_id || entry.generation != request.generation {
        return hook_protocol_failure_json(
            &request.request_id,
            "cancellation_identity_mismatch",
            "nodeId or generation does not match the active request",
        );
    }
    if entry.device_id != request.device_id {
        return hook_protocol_failure_json(
            &request.request_id,
            "cancellation_device_mismatch",
            "deviceId does not own the active request",
        );
    }
    entry.status = HookRequestStatus::CancelRequested;
    entry.cancellation.store(true, Ordering::Release);
    let released_handles = std::mem::take(&mut entry.live_resource_handles);
    let response = hook_protocol_response_json(
        &request.request_id,
        HookRequestStatus::CancelRequested,
        json!({ "nodeId": request.node_id, "generation": request.generation }),
        None,
    );
    drop(state);
    release_shared_image_handles(shared_images, released_handles);
    response
}

fn release_hook_art_resources(
    request: &HookArtResourcesReleaseRequest,
    shared_images: &SharedImageStoreHandle,
) -> String {
    let unique_handles = request.handles.iter().collect::<BTreeSet<_>>();
    if request.protocol_version != loom_protocol::HOOK_PROTOCOL_VERSION
        || !loom_protocol::is_safe_hook_identifier(&request.request_id)
        || !loom_protocol::is_safe_hook_identifier(&request.execution_request_id)
        || !loom_protocol::is_safe_hook_identifier(&request.node_id)
        || request.handles.is_empty()
        || unique_handles.len() != request.handles.len()
        || request.handles.iter().any(|handle| {
            !loom_protocol::is_safe_hook_identifier(handle) || !handle.starts_with("Loom_Buffer_")
        })
    {
        return hook_protocol_failure_json(
            &request.request_id,
            "invalid_resource_release",
            "Art resource release requires canonical execution identity and unique Loom shared-memory handles",
        );
    }
    let execution_scope =
        HookArtRequestScope::new(request.device_id.as_deref(), &request.execution_request_id);
    let mut state = match hook_art_requests().lock() {
        Ok(state) => state,
        Err(_) => {
            return hook_protocol_failure_json(
                &request.request_id,
                "request_state_unavailable",
                "lock Hook Art request state",
            )
        }
    };
    if !state.active_by_request.contains_key(&execution_scope)
        && !state.terminal_by_request.contains_key(&execution_scope)
    {
        let request_exists_for_another_device = state
            .active_by_request
            .keys()
            .chain(state.terminal_by_request.keys())
            .any(|scope| scope.request_id == request.execution_request_id);
        return hook_protocol_failure_json(
            &request.request_id,
            if request_exists_for_another_device {
                "resource_release_device_mismatch"
            } else {
                "execution_request_not_found"
            },
            if request_exists_for_another_device {
                "deviceId does not own the Art execution resources"
            } else {
                "no Art execution matches executionRequestId"
            },
        );
    }
    let mut store = match shared_images.lock() {
        Ok(store) => store,
        Err(_) => {
            return hook_protocol_failure_json(
                &request.request_id,
                "shared_image_store_unavailable",
                "lock shared image store",
            )
        }
    };
    let release_owned =
        |node_id: &str,
         generation: u64,
         resource_handles: &BTreeSet<String>,
         live_resource_handles: &mut BTreeSet<String>,
         store: &mut SharedImageStore|
         -> std::result::Result<(Vec<String>, Vec<String>), (&'static str, &'static str)> {
            if node_id != request.node_id || generation != request.generation {
                return Err((
                    "resource_release_identity_mismatch",
                    "nodeId or generation does not own the Art execution resources",
                ));
            }
            if request
                .handles
                .iter()
                .any(|handle| !resource_handles.contains(handle))
            {
                return Err((
                    "resource_release_ownership_mismatch",
                    "one or more handles do not belong to the Art execution",
                ));
            }
            let mut released = Vec::new();
            let mut missing = Vec::new();
            for handle in &request.handles {
                if live_resource_handles.remove(handle) {
                    if store.release(handle) {
                        released.push(handle.clone());
                    } else {
                        missing.push(handle.clone());
                    }
                } else {
                    missing.push(handle.clone());
                }
            }
            Ok((released, missing))
        };
    let release_result = if let Some(entry) = state.active_by_request.get_mut(&execution_scope) {
        release_owned(
            &entry.node_id,
            entry.generation,
            &entry.resource_handles,
            &mut entry.live_resource_handles,
            &mut store,
        )
    } else if let Some(entry) = state.terminal_by_request.get_mut(&execution_scope) {
        release_owned(
            &entry.node_id,
            entry.generation,
            &entry.resource_handles,
            &mut entry.live_resource_handles,
            &mut store,
        )
    } else {
        unreachable!("execution ownership was checked above")
    };
    let (released, missing) = match release_result {
        Ok(result) => result,
        Err((code, message)) => {
            return hook_protocol_failure_json(&request.request_id, code, message)
        }
    };
    hook_protocol_response_json(
        &request.request_id,
        HookRequestStatus::Succeeded,
        json!({
            "executionRequestId": request.execution_request_id,
            "nodeId": request.node_id,
            "generation": request.generation,
            "released": released,
            "missing": missing,
        }),
        None,
    )
}

fn store_hook_live_workflow_snapshot(
    source_path: &Path,
    workflow_id: &str,
    snapshot: &Value,
) -> std::result::Result<(), String> {
    let mut root = snapshot.clone();
    let Some(object) = root.as_object_mut() else {
        return Err("Hook live workflow snapshot must be a JSON object".to_owned());
    };
    if !object.contains_key("workflowId") {
        object.insert(
            "workflowId".to_owned(),
            Value::String(workflow_id.to_owned()),
        );
    }
    let document_revision = hook_session_document_revision(&root)?;
    let bytes = serde_json::to_vec(&root)
        .map_err(|error| format!("serialize Hook live workflow snapshot: {error}"))?;
    let mut snapshots = hook_live_workflow_snapshots()
        .lock()
        .map_err(|_| "lock Hook live workflow snapshots".to_owned())?;
    snapshots.insert(
        workflow_id.to_owned(),
        HookLiveWorkflowSnapshot {
            source_path: source_path.to_path_buf(),
            bytes,
            root,
            document_revision,
            updated_at: Some(workflow_timestamp()),
        },
    );
    Ok(())
}

fn hook_canvas_root_nodes_mut(root: &mut Value) -> Option<&mut Vec<Value>> {
    let object = root.as_object()?;
    let key = if object.get("stickers").and_then(Value::as_array).is_some() {
        "stickers"
    } else if object.get("nodes").and_then(Value::as_array).is_some() {
        "nodes"
    } else {
        return None;
    };
    root.get_mut(key).and_then(Value::as_array_mut)
}

fn hook_canvas_node_mut<'a>(root: &'a mut Value, node_id: &str) -> Option<&'a mut Value> {
    hook_canvas_root_nodes_mut(root)?
        .iter_mut()
        .find(|node| node.get("id").and_then(Value::as_str) == Some(node_id))
}

fn hook_canvas_node_field_owner_mut<'a>(
    node: &'a mut Value,
    field: &str,
) -> Option<&'a mut serde_json::Map<String, Value>> {
    let use_top_level = node.get(field).is_some() || node.get("data").is_none();
    if use_top_level {
        return node.as_object_mut();
    }
    let object = node.as_object_mut()?;
    let data = object.entry("data").or_insert_with(|| json!({}));
    if !data.is_object() {
        *data = json!({});
    }
    data.as_object_mut()
}

fn hook_canvas_set_node_param(node: &mut Value, param: &str, value: Value) -> bool {
    let Some(owner) = hook_canvas_node_field_owner_mut(node, "params") else {
        return false;
    };
    let params = owner.entry("params").or_insert_with(|| json!({}));
    if !params.is_object() {
        *params = json!({});
    }
    params
        .as_object_mut()
        .expect("params object")
        .insert(param.to_owned(), value);
    true
}

fn apply_hook_canvas_persist_patch(
    root: &mut Value,
    node_id: &str,
    patch: &HookCanvasPersistPatch,
) -> bool {
    let Some(node) = hook_canvas_node_mut(root, node_id) else {
        return false;
    };
    let mut changed = false;
    for (param, value) in &patch.param_updates {
        changed |= hook_canvas_set_node_param(node, param, value.clone());
    }
    changed
}

fn write_hook_canvas_root(path: &Path, root: &Value) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(root).context("serialize Hook canvas root")?;
    // Atomic because a truncate-then-fill here is observable by Hook: Hook owns this file and may
    // be reading it concurrently, and a bare `fs::write` lets it parse a zero-length or half-written
    // document. Permissions are preserved rather than restricted — see `AtomicWritePermissions`.
    // Callers that mutate Hook-owned session data must hold HookSessionFileLease and compare the
    // Hook-owned documentRevision before reaching this atomic replacement.
    write_bytes_atomically(path, &bytes, AtomicWritePermissions::Preserve)
        .with_context(|| format!("write Hook canvas root `{}`", path.display()))?;
    Ok(bytes)
}

fn persist_hook_canvas_live_node_patch(
    node_id: &str,
    patch: &HookCanvasPersistPatch,
) -> std::result::Result<u64, HookCanvasPersistError> {
    let (session_path, expected_revision) = {
        let snapshots = hook_live_workflow_snapshots().lock().map_err(|_| {
            HookCanvasPersistError::WriteFailed(
                "Unable to lock Hook live workflow snapshots".to_owned(),
            )
        })?;
        let snapshot = snapshots
            .get(HOOK_LIVE_WORKFLOW_ID)
            .ok_or(HookCanvasPersistError::SnapshotUnavailable)?;
        (snapshot.source_path.clone(), snapshot.document_revision)
    };

    let _lease = HookSessionFileLease::acquire(&session_path).map_err(|error| {
        HookCanvasPersistError::WriteFailed(format!(
            "Unable to lock Hook session `{}`: {error:#}",
            session_path.display()
        ))
    })?;
    let content = fs::read_to_string(&session_path).map_err(|error| {
        HookCanvasPersistError::SessionUnavailable(format!(
            "Unable to read Hook session `{}`: {error}",
            session_path.display()
        ))
    })?;
    let mut session_root = serde_json::from_str::<Value>(&content).map_err(|error| {
        HookCanvasPersistError::UnsupportedDocument(format!(
            "Invalid Hook session JSON at `{}`: {error}",
            session_path.display()
        ))
    })?;
    let current_revision = hook_session_document_revision(&session_root)
        .map_err(HookCanvasPersistError::UnsupportedDocument)?;
    if current_revision != expected_revision {
        return Err(HookCanvasPersistError::RevisionConflict {
            expected: expected_revision,
            current: current_revision,
        });
    }
    if !apply_hook_canvas_persist_patch(&mut session_root, node_id, patch) {
        return Err(HookCanvasPersistError::NodeUnavailable(format!(
            "Hook session node `{node_id}` is unavailable; refresh the canvas before retrying"
        )));
    }
    let next_revision = current_revision.checked_add(1).ok_or_else(|| {
        HookCanvasPersistError::UnsupportedDocument(
            "Hook session documentRevision is exhausted".to_owned(),
        )
    })?;
    set_hook_session_document_revision(&mut session_root, next_revision).map_err(|error| {
        HookCanvasPersistError::UnsupportedDocument(format!(
            "Unable to update Hook session revision: {error:#}"
        ))
    })?;
    write_hook_canvas_root(&session_path, &session_root).map_err(|error| {
        HookCanvasPersistError::WriteFailed(format!(
            "Unable to persist Hook session `{}`: {error:#}",
            session_path.display()
        ))
    })?;

    if let Ok(mut snapshots) = hook_live_workflow_snapshots().lock() {
        if let Some(snapshot) = snapshots.get_mut(HOOK_LIVE_WORKFLOW_ID) {
            if snapshot.document_revision == expected_revision {
                let _ = apply_hook_canvas_persist_patch(&mut snapshot.root, node_id, patch);
                let _ = set_hook_session_document_revision(&mut snapshot.root, next_revision);
                if let Ok(bytes) = serde_json::to_vec(&snapshot.root) {
                    snapshot.bytes = bytes;
                }
                snapshot.document_revision = next_revision;
                snapshot.updated_at = Some(workflow_timestamp());
            }
        }
    }
    Ok(next_revision)
}
