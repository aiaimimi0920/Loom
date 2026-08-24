// Canvas workflow paths, preview formats, and durable save behavior.
fn canvas_workflow_dir(root: &Path, id: &str) -> Option<PathBuf> {
    // The id becomes a directory name, so reject anything that could escape the
    // canvas-workflow root or is not a plain slug.
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return None;
    }
    // Reject `..` traversal, hidden (leading-dot) names, and trailing-dot names
    // (Windows silently strips a trailing dot, aliasing `foo.` to `foo`).
    if trimmed.contains("..") || trimmed.starts_with('.') || trimmed.ends_with('.') {
        return None;
    }
    Some(root.join(trimmed))
}

fn canvas_workflow_preview_ext(source: &hook_canvas::HookCanvasPreviewSource) -> &'static str {
    // Sniff the extension from the source bytes so the saved file keeps a usable
    // type; default to png which the content-type sniffer also accepts.
    match source {
        hook_canvas::HookCanvasPreviewSource::File(path) => {
            match path.extension().and_then(|ext| ext.to_str()) {
                Some(ext)
                    if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") =>
                {
                    "jpg"
                }
                Some(ext) if ext.eq_ignore_ascii_case("webp") => "webp",
                _ => "png",
            }
        }
        hook_canvas::HookCanvasPreviewSource::DataUrl(_) => "png",
    }
}

fn save_hook_canvas_workflow(
    path_id: &str,
    body: &str,
    workflow_store: &WorkflowStore,
    canvas_workflow_root: &Path,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<SaveHookCanvasWorkflowRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    if request.selected_node_id.trim().is_empty() {
        return invalid_request("selectedNodeId is required");
    }

    let Some(workflow_dir) = canvas_workflow_dir(canvas_workflow_root, path_id) else {
        return invalid_request("invalid workflow id");
    };

    let document = match load_active_hook_canvas_document() {
        Ok(document) => document,
        Err(error) => {
            eprintln!("loom Hook canvas workflow export failed: {error:#}");
            return structured_error(
                500,
                json!({
                    "code": "hook_canvas_error",
                    "message": "Hook canvas workflow export is temporarily unavailable",
                }),
            );
        }
    };

    let workflow_name = request
        .workflow_name
        .clone()
        .unwrap_or_else(|| path_id.to_owned());

    // Topology YAML (kept for the existing workflow store / studio).
    let data = match document
        .export_workflow_yaml_for_selected_node(request.selected_node_id.trim(), &workflow_name)
    {
        Ok(data) => data,
        Err(hook_canvas::HookCanvasWorkflowExportError::NodeNotFound(node_id)) => {
            return structured_error(
                404,
                json!({
                    "code": "hook_canvas_node_not_found",
                    "message": format!("Hook canvas node `{node_id}` was not found"),
                }),
            );
        }
        Err(hook_canvas::HookCanvasWorkflowExportError::InvalidNode(node_id)) => {
            return structured_error(
                400,
                json!({
                    "code": "hook_canvas_node_invalid",
                    "message": format!("Hook canvas node `{node_id}` is not canonical"),
                }),
            );
        }
    };

    // Frozen full snapshot (geometry + crop) scoped to the selected component.
    let component =
        match document.component_snapshot_for_selected_node(request.selected_node_id.trim()) {
            Ok(component) => component,
            Err(hook_canvas::HookCanvasWorkflowExportError::NodeNotFound(node_id)) => {
                return structured_error(
                    404,
                    json!({
                        "code": "hook_canvas_node_not_found",
                        "message": format!("Hook canvas node `{node_id}` was not found"),
                    }),
                );
            }
            Err(hook_canvas::HookCanvasWorkflowExportError::InvalidNode(node_id)) => {
                return structured_error(
                    400,
                    json!({
                        "code": "hook_canvas_node_invalid",
                        "message": format!("Hook canvas node `{node_id}` is not canonical"),
                    }),
                );
            }
        };

    // Persist image copies for each member node, then rewrite preview URLs to the
    // saved-workflow preview route so the frozen snapshot renders without the
    // live Hook session.
    let images_dir = workflow_dir.join("images");
    if let Err(error) = fs::create_dir_all(&images_dir) {
        eprintln!("loom canvas workflow image dir failed: {error:#}");
        return structured_error(
            500,
            json!({
                "code": "canvas_workflow_write_failed",
                "message": "Unable to persist canvas workflow images",
            }),
        );
    }

    let mut saved_previews: HashMap<String, String> = HashMap::new();
    for (node_id, source) in &component.previews {
        let bytes = match source {
            hook_canvas::HookCanvasPreviewSource::DataUrl(data_url) => {
                match loom_image_io::decode_data_url_bytes(data_url) {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                }
            }
            hook_canvas::HookCanvasPreviewSource::File(path) => match fs::read(path) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            },
        };
        if bytes.len() as u64 > hook_canvas::MAX_PREVIEW_BYTES {
            continue;
        }
        let ext = canvas_workflow_preview_ext(source);
        let file_name = format!("{}.{ext}", sanitize_preview_file_stem(node_id));
        if fs::write(images_dir.join(&file_name), &bytes).is_ok() {
            saved_previews.insert(node_id.clone(), file_name);
        }
    }

    // Rewrite the frozen snapshot's preview URLs to the saved-workflow route.
    let mut snapshot = component.snapshot;
    for node in &mut snapshot.nodes {
        if saved_previews.contains_key(&node.id) {
            node.preview_available = true;
            node.preview_url = Some(format!(
                "/v1/hook-bridge/canvas/workflows/{}/nodes/{}/preview",
                percent_encode_path_segment(path_id),
                percent_encode_path_segment(&node.id),
            ));
        } else {
            node.preview_available = false;
            node.preview_url = None;
        }
    }

    let node_count = snapshot.nodes.len();
    let snapshot_json = match serde_json::to_string(&snapshot) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("loom canvas workflow snapshot serialize failed: {error:#}");
            return structured_error(
                500,
                json!({
                    "code": "canvas_workflow_write_failed",
                    "message": "Unable to serialize canvas workflow snapshot",
                }),
            );
        }
    };
    let saved_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let meta = json!({
        "id": path_id,
        "name": workflow_name,
        "nodeCount": node_count,
        "savedAt": saved_at,
    });
    if fs::write(workflow_dir.join("snapshot.json"), &snapshot_json).is_err()
        || fs::write(workflow_dir.join("meta.json"), meta.to_string()).is_err()
    {
        return structured_error(
            500,
            json!({
                "code": "canvas_workflow_write_failed",
                "message": "Unable to persist canvas workflow snapshot",
            }),
        );
    }

    // Persist the topology in the workflow store consumed by Workflow Studio.
    let workflow = match workflow_store.save_workflow(path_id, &data) {
        Ok(workflow) => workflow,
        Err(error) => return workflow_store_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "workflow": workflow,
            "sourceNodeId": request.selected_node_id,
            "workflowName": workflow_name,
            "canvasWorkflow": meta,
        }))?,
    ))
}
