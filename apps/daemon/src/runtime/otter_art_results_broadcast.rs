// Otter Art input materialization, results, and broadcasts.
fn materialize_otter_art_inputs(
    inputs: &BTreeMap<String, OtterArtPortValue>,
    shared_images: &SharedImageStoreHandle,
) -> std::result::Result<Value, String> {
    let mut materialized = serde_json::Map::new();
    for (name, input) in inputs {
        let value = match input {
            OtterArtPortValue::Value { value } => value.clone(),
            OtterArtPortValue::InlineResource {
                mime, data_base64, ..
            } => {
                if !mime.starts_with("image/") {
                    return Err(format!("inline input `{name}` must be an image"));
                }
                if data_base64.starts_with("data:") {
                    return Err(format!(
                        "inline input `{name}` dataBase64 must contain bare base64 data"
                    ));
                }
                BASE64.decode(data_base64).map_err(|error| {
                    format!("inline input `{name}` is not valid base64: {error}")
                })?;
                Value::String(format!("data:{mime};base64,{data_base64}"))
            }
            OtterArtPortValue::SharedMemory {
                handle,
                size,
                width,
                height,
                format,
            } => {
                if format != "rgba8" {
                    return Err(format!("shared-memory input `{name}` must use rgba8"));
                }
                let size = usize::try_from(*size)
                    .map_err(|_| format!("shared-memory input `{name}` size exceeds usize"))?;
                let expected = usize::try_from(u64::from(*width) * u64::from(*height) * 4)
                    .map_err(|_| format!("shared-memory input `{name}` dimensions overflow"))?;
                if size != expected {
                    return Err(format!(
                        "shared-memory input `{name}` size mismatch: expected {expected}, got {size}"
                    ));
                }
                let bytes = shared_images
                    .lock()
                    .map_err(|_| "lock shared image store".to_owned())?
                    .read_rgba8_or_open(handle, size)
                    .map_err(|error| error.to_string())?;
                Value::String(
                    loom_shared_image::rgba8_to_png_data_url(*width, *height, bytes)
                        .map_err(|error| error.to_string())?,
                )
            }
            OtterArtPortValue::Resource { .. } => {
                return Err(format!(
                    "broker resource input `{name}` requires an attachment-scoped lease"
                ))
            }
        };
        materialized.insert(name.clone(), value);
    }
    Ok(Value::Object(materialized))
}

fn otter_art_result_outputs(
    tool: &ToolDefinition,
    result: &Value,
    shared_images: &SharedImageStoreHandle,
    allow_shared_memory: bool,
) -> BTreeMap<String, OtterArtPortValue> {
    let output_id = otter_art_primary_output_id(tool);
    let mut outputs = BTreeMap::new();
    for output in &tool.outputs {
        let Some(id) = output
            .get("name")
            .or_else(|| output.get("id"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if let Some(value) = result.get(id) {
            outputs.insert(
                id.to_owned(),
                otter_art_output_value(value, shared_images, allow_shared_memory),
            );
        }
    }
    if !outputs.contains_key(&output_id) {
        if let Some(value) =
            otter_art_primary_output_value(result, shared_images, allow_shared_memory)
        {
            outputs.insert(output_id.clone(), value);
        }
    }
    if outputs.is_empty() {
        outputs.insert(
            output_id,
            OtterArtPortValue::Value {
                value: result.clone(),
            },
        );
    }
    outputs
}

fn otter_art_port_resource_handles(value: &OtterArtPortValue) -> BTreeSet<String> {
    match value {
        OtterArtPortValue::SharedMemory { handle, .. } => BTreeSet::from([handle.clone()]),
        _ => BTreeSet::new(),
    }
}

fn otter_art_output_resource_handles(
    outputs: &BTreeMap<String, OtterArtPortValue>,
) -> BTreeSet<String> {
    outputs
        .values()
        .filter_map(|value| match value {
            OtterArtPortValue::SharedMemory { handle, .. } => Some(handle.clone()),
            _ => None,
        })
        .collect()
}

fn otter_art_primary_output_value(
    result: &Value,
    shared_images: &SharedImageStoreHandle,
    allow_shared_memory: bool,
) -> Option<OtterArtPortValue> {
    if let Some(data_url) = extract_art_image_data_url(result) {
        return Some(otter_art_image_value(
            &data_url,
            shared_images,
            allow_shared_memory,
        ));
    }
    if result.get("type").and_then(Value::as_str) == Some("shader") {
        return Some(OtterArtPortValue::Value {
            value: result.clone(),
        });
    }
    None
}

fn otter_art_output_value(
    value: &Value,
    shared_images: &SharedImageStoreHandle,
    allow_shared_memory: bool,
) -> OtterArtPortValue {
    otter_art_primary_output_value(value, shared_images, allow_shared_memory).unwrap_or_else(|| {
        OtterArtPortValue::Value {
            value: value.clone(),
        }
    })
}

fn otter_art_image_value(
    data_url: &str,
    shared_images: &SharedImageStoreHandle,
    allow_shared_memory: bool,
) -> OtterArtPortValue {
    otter_art_image_value_with(data_url, || {
        allow_shared_memory
            .then(|| otter_art_shared_image_value(data_url, shared_images))
            .unwrap_or_else(|| Err("shared memory was not negotiated".to_owned()))
    })
}

fn otter_art_image_value_with(
    data_url: &str,
    create_shared_image: impl FnOnce() -> std::result::Result<OtterArtPortValue, String>,
) -> OtterArtPortValue {
    create_shared_image().unwrap_or_else(|_| {
        let (mime, data_base64) = data_url
            .strip_prefix("data:")
            .and_then(|value| value.split_once(";base64,"))
            .unwrap_or(("image/png", data_url));
        let dimensions = loom_image_io::decode_image_base64_to_rgba8(data_url)
            .ok()
            .map(|image| (image.width, image.height));
        OtterArtPortValue::InlineResource {
            mime: mime.to_owned(),
            data_base64: data_base64.to_owned(),
            width: dimensions.map(|value| value.0),
            height: dimensions.map(|value| value.1),
        }
    })
}

fn extract_art_image_data_url(value: &Value) -> Option<String> {
    const OUTPUT_KEYS: &[&str] = &[
        "output_base64",
        "outputBase64",
        "image_base64",
        "imageBase64",
        "data_url",
        "dataUrl",
        "data",
        "image",
        "output",
        "result",
        "content",
    ];
    if let Some(content) = value.get("content").and_then(Value::as_array) {
        if let Some(data) = content.iter().find_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("image"))
                .then(|| item.get("data").and_then(Value::as_str))
                .flatten()
        }) {
            return normalize_art_image_data_url(
                data,
                content
                    .iter()
                    .find(|item| item.get("data").and_then(Value::as_str) == Some(data))
                    .and_then(|item| item.get("mimeType").and_then(Value::as_str)),
            );
        }
    }
    match value {
        Value::String(value) if value.trim_start().starts_with("data:image/") => {
            Some(value.clone())
        }
        Value::Object(object) => OUTPUT_KEYS
            .iter()
            .filter_map(|key| object.get(*key))
            .find_map(extract_art_image_data_url),
        Value::Array(values) => values.iter().find_map(extract_art_image_data_url),
        _ => None,
    }
}

fn normalize_art_image_data_url(data: &str, mime: Option<&str>) -> Option<String> {
    if data.trim_start().starts_with("data:image/") {
        return Some(data.to_owned());
    }
    let mime = mime.filter(|value| value.starts_with("image/"))?;
    BASE64.decode(data).ok()?;
    Some(format!("data:{mime};base64,{data}"))
}

fn otter_art_shared_image_value(
    data_url: &str,
    shared_images: &SharedImageStoreHandle,
) -> std::result::Result<OtterArtPortValue, String> {
    let image = match shared_images.lock() {
        Ok(mut store) => store.create_from_data_url(data_url),
        Err(poisoned) => poisoned.into_inner().create_from_data_url(data_url),
    }
    .map_err(|error| error.to_string())?;
    Ok(OtterArtPortValue::SharedMemory {
        handle: image.handle,
        size: u64::try_from(image.size).unwrap_or(u64::MAX),
        width: image.width,
        height: image.height,
        format: "rgba8".to_owned(),
    })
}

fn otter_art_primary_output_id(tool: &ToolDefinition) -> String {
    tool.outputs
        .first()
        .and_then(|output| {
            output
                .get("name")
                .or_else(|| output.get("id"))
                .and_then(Value::as_str)
        })
        .unwrap_or("output")
        .to_owned()
}

fn otter_protocol_event_json(method: &str, params: &impl Serialize) -> String {
    let params = serde_json::to_value(params).unwrap_or_default();
    serde_json::to_string(&HookEvent {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        method: method.to_owned(),
        params,
    })
    .unwrap_or_else(|error| otter_protocol_failure_json("event", "serialization_failed", error))
}

fn register_otter_bridge_subscription(
    hub: &OtterBridgeBroadcastHub,
    channels: Vec<String>,
) -> (BroadcastReceiver, OtterBridgeSubscriptionGuard) {
    let (tx, rx) = otter_broadcast_queue::channel();
    let id = hub.next_subscriber_id.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut subscribers) = hub.subscribers.lock() {
        subscribers.push(OtterBridgeSubscriber { id, tx, channels });
    }
    (
        rx,
        OtterBridgeSubscriptionGuard {
            id,
            subscribers: Arc::clone(&hub.subscribers),
        },
    )
}

fn drain_otter_bridge_broadcasts<S: std::io::Read + std::io::Write>(
    websocket: &mut tungstenite::WebSocket<S>,
    rx: &BroadcastReceiver,
) -> bool {
    // Give cancellation and incoming requests a turn even when producers keep refilling.
    for _ in 0..otter_broadcast_queue::MAX_MESSAGES {
        match rx.try_recv() {
            Ok(message) => {
                if websocket.send(tungstenite::Message::Text(message)).is_err() {
                    return false;
                }
            }
            Err(mpsc::TryRecvError::Empty) => return true,
            Err(mpsc::TryRecvError::Disconnected) => return false,
        }
    }
    true
}

fn broadcast_otter_bridge_messages(hub: &OtterBridgeBroadcastHub, broadcasts: &[String]) {
    let _ = broadcast_otter_bridge_messages_with_count(hub, broadcasts);
}

fn broadcast_otter_bridge_messages_with_count(
    hub: &OtterBridgeBroadcastHub,
    broadcasts: &[String],
) -> usize {
    if broadcasts.is_empty() {
        return 0;
    }
    hub.record(broadcasts);
    let Ok(mut subscribers) = hub.subscribers.lock() else {
        return 0;
    };
    let mut delivered = 0;
    subscribers.retain(|subscriber| {
        let mut accepted = false;
        let retained = broadcasts.iter().all(|broadcast| {
            if !subscriber_accepts_broadcast(subscriber, broadcast) {
                return true;
            }
            accepted = true;
            subscriber.tx.try_send(broadcast)
        });
        if retained && accepted {
            delivered += 1;
        }
        retained
    });
    delivered
}

fn broadcast_otter_bridge_json(hook_bridge: &SharedOtterBridgeRuntime, broadcast: impl Serialize) {
    let serialized = match serde_json::to_string(&broadcast) {
        Ok(serialized) => serialized,
        Err(_) => return,
    };
    let hub = match hook_bridge.lock() {
        Ok(runtime) => runtime.broadcast_hub.clone(),
        Err(_) => return,
    };
    broadcast_otter_bridge_messages(&hub, &[serialized]);
}

fn surface_snapshot_recovery_messages(
    surface_instances: &SharedSurfaceInstanceStore,
) -> Vec<String> {
    surface_snapshot_recovery_messages_for_device(surface_instances, None)
}
