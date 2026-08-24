fn read_session_value(session_path: &Path) -> Result<Option<(Vec<u8>, Value)>, HookCanvasError> {
    read_session_value_with(
        || read_session_bytes(session_path),
        || thread::sleep(SESSION_READ_RETRY_DELAY),
    )
}

// Read at most one byte beyond the contract limit. This keeps a malformed or
// attacker-controlled session from making `fs::read` allocate the whole file.
fn read_session_bytes(session_path: &Path) -> std::io::Result<Vec<u8>> {
    let file = fs::File::open(session_path)?;
    let capacity = file
        .metadata()
        .ok()
        .map(|metadata| metadata.len().min(MAX_HOOK_SESSION_BYTES as u64 + 1) as usize)
        .unwrap_or_default();
    let mut bytes = Vec::with_capacity(capacity);
    file.take(MAX_HOOK_SESSION_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read_session_value_with<Read, Wait>(
    read: Read,
    wait: Wait,
) -> Result<Option<(Vec<u8>, Value)>, HookCanvasError>
where
    Read: FnMut() -> std::io::Result<Vec<u8>>,
    Wait: FnMut(),
{
    read_session_value_with_limits(read, wait, MAX_HOOK_SESSION_BYTES, MAX_HOOK_SESSION_DEPTH)
}

fn read_session_value_with_limits<Read, Wait>(
    mut read: Read,
    mut wait: Wait,
    max_bytes: usize,
    max_depth: usize,
) -> Result<Option<(Vec<u8>, Value)>, HookCanvasError>
where
    Read: FnMut() -> std::io::Result<Vec<u8>>,
    Wait: FnMut(),
{
    let mut last_json_error = None;

    for attempt in 0..SESSION_READ_ATTEMPTS {
        let bytes = match read() {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound && attempt == 0 => return Ok(None),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if attempt + 1 < SESSION_READ_ATTEMPTS {
                    wait();
                }
                continue;
            }
            Err(error) => return Err(HookCanvasError::Read(error)),
        };
        if bytes.len() > max_bytes {
            return Err(HookCanvasError::Limit(format!(
                "session JSON exceeds the {max_bytes} byte limit"
            )));
        }

        let root = match serde_json::from_slice(&bytes) {
            Ok(root) => root,
            Err(error) => {
                last_json_error = Some(error);
                if attempt + 1 < SESSION_READ_ATTEMPTS {
                    wait();
                }
                continue;
            }
        };
        if !loom_security::json::value_is_within_depth(&root, max_depth) {
            return Err(HookCanvasError::Limit(format!(
                "session JSON exceeds the nesting limit of {max_depth} levels"
            )));
        }
        return Ok(Some((bytes, root)));
    }

    match last_json_error {
        Some(error) => Err(HookCanvasError::Json(error)),
        None => Ok(None),
    }
}

#[derive(Clone, Copy)]
enum HookCanvasSource {
    Session,
    Workflow,
    Invalid,
}

#[derive(Clone, Copy)]
enum EdgeEnd {
    Source,
    Target,
}

fn hook_canvas_source(root: &Value) -> HookCanvasSource {
    let has_session_keys = root.get("stickers").is_some() || root.get("links").is_some();
    let has_workflow_keys = root.get("nodes").is_some() || root.get("edges").is_some();
    let valid_session = root.get("stickers").is_some_and(Value::is_array)
        && root.get("links").is_some_and(Value::is_array);
    let valid_workflow = root.get("nodes").is_some_and(Value::is_array)
        && root.get("edges").is_some_and(Value::is_array);
    match (
        valid_session,
        valid_workflow,
        has_session_keys,
        has_workflow_keys,
    ) {
        (true, false, true, false) => HookCanvasSource::Session,
        (false, true, false, true) => HookCanvasSource::Workflow,
        _ => HookCanvasSource::Invalid,
    }
}

fn canvas_nodes(root: &Value, source: HookCanvasSource) -> &[Value] {
    let key = match source {
        HookCanvasSource::Session => "stickers",
        HookCanvasSource::Workflow => "nodes",
        HookCanvasSource::Invalid => return &[],
    };
    root.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn canvas_edges(root: &Value, source: HookCanvasSource) -> &[Value] {
    let key = match source {
        HookCanvasSource::Session => "links",
        HookCanvasSource::Workflow => "edges",
        HookCanvasSource::Invalid => return &[],
    };
    root.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn edge_endpoint(raw_edge: &Value, source: HookCanvasSource, end: EdgeEnd) -> Option<String> {
    let key = match (source, end) {
        (HookCanvasSource::Session, EdgeEnd::Source) => "fromUnitId",
        (HookCanvasSource::Session, EdgeEnd::Target) => "toUnitId",
        (HookCanvasSource::Workflow, EdgeEnd::Source) => "source",
        (HookCanvasSource::Workflow, EdgeEnd::Target) => "target",
        (HookCanvasSource::Invalid, _) => return None,
    };
    first_non_empty_string(raw_edge, &[key])
}

fn edge_port(raw_edge: &Value, source: HookCanvasSource, end: EdgeEnd) -> Option<String> {
    let key = match (source, end) {
        (HookCanvasSource::Session, EdgeEnd::Source) => "fromPortId",
        (HookCanvasSource::Session, EdgeEnd::Target) => "toPortId",
        (HookCanvasSource::Workflow, EdgeEnd::Source) => "sourceHandle",
        (HookCanvasSource::Workflow, EdgeEnd::Target) => "targetHandle",
        (HookCanvasSource::Invalid, _) => return None,
    };
    first_non_empty_string(raw_edge, &[key])
}

fn node_data(node: &Value) -> Option<&Value> {
    node.get("data").filter(|value| value.is_object())
}

fn node_value<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    node.get(key).or_else(|| node_data(node)?.get(key))
}

fn node_nested_value<'a>(node: &'a Value, container: &str, key: &str) -> Option<&'a Value> {
    node.get(container)
        .and_then(|value| value.get(key))
        .or_else(|| node_data(node)?.get(container)?.get(key))
}

fn node_art_result_metadata(node: &Value) -> Option<&Value> {
    node_value(node, "loomMetadata").and_then(|metadata| metadata.get("candidates"))
}

fn node_result_candidates(node: &Value) -> Vec<HookCanvasResultCandidate> {
    let metadata = node_art_result_metadata(node);
    let items = metadata
        .and_then(|metadata| metadata.get("items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let image_url = item.get("imageUrl").and_then(Value::as_str)?.to_owned();
                    Some(HookCanvasResultCandidate {
                        index: item.get("index").and_then(Value::as_u64).unwrap_or(0) as usize,
                        title: item.get("title").and_then(Value::as_str).map(str::to_owned),
                        image_url,
                        thumbnail: item
                            .get("thumbnail")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        preview: item
                            .get("preview")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        thumbnail_url: item
                            .get("thumbnailUrl")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        source_page_url: item
                            .get("sourcePageUrl")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        width: item.get("width").and_then(Value::as_u64),
                        height: item.get("height").and_then(Value::as_u64),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    items
}

fn node_selected_result_index(node: &Value, params: &Value) -> Option<usize> {
    node_art_result_metadata(node)
        .and_then(|metadata| metadata.get("selectedIndex"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .or_else(|| {
            params
                .get("result_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
}
