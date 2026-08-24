// Shared-memory and shared-image routes, conversion, and error responses.
fn list_shared_images(shared_images: &SharedImageStoreHandle) -> Result<(u16, String)> {
    let images = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .list();
    Ok((200, serde_json::to_string(&json!({ "images": images }))?))
}

fn default_shared_memory_channels() -> u32 {
    4
}

fn list_shared_memory_buffers(shared_images: &SharedImageStoreHandle) -> Result<(u16, String)> {
    let images = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .list();
    let buffers: Vec<Value> = images.iter().map(shared_memory_buffer_info_json).collect();
    Ok((
        200,
        serde_json::to_string(&json!({
            "buffers": buffers,
            "images": images,
        }))?,
    ))
}

fn create_shared_memory_buffer(
    body: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let request: SharedMemoryCreateBufferRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    if request.channels != 4 {
        return invalid_request("Loom shared-memory buffers require rgba8 channels=4");
    }
    let size = match rgba8_buffer_size(request.width, request.height) {
        Ok(size) => size,
        Err(message) => return invalid_request(message),
    };
    let data = vec![0_u8; size];
    let image = match shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .create_rgba8(request.width, request.height, data)
    {
        Ok(image) => image,
        Err(error) => return shared_image_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "handle": &image.handle,
            "handle_name": &image.handle,
            "buffer": shared_memory_buffer_info_json(&image),
            "image": &image,
        }))?,
    ))
}

fn get_shared_memory_buffer_info(
    handle: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let store = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?;
    let Some(image) = store.get(handle) else {
        return shared_image_error_response(SharedImageError::NotFound(handle.to_owned()));
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "handle": handle,
            "buffer": shared_memory_buffer_info_json(&image),
            "image": &image,
        }))?,
    ))
}

fn release_shared_memory_buffer(
    handle: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let released = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .release(handle);
    if !released {
        return shared_image_error_response(SharedImageError::NotFound(handle.to_owned()));
    }
    Ok((
        200,
        serde_json::to_string(&json!({
            "handle": handle,
            "released": true,
            "deleted": true,
        }))?,
    ))
}

fn shared_memory_buffer_info_json(info: &SharedImageInfo) -> Value {
    json!({
        "handle": &info.handle,
        "handle_name": &info.handle,
        "size": info.size,
        "width": info.width,
        "height": info.height,
        "format": shared_image_format_name(&info.format),
        "ref_count": 1,
    })
}

fn shared_image_format_name(format: &SharedImageFormat) -> &'static str {
    match format {
        SharedImageFormat::Rgba8 => "rgba8",
    }
}

fn rgba8_buffer_size(width: u32, height: u32) -> std::result::Result<usize, String> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "shared-memory dimensions overflow".to_owned())?;
    usize::try_from(pixels).map_err(|_| "shared-memory buffer is too large".to_owned())
}

fn create_shared_image(
    body: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<Value>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let mut store = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?;

    let image = if let Some(data_url) = request.get("dataBase64").and_then(Value::as_str) {
        match store.create_from_data_url(data_url) {
            Ok(image) => image,
            Err(error) => return shared_image_error_response(error),
        }
    } else {
        let Some(width) = value_u32(&request, "width") else {
            return invalid_request("shared image width is required");
        };
        let Some(height) = value_u32(&request, "height") else {
            return invalid_request("shared image height is required");
        };
        let format = request
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or("rgba8");
        if format != "rgba8" {
            return invalid_request(format!("unsupported shared image format: {format}"));
        }
        let Some(data) = request.get("data").and_then(Value::as_array) else {
            return invalid_request("shared image data array is required");
        };
        let mut bytes = Vec::with_capacity(data.len());
        for value in data {
            let Some(byte) = value.as_u64().and_then(|value| u8::try_from(value).ok()) else {
                return invalid_request("shared image data must contain bytes");
            };
            bytes.push(byte);
        }
        match store.create_rgba8(width, height, bytes) {
            Ok(image) => image,
            Err(error) => return shared_image_error_response(error),
        }
    };

    Ok((200, serde_json::to_string(&json!({ "image": image }))?))
}

fn get_shared_image(handle: &str, shared_images: &SharedImageStoreHandle) -> Result<(u16, String)> {
    let store = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?;
    let Some(image) = store.get(handle) else {
        return shared_image_error_response(SharedImageError::NotFound(handle.to_owned()));
    };
    let data = match store.read_rgba8(handle) {
        Ok(data) => data,
        Err(error) => return shared_image_error_response(error),
    };
    let data_base64 = match store.read_png_data_url(handle) {
        Ok(data_url) => data_url,
        Err(error) => return shared_image_error_response(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "image": image,
            "data": data,
            "dataBase64": data_base64,
        }))?,
    ))
}

fn delete_shared_image(
    handle: &str,
    shared_images: &SharedImageStoreHandle,
) -> Result<(u16, String)> {
    let deleted = shared_images
        .lock()
        .map_err(|_| anyhow::anyhow!("lock shared image store"))?
        .release(handle);
    if !deleted {
        return shared_image_error_response(SharedImageError::NotFound(handle.to_owned()));
    }
    Ok((200, serde_json::to_string(&json!({ "deleted": true }))?))
}

fn convert_image_helper(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<ImageHelperConvertRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let source_type = request.source_type.as_str();
    let target_type = request.target_type.as_str();

    match (source_type, target_type) {
        ("image_base64", "image_buffer") => {
            let data = match request.data.as_ref().and_then(Value::as_str) {
                Some(data) => data,
                None => return invalid_request("image_base64 data is required"),
            };
            let rgba = match loom_image_io::decode_image_base64_to_rgba8(data) {
                Ok(rgba) => rgba,
                Err(error) => return image_io_error_response(error),
            };
            image_buffer_response(rgba)
        }
        ("image_base64", "image_base64") => {
            let data = match request.data.as_ref().and_then(Value::as_str) {
                Some(data) => data,
                None => return invalid_request("image_base64 data is required"),
            };
            let rgba = match loom_image_io::decode_image_base64_to_rgba8(data) {
                Ok(rgba) => rgba,
                Err(error) => return image_io_error_response(error),
            };
            let data_base64 =
                match loom_image_io::rgba8_to_png_data_url(rgba.width, rgba.height, &rgba.data) {
                    Ok(data_url) => data_url,
                    Err(error) => return image_io_error_response(error),
                };
            Ok((
                200,
                serde_json::to_string(&json!({ "dataBase64": data_base64 }))?,
            ))
        }
        ("image_path", "image_base64") => {
            let path = match request.path.as_deref() {
                Some(path) => path,
                None => return invalid_request("image_path path is required"),
            };
            let data_base64 = match loom_image_io::read_image_path_as_data_url(path) {
                Ok(data_url) => data_url,
                Err(error) => return image_io_error_response(error),
            };
            Ok((
                200,
                serde_json::to_string(&json!({ "dataBase64": data_base64 }))?,
            ))
        }
        ("image_path", "image_buffer") => {
            let path = match request.path.as_deref() {
                Some(path) => path,
                None => return invalid_request("image_path path is required"),
            };
            let data_url = match loom_image_io::read_image_path_as_data_url(path) {
                Ok(data_url) => data_url,
                Err(error) => return image_io_error_response(error),
            };
            let rgba = match loom_image_io::decode_image_base64_to_rgba8(&data_url) {
                Ok(rgba) => rgba,
                Err(error) => return image_io_error_response(error),
            };
            image_buffer_response(rgba)
        }
        ("image_buffer", "image_base64") => {
            let Some(width) = request.width else {
                return invalid_request("image_buffer width is required");
            };
            let Some(height) = request.height else {
                return invalid_request("image_buffer height is required");
            };
            let data = match request.data.as_ref().and_then(value_byte_array) {
                Some(data) => data,
                None => return invalid_request("image_buffer data array is required"),
            };
            let data_base64 = match loom_image_io::rgba8_to_png_data_url(width, height, &data) {
                Ok(data_url) => data_url,
                Err(error) => return image_io_error_response(error),
            };
            Ok((
                200,
                serde_json::to_string(&json!({ "dataBase64": data_base64 }))?,
            ))
        }
        ("image_buffer", "image_buffer") => {
            let Some(width) = request.width else {
                return invalid_request("image_buffer width is required");
            };
            let Some(height) = request.height else {
                return invalid_request("image_buffer height is required");
            };
            let data = match request.data.as_ref().and_then(value_byte_array) {
                Some(data) => data,
                None => return invalid_request("image_buffer data array is required"),
            };
            let size = data.len();
            Ok((
                200,
                serde_json::to_string(&json!({
                    "image": {
                        "width": width,
                        "height": height,
                        "format": "rgba8",
                        "size": size
                    },
                    "data": data
                }))?,
            ))
        }
        _ => invalid_request(format!(
            "unsupported image helper conversion: {source_type} to {target_type}"
        )),
    }
}

fn image_buffer_response(rgba: loom_image_io::RgbaImageData) -> Result<(u16, String)> {
    Ok((
        200,
        serde_json::to_string(&json!({
            "image": {
                "width": rgba.width,
                "height": rgba.height,
                "format": rgba.format,
                "size": rgba.size
            },
            "data": rgba.data
        }))?,
    ))
}

fn value_byte_array(value: &Value) -> Option<Vec<u8>> {
    value.as_array().map(|values| {
        values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|byte| u8::try_from(byte).ok())
                    .ok_or(())
            })
            .collect::<Result<Vec<_>, _>>()
            .ok()
    })?
}

fn image_io_error_response(error: loom_image_io::ImageIoError) -> Result<(u16, String)> {
    invalid_request(error.to_string())
}

fn value_u32(value: &Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn shared_image_error_response(error: SharedImageError) -> Result<(u16, String)> {
    match error {
        SharedImageError::NotFound(handle) => structured_error(
            404,
            json!({
                "code": "shared_image_not_found",
                "message": format!("shared image `{handle}` was not found"),
                "handle": handle,
            }),
        ),
        SharedImageError::Platform(message) => structured_error(
            500,
            json!({
                "code": "shared_image_platform_error",
                "message": message,
            }),
        ),
        other => invalid_request(other.to_string()),
    }
}
