use super::*;

pub(super) fn normalize_framework_image_output(
    tool: &ToolDefinition,
    framework: &str,
    output: &mut Value,
    allowed_roots: &[&Path],
) -> ToolRegistryResult<()> {
    if !tool.outputs.iter().any(is_image_output_definition) {
        return Ok(());
    }
    let Some(path) = framework_image_output_path(output) else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(framework_image_output_error(
            tool,
            framework,
            "image output path must be absolute",
        ));
    }
    let canonical_path = fs::canonicalize(&path).map_err(|error| {
        framework_image_output_error(
            tool,
            framework,
            format!("cannot resolve image output path: {error}"),
        )
    })?;
    if !canonical_path.is_file() {
        return Err(framework_image_output_error(
            tool,
            framework,
            "image output path is not a file",
        ));
    }
    let inside_allowed_root = allowed_roots.iter().any(|root| {
        fs::canonicalize(root)
            .ok()
            .is_some_and(|canonical_root| canonical_path.starts_with(canonical_root))
    });
    if !inside_allowed_root {
        return Err(framework_image_output_error(
            tool,
            framework,
            "image output path resolves outside the execution output roots",
        ));
    }
    let bytes = fs::metadata(&canonical_path)
        .map_err(|error| {
            framework_image_output_error(
                tool,
                framework,
                format!("cannot inspect image output: {error}"),
            )
        })?
        .len();
    if bytes > MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES {
        return Err(framework_image_output_error(
            tool,
            framework,
            format!(
                "image output exceeds the {} byte limit",
                MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES
            ),
        ));
    }
    let image =
        loom_image_io::read_image_path_as_web_data_url(&canonical_path).map_err(|error| {
            framework_image_output_error(
                tool,
                framework,
                format!("cannot decode image output: {error}"),
            )
        })?;
    let content = json!([{
        "type": "image",
        "data": image.data_url,
        // The label comes from the bytes rather than from a constant. It used to say `image/png`
        // unconditionally, which was true only because the reader re-encoded everything to PNG; now
        // that a JPEG or WebP output is passed through as itself, a fixed label would be a lie to
        // every consumer that trusts the field over the data URL's own prefix.
        "mimeType": image.mime_type
    }]);
    match output {
        Value::Object(object) => {
            for key in ["output_path", "outputPath", "file_path", "filePath", "path"] {
                object.remove(key);
            }
            // The shared Art runtime emits `output_base64` beside a `content` block of its own, so
            // the data URL the host just built from the validated file is a second full copy of the
            // same image. Drop the self-declared one: it was never checked against the output roots
            // or the size limit, and every reader in the workspace falls back to `content[0].data`
            // when it is absent.
            for key in ["output_base64", "outputBase64"] {
                object.remove(key);
            }
            object.insert("content".to_owned(), content);
        }
        value => {
            *value = json!({ "content": content });
        }
    }
    Ok(())
}

pub(super) fn is_image_output_definition(output: &Value) -> bool {
    let Some(output) = output.as_object() else {
        return false;
    };
    output
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("image"))
        || output
            .get("executionType")
            .or_else(|| output.get("execution_type"))
            .and_then(Value::as_str)
            .is_some_and(|value| value.to_ascii_lowercase().starts_with("image_"))
}

fn framework_image_output_path(output: &Value) -> Option<&str> {
    match output {
        Value::String(path) => Some(path),
        Value::Object(object) => ["output_path", "outputPath", "file_path", "filePath", "path"]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_str)),
        _ => None,
    }
}

fn framework_image_output_error(
    tool: &ToolDefinition,
    framework: &str,
    reason: impl Into<String>,
) -> ToolRegistryError {
    ToolRegistryError::FrameworkProcessProtocol {
        id: tool.id.clone(),
        framework: framework.to_owned(),
        reason: reason.into(),
    }
}

pub(super) fn map_process_error(
    tool: &ToolDefinition,
    framework: &str,
    timeout: Duration,
    error: ProcessError,
) -> ToolRegistryError {
    match error {
        ProcessError::Spawn(error) => ToolRegistryError::FrameworkProcessSpawn {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: error.to_string(),
        },
        ProcessError::Timeout { .. } => ToolRegistryError::FrameworkProcessTimeout {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            timeout_ms: timeout.as_millis(),
        },
        ProcessError::Cancelled { .. } => ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: "cancelled".to_owned(),
            message: "framework process was cancelled".to_owned(),
            detail: String::new(),
        },
        ProcessError::OutputLimit {
            stderr,
            diagnostics,
            ..
        } => ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: "resource_limit".to_owned(),
            message: "framework process exceeded bounded output limits".to_owned(),
            detail: format!(
                "stderr={} bytes; stdout={} bytes; {}",
                diagnostics.stderr_bytes,
                diagnostics.stdout_bytes,
                crate::bounded_error_text(&String::from_utf8_lossy(&stderr))
            ),
        },
        other => ToolRegistryError::FrameworkProcessIo {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: other.to_string(),
        },
    }
}

pub(super) fn framework_io_error(
    tool: &ToolDefinition,
    framework: &str,
    error: std::io::Error,
) -> ToolRegistryError {
    ToolRegistryError::FrameworkProcessIo {
        id: tool.id.clone(),
        framework: framework.to_owned(),
        reason: error.to_string(),
    }
}
