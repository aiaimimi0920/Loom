fn is_image_like_port_name(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    normalized.contains("image")
        || DEFAULT_IMAGE_INPUTS.contains(&normalized.as_str())
        || normalized.ends_with("_image")
        || normalized.ends_with("_file")
}

fn connected_image_inputs<'a>(links: &'a [HookCanvasSessionLink]) -> HashMap<&'a str, &'a str> {
    let mut inputs = HashMap::new();
    for link in links {
        if link
            .to_port_id
            .as_deref()
            .is_some_and(is_image_like_port_name)
        {
            // Preserve the previous `find` behavior when several image inputs
            // target one node: the first canonical edge wins.
            inputs
                .entry(link.to_unit_id.as_str())
                .or_insert(link.from_unit_id.as_str());
        }
    }
    inputs
}

fn sticker_image_input_disabled(node: &Value) -> bool {
    node_nested_value(node, "params", "image")
        .and_then(Value::as_str)
        .is_some_and(|value| value == DISABLED_PREFIX)
}

fn sticker_manual_image_data_url(node: &Value) -> Option<String> {
    node_nested_value(node, "params", "image_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| is_supported_image_data_url(value))
        .map(str::to_owned)
}

fn sticker_annotation_count(node: &Value) -> usize {
    node_value(node, "annotationState")
        .and_then(|state| state.get("elements"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn has_meaningful_image_edit_state(node: &Value) -> bool {
    let Some(state) = node_value(node, "imageEditState") else {
        return false;
    };

    if state
        .get("contentEraseStrokes")
        .and_then(Value::as_array)
        .is_some_and(|strokes| !strokes.is_empty())
    {
        return true;
    }
    if state.get("cropRect").is_some_and(|value| !value.is_null()) {
        return true;
    }
    if state
        .get("sourceSize")
        .is_some_and(|value| !value.is_null())
    {
        return true;
    }
    if value_as_f64(state.get("rotation")).is_some_and(|rotation| rotation != 0.0) {
        return true;
    }
    if state
        .get("flippedX")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    if state
        .get("flippedY")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    if value_as_f64(state.get("borderWidth")).is_some_and(|width| width > 0.0) {
        return true;
    }
    if non_empty_string(state.get("borderColor")).is_some() {
        return true;
    }
    if value_as_f64(state.get("cornerRadius")).is_some_and(|radius| radius > 0.0) {
        return true;
    }

    state
        .get("beautify")
        .and_then(|beautify| beautify.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn sticker_requires_local_baked_preview(node: &Value) -> bool {
    sticker_annotation_count(node) > 0
        || node_string(node, "rasterizedAnnotationLayerSrc").is_some()
        || has_meaningful_image_edit_state(node)
}

fn push_normalized_preview_source(sources: &mut Vec<String>, value: Option<String>) {
    if let Some(raw) = value {
        if let Some(path) = normalize_preview_source(&raw) {
            if !sources.contains(&path) {
                sources.push(path);
            }
        }
    }
}

fn node_preview_only_sources(node: &Value) -> Vec<String> {
    let mut sources = Vec::new();
    push_normalized_preview_source(&mut sources, non_empty_string(node.get("previewSrc")));
    push_normalized_preview_source(
        &mut sources,
        node_data(node).and_then(|data| non_empty_string(data.get("previewSrc"))),
    );
    sources
}

fn node_src_fallback_sources(node: &Value) -> Vec<String> {
    let mut sources = Vec::new();
    push_normalized_preview_source(&mut sources, non_empty_string(node.get("src")));
    push_normalized_preview_source(
        &mut sources,
        node_data(node).and_then(|data| non_empty_string(data.get("src"))),
    );
    push_normalized_preview_source(&mut sources, non_empty_string(node.get("filePath")));
    push_normalized_preview_source(
        &mut sources,
        node_data(node).and_then(|data| non_empty_string(data.get("filePath"))),
    );
    sources
}

// Hook references a node's image through several shapes: a Tauri asset URL
// (`http://asset.localhost/<percent-encoded-path>`), a `file://` URL, a plain
// absolute path, or a clean `filePath` field. Return every candidate in
// preference order (a preview-sized image first, then the full image, then the
// raw file path) so the caller can pick the first that resolves to a real file.
fn node_preview_sources(node: &Value) -> Vec<String> {
    let mut sources = node_preview_only_sources(node);
    for source in node_src_fallback_sources(node) {
        if !sources.contains(&source) {
            sources.push(source);
        }
    }
    sources
}

fn resolve_effective_preview_source(
    node_id: &str,
    raw_nodes: &HashMap<String, Value>,
    image_inputs: &HashMap<&str, &str>,
    session_dir: &Path,
    preview_roots: &[PathBuf],
    cache: &mut HashMap<String, HookCanvasResolvedPreview>,
    visiting: &mut HashSet<String>,
    depth: usize,
) -> HookCanvasResolvedPreview {
    if let Some(cached) = cache.get(node_id) {
        return cached.clone();
    }
    if depth > MAX_PREVIEW_CHAIN_DEPTH {
        return HookCanvasResolvedPreview {
            depth_limited: true,
            ..HookCanvasResolvedPreview::default()
        };
    }
    if !visiting.insert(node_id.to_owned()) {
        return HookCanvasResolvedPreview::default();
    }

    let resolved = raw_nodes
        .get(node_id)
        .map_or_else(HookCanvasResolvedPreview::default, |node| {
            let node_kind = classify_node(
                node_type(node).as_deref(),
                node_string(node, "artId").as_deref(),
            );

            if matches!(node_kind, HookCanvasNodeKind::Screenshot) {
                if sticker_requires_local_baked_preview(node) {
                    let local_sources = node_preview_sources(node);
                    return HookCanvasResolvedPreview {
                        source: resolve_first_preview_source(
                            session_dir,
                            preview_roots,
                            &local_sources,
                        ),
                        had_candidates: !local_sources.is_empty(),
                        depth_limited: false,
                    };
                }

                let mut had_candidates = false;
                let mut depth_limited = false;
                if !sticker_image_input_disabled(node) {
                    if let Some(upstream_node_id) = image_inputs.get(node_id).copied() {
                        had_candidates = true;
                        let upstream = resolve_effective_preview_source(
                            upstream_node_id,
                            raw_nodes,
                            image_inputs,
                            session_dir,
                            preview_roots,
                            cache,
                            visiting,
                            depth + 1,
                        );
                        if upstream.source.is_some() {
                            return upstream;
                        }
                        had_candidates |= upstream.had_candidates;
                        depth_limited |= upstream.depth_limited;
                    }
                    if let Some(manual_image) = sticker_manual_image_data_url(node) {
                        let source =
                            resolve_preview_source(session_dir, preview_roots, &manual_image);
                        return HookCanvasResolvedPreview {
                            source,
                            had_candidates: true,
                            depth_limited: false,
                        };
                    }
                }

                let local_sources = node_preview_sources(node);
                return HookCanvasResolvedPreview {
                    source: resolve_first_preview_source(
                        session_dir,
                        preview_roots,
                        &local_sources,
                    ),
                    had_candidates: had_candidates || !local_sources.is_empty(),
                    depth_limited,
                };
            }

            let local_sources = node_preview_sources(node);
            if let Some(source) =
                resolve_first_preview_source(session_dir, preview_roots, &local_sources)
            {
                return HookCanvasResolvedPreview {
                    source: Some(source),
                    had_candidates: true,
                    depth_limited: false,
                };
            }

            let mut had_candidates = !local_sources.is_empty();
            let mut depth_limited = false;
            if let Some(upstream_node_id) = image_inputs.get(node_id).copied() {
                had_candidates = true;
                let upstream = resolve_effective_preview_source(
                    upstream_node_id,
                    raw_nodes,
                    image_inputs,
                    session_dir,
                    preview_roots,
                    cache,
                    visiting,
                    depth + 1,
                );
                if upstream.source.is_some() {
                    return upstream;
                }
                had_candidates |= upstream.had_candidates;
                depth_limited |= upstream.depth_limited;
            }
            HookCanvasResolvedPreview {
                source: None,
                had_candidates,
                depth_limited,
            }
        });

    visiting.remove(node_id);
    if !resolved.depth_limited {
        cache.insert(node_id.to_owned(), resolved.clone());
    }
    resolved
}
