// Hook Art input materialization, results, OCR and translation, and broadcasts.
fn materialize_hook_art_inputs(
    inputs: &BTreeMap<String, HookArtPortValue>,
    shared_images: &SharedImageStoreHandle,
) -> std::result::Result<Value, String> {
    let mut materialized = serde_json::Map::new();
    for (name, input) in inputs {
        let value = match input {
            HookArtPortValue::Value { value } => value.clone(),
            HookArtPortValue::InlineResource {
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
            HookArtPortValue::SharedMemory {
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
            HookArtPortValue::Resource { .. } => {
                return Err(format!(
                    "broker resource input `{name}` requires an attachment-scoped lease"
                ))
            }
        };
        materialized.insert(name.clone(), value);
    }
    Ok(Value::Object(materialized))
}

fn hook_art_result_outputs(
    tool: &ToolDefinition,
    result: &Value,
    shared_images: &SharedImageStoreHandle,
    allow_shared_memory: bool,
) -> BTreeMap<String, HookArtPortValue> {
    let output_id = hook_art_primary_output_id(tool);
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
                hook_art_output_value(value, shared_images, allow_shared_memory),
            );
        }
    }
    if !outputs.contains_key(&output_id) {
        if let Some(value) =
            hook_art_primary_output_value(result, shared_images, allow_shared_memory)
        {
            outputs.insert(output_id.clone(), value);
        }
    }
    if outputs.is_empty() {
        outputs.insert(
            output_id,
            HookArtPortValue::Value {
                value: result.clone(),
            },
        );
    }
    outputs
}

fn hook_art_port_resource_handles(value: &HookArtPortValue) -> BTreeSet<String> {
    match value {
        HookArtPortValue::SharedMemory { handle, .. } => BTreeSet::from([handle.clone()]),
        _ => BTreeSet::new(),
    }
}

fn hook_art_output_resource_handles(
    outputs: &BTreeMap<String, HookArtPortValue>,
) -> BTreeSet<String> {
    outputs
        .values()
        .filter_map(|value| match value {
            HookArtPortValue::SharedMemory { handle, .. } => Some(handle.clone()),
            _ => None,
        })
        .collect()
}

fn hook_art_primary_output_value(
    result: &Value,
    shared_images: &SharedImageStoreHandle,
    allow_shared_memory: bool,
) -> Option<HookArtPortValue> {
    if let Some(data_url) = extract_art_image_data_url(result) {
        return Some(hook_art_image_value(
            &data_url,
            shared_images,
            allow_shared_memory,
        ));
    }
    if result.get("type").and_then(Value::as_str) == Some("shader") {
        return Some(HookArtPortValue::Value {
            value: result.clone(),
        });
    }
    None
}

fn hook_art_output_value(
    value: &Value,
    shared_images: &SharedImageStoreHandle,
    allow_shared_memory: bool,
) -> HookArtPortValue {
    hook_art_primary_output_value(value, shared_images, allow_shared_memory).unwrap_or_else(|| {
        HookArtPortValue::Value {
            value: value.clone(),
        }
    })
}

fn hook_art_image_value(
    data_url: &str,
    shared_images: &SharedImageStoreHandle,
    allow_shared_memory: bool,
) -> HookArtPortValue {
    hook_art_image_value_with(data_url, || {
        allow_shared_memory
            .then(|| hook_art_shared_image_value(data_url, shared_images))
            .unwrap_or_else(|| Err("shared memory was not negotiated".to_owned()))
    })
}

fn hook_art_image_value_with(
    data_url: &str,
    create_shared_image: impl FnOnce() -> std::result::Result<HookArtPortValue, String>,
) -> HookArtPortValue {
    create_shared_image().unwrap_or_else(|_| {
        let (mime, data_base64) = data_url
            .strip_prefix("data:")
            .and_then(|value| value.split_once(";base64,"))
            .unwrap_or(("image/png", data_url));
        let dimensions = loom_image_io::decode_image_base64_to_rgba8(data_url)
            .ok()
            .map(|image| (image.width, image.height));
        HookArtPortValue::InlineResource {
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

fn hook_art_shared_image_value(
    data_url: &str,
    shared_images: &SharedImageStoreHandle,
) -> std::result::Result<HookArtPortValue, String> {
    let image = match shared_images.lock() {
        Ok(mut store) => store.create_from_data_url(data_url),
        Err(poisoned) => poisoned.into_inner().create_from_data_url(data_url),
    }
    .map_err(|error| error.to_string())?;
    Ok(HookArtPortValue::SharedMemory {
        handle: image.handle,
        size: u64::try_from(image.size).unwrap_or(u64::MAX),
        width: image.width,
        height: image.height,
        format: "rgba8".to_owned(),
    })
}

fn hook_art_primary_output_id(tool: &ToolDefinition) -> String {
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

fn hook_protocol_event_json(method: &str, params: &impl Serialize) -> String {
    let params = serde_json::to_value(params).unwrap_or_default();
    serde_json::to_string(&HookEvent {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        method: method.to_owned(),
        params,
    })
    .unwrap_or_else(|error| hook_protocol_failure_json("event", "serialization_failed", error))
}

fn translate_text_via_provider(
    text: &str,
    target_lang: &str,
) -> std::result::Result<Option<String>, String> {
    let endpoint = match std::env::var("LOOM_TRANSLATE_ENDPOINT") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return Ok(None),
    };

    let client = loom_tool_registry::network_policy::apply_runtime_proxy(
        reqwest::blocking::Client::builder(),
    )
    .map_err(|error| format!("configure translate provider proxy: {error}"))?
    .timeout(Duration::from_secs(15))
    .build()
    .map_err(|error| format!("build translate provider client: {error}"))?;
    let response = client
        .post(endpoint)
        .json(&json!({
            "text": text,
            "target_lang": target_lang,
            "source_lang": "auto"
        }))
        .send()
        .map_err(|error| format!("translate provider request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("read translate provider response: {error}"))?;
    if !status.is_success() {
        return Err(format!("translate provider returned {status}: {body}"));
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| format!("translate provider returned invalid JSON: {error}"))?;
    value
        .get("translated_text")
        .or_else(|| value.get("data"))
        .or_else(|| value.get("translation"))
        .and_then(Value::as_str)
        .map(|translated| Some(translated.to_owned()))
        .ok_or_else(|| "translate provider response missing translated text".to_owned())
}

fn execute_hook_ocr(
    image_base64: &str,
    ocr_provider: &OcrProviderHandle,
) -> std::result::Result<Value, String> {
    let mut provider = match ocr_provider.lock() {
        Ok(provider) => provider,
        Err(_) => return Err("OCR enhancement unavailable".to_owned()),
    };

    match &mut *provider {
        OcrProvider::Unavailable => Err("OCR enhancement unavailable".to_owned()),
        OcrProvider::Fixture { text } => {
            let rgba = loom_image_io::decode_image_base64_to_rgba8(image_base64)
                .map_err(|error| error.to_string())?;
            Ok(json!({ "text": text, "width": rgba.width, "height": rgba.height }))
        }
        OcrProvider::Real { engine } => {
            let image_bytes = loom_image_io::decode_data_url_bytes(image_base64)
                .map_err(|error| error.to_string())?;
            let result = engine
                .detect_image_bytes(&image_bytes, false)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(result).map_err(|error| error.to_string())
        }
    }
}

fn register_hook_bridge_subscription(
    hub: &HookBridgeBroadcastHub,
    channels: Vec<String>,
) -> (Receiver<String>, HookBridgeSubscriptionGuard) {
    let (tx, rx) = mpsc::channel();
    let id = hub.next_subscriber_id.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut subscribers) = hub.subscribers.lock() {
        subscribers.push(HookBridgeSubscriber { id, tx, channels });
    }
    (
        rx,
        HookBridgeSubscriptionGuard {
            id,
            subscribers: Arc::clone(&hub.subscribers),
        },
    )
}

fn drain_hook_bridge_broadcasts(
    websocket: &mut tungstenite::WebSocket<std::net::TcpStream>,
    rx: &Receiver<String>,
) -> bool {
    loop {
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
}

fn broadcast_hook_bridge_messages(hub: &HookBridgeBroadcastHub, broadcasts: &[String]) {
    let _ = broadcast_hook_bridge_messages_with_count(hub, broadcasts);
}

fn broadcast_hook_bridge_messages_with_count(
    hub: &HookBridgeBroadcastHub,
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
            subscriber.tx.send(broadcast.clone()).is_ok()
        });
        if retained && accepted {
            delivered += 1;
        }
        retained
    });
    delivered
}

fn broadcast_hook_bridge_json(hook_bridge: &SharedHookBridgeRuntime, broadcast: impl Serialize) {
    let serialized = match serde_json::to_string(&broadcast) {
        Ok(serialized) => serialized,
        Err(_) => return,
    };
    let hub = match hook_bridge.lock() {
        Ok(runtime) => runtime.broadcast_hub.clone(),
        Err(_) => return,
    };
    broadcast_hook_bridge_messages(&hub, &[serialized]);
}

fn surface_snapshot_recovery_messages(
    surface_instances: &SharedSurfaceInstanceStore,
) -> Vec<String> {
    surface_snapshot_recovery_messages_for_device(surface_instances, None)
}
