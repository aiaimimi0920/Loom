fn node_coordinate(node: &Value, key: &str, source: HookCanvasSource) -> Option<f64> {
    match source {
        HookCanvasSource::Session => value_as_f64(node.get(key))
            .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(key)))),
        HookCanvasSource::Workflow => value_as_f64(node_nested_value(node, "position", key)),
        HookCanvasSource::Invalid => None,
    }
}

fn node_size(
    node: &Value,
    short_key: &str,
    long_key: &str,
    source: HookCanvasSource,
) -> Option<f64> {
    match source {
        HookCanvasSource::Session => value_as_f64(node.get(short_key))
            .or_else(|| value_as_f64(node.get(long_key)))
            .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(short_key))))
            .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(long_key)))),
        HookCanvasSource::Workflow => value_as_f64(node_nested_value(node, "measured", long_key))
            .or_else(|| value_as_f64(node.get(long_key))),
        HookCanvasSource::Invalid => None,
    }
}

// Build the crop viewport for a minified sticker, mirroring Hook's
// `computeMinifiedStickerViewport`: the source image is rendered at
// `savedRect` size and shifted by `cropOffset` so the node window shows a
// local region of the full image instead of the whole image scaled down.
// `imageEditState.cropRect/sourceSize` further refines the source region when
// present. Returns None when the node has no savedRect (nothing to crop).
fn extract_crop(node: &Value, window_width: f64, window_height: f64) -> Option<HookCanvasCrop> {
    if !(window_width > 0.0) || !(window_height > 0.0) {
        return None;
    }
    let saved_rect = node_value(node, "savedRect")?;
    let source_width = value_as_f64(saved_rect.get("w"))?;
    let source_height = value_as_f64(saved_rect.get("h"))?;
    if !(source_width > 0.0) || !(source_height > 0.0) {
        return None;
    }
    let base_offset_x = node_value(node, "cropOffset")
        .and_then(|value| value_as_f64(value.get("x")))
        .unwrap_or(0.0);
    let base_offset_y = node_value(node, "cropOffset")
        .and_then(|value| value_as_f64(value.get("y")))
        .unwrap_or(0.0);

    // Hook's getMinifiedViewport: imageEditState.cropRect/sourceSize refines the
    // laid-out source region when present, otherwise the whole savedRect is used.
    let (viewport_width, viewport_height, offset_x, offset_y) = node_value(node, "imageEditState")
        .and_then(|edit| {
            let crop_rect = edit.get("cropRect")?;
            let source_size = edit.get("sourceSize")?;
            let width = value_as_f64(source_size.get("w"))?;
            let height = value_as_f64(source_size.get("h"))?;
            let crop_x = value_as_f64(crop_rect.get("x"))?;
            let crop_y = value_as_f64(crop_rect.get("y"))?;
            (width > 0.0 && height > 0.0).then_some((
                width,
                height,
                crop_x + base_offset_x,
                crop_y + base_offset_y,
            ))
        })
        .unwrap_or((source_width, source_height, base_offset_x, base_offset_y));

    // Corner-click special case: Hook clamps the crop offset at minify time so
    // the crop window never leaves the image (useUnitActions):
    //   offset = clamp(raw, 0, max(0, savedRect - window))
    // A double-click near an edge/corner would otherwise push the window past the
    // image edge and expose blank space. We reproduce that clamp defensively so
    // the window's far edge aligns with the image edge regardless of what the
    // stored offset was.
    let max_offset_x = (viewport_width - window_width).max(0.0);
    let max_offset_y = (viewport_height - window_height).max(0.0);
    let offset_x = offset_x.clamp(0.0, max_offset_x);
    let offset_y = offset_y.clamp(0.0, max_offset_y);

    // Ratios relative to the node window (unit.w/unit.h), mirroring Hook's
    // `img { width: viewport.width px; left: -viewport.offsetX px }` inside a
    // `unit.w × unit.h` overflow-hidden box.
    Some(HookCanvasCrop {
        image_width_ratio: viewport_width / window_width,
        image_height_ratio: viewport_height / window_height,
        offset_x_ratio: offset_x / window_width,
        offset_y_ratio: offset_y / window_height,
    })
}

fn node_string(node: &Value, key: &str) -> Option<String> {
    non_empty_string(node_value(node, key))
}

fn node_type(node: &Value) -> Option<String> {
    non_empty_string(node.get("type"))
}
