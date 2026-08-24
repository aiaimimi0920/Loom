// Python runtime inspection, Art inference, dependency collection, and external tool helpers.
fn python_engine_status(framework_registry: &FrameworkRegistry) -> Result<(u16, String)> {
    let status = framework_registry
        .statuses()
        .into_iter()
        .find(|status| status.id == "process");
    let runtime_dir = status
        .as_ref()
        .and_then(|status| status.runtime_dir.clone());
    let python = runtime_dir
        .as_ref()
        .map(|directory| directory.join("python-embed").join("python.exe"));
    let available = status.as_ref().is_some_and(|status| status.ready)
        && python.as_ref().is_some_and(|path| path.is_file());
    Ok((
        200,
        serde_json::to_string(&json!({
            "available": available,
            "frameworkId": "process",
            "pythonExe": python.as_ref().map(|path| display_path(path)).unwrap_or_default(),
            "launcherAvailable": false,
            "artsDirs": python_arts_dirs()
                .iter()
                .map(|path| display_path(path))
                .collect::<Vec<_>>(),
            "installedArtCount": collect_python_arts().len(),
        }))?,
    ))
}

fn read_python_art_source(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonSourceReadRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let path = request.path.trim();
    if path.is_empty() {
        return invalid_request("path is required");
    }
    let (path, content) = match read_python_source_file(Path::new(path)) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "path": display_path(&path),
            "content": content,
            "bytes": content.len(),
        }))?,
    ))
}

fn read_python_art_json(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonArtJsonReadRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let art_path = request.art_path.trim();
    if art_path.is_empty() {
        return invalid_request("artPath is required");
    }
    let art_json_path = resolve_art_json_path(Path::new(art_path));
    let (path, art_json) = match read_art_json_file(&art_json_path) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "artJsonPath": display_path(&path),
            "artJson": art_json,
        }))?,
    ))
}

fn check_python_art_json_nearby(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonNearbyArtJsonRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let python_path = request.python_path.trim();
    if python_path.is_empty() {
        return invalid_request("pythonPath is required");
    }
    let (python_path, _) = match read_python_source_file(Path::new(python_path)) {
        Ok(result) => result,
        Err(response) => return response,
    };
    let Some(parent) = python_path.parent() else {
        return invalid_request("pythonPath must have a parent directory");
    };
    let art_json_path = parent.join("art.json");
    if !art_json_path.is_file() {
        return Ok((
            200,
            serde_json::to_string(&json!({
                "found": false,
                "pythonPath": display_path(&python_path),
            }))?,
        ));
    }
    let (path, art_json) = match read_art_json_file(&art_json_path) {
        Ok(result) => result,
        Err(response) => return response,
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "found": true,
            "pythonPath": display_path(&python_path),
            "artJsonPath": display_path(&path),
            "artJson": art_json,
        }))?,
    ))
}

fn infer_python_art_ports(body: &str) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<PythonInferPortsRequest>(body) {
        Ok(request) => request,
        Err(error) => return invalid_request(error.to_string()),
    };
    let (source_path, code) = if request.code.trim().is_empty() {
        let path = request.path.trim();
        if path.is_empty() {
            return invalid_request("code or path is required");
        }
        let (path, code) = match read_python_source_file(Path::new(path)) {
            Ok(result) => result,
            Err(response) => return response,
        };
        (Some(path), code)
    } else {
        if request.code.len() as u64 > MAX_PYTHON_SOURCE_BYTES {
            return structured_error(
                413,
                json!({
                    "code": "python_source_too_large",
                    "message": format!("Python source exceeds {MAX_PYTHON_SOURCE_BYTES} bytes"),
                }),
            );
        }
        (None, request.code)
    };
    let (inputs, outputs) = infer_python_ports_from_code(&code);
    Ok((
        200,
        serde_json::to_string(&json!({
            "path": source_path.map(|path| display_path(&path)),
            "inputs": inputs,
            "outputs": outputs,
        }))?,
    ))
}

fn read_python_source_file(
    path: &Path,
) -> std::result::Result<(PathBuf, String), Result<(u16, String)>> {
    let path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) => {
            return Err(structured_error(
                404,
                json!({
                    "code": "python_source_not_found",
                    "message": format!("Python source file was not found: {error}"),
                }),
            ));
        }
    };
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("py"))
    {
        return Err(invalid_request("path must point to a .py file"));
    }
    read_text_file_limited(&path, MAX_PYTHON_SOURCE_BYTES, "python_source_too_large")
}

fn read_art_json_file(path: &Path) -> std::result::Result<(PathBuf, Value), Result<(u16, String)>> {
    let path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) => {
            return Err(structured_error(
                404,
                json!({
                    "code": "art_json_not_found",
                    "message": format!("art.json was not found: {error}"),
                }),
            ));
        }
    };
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| !name.eq_ignore_ascii_case("art.json"))
    {
        return Err(invalid_request(
            "artPath must point to an art.json file or an Art directory",
        ));
    }
    let (path, content) = read_text_file_limited(&path, MAX_ART_JSON_BYTES, "art_json_too_large")?;
    let art_json = match serde_json::from_str::<Value>(&content) {
        Ok(json) => json,
        Err(error) => {
            return Err(invalid_request(format!(
                "failed to parse art.json: {error}"
            )));
        }
    };
    Ok((path, art_json))
}

fn read_text_file_limited(
    path: &Path,
    max_bytes: u64,
    too_large_code: &str,
) -> std::result::Result<(PathBuf, String), Result<(u16, String)>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(structured_error(
                404,
                json!({
                    "code": "file_not_found",
                    "message": format!("file was not found: {error}"),
                }),
            ));
        }
    };
    if !metadata.is_file() {
        return Err(invalid_request("path must point to a file"));
    }
    if metadata.len() > max_bytes {
        return Err(structured_error(
            413,
            json!({
                "code": too_large_code,
                "message": format!("file exceeds {max_bytes} bytes"),
                "bytes": metadata.len(),
            }),
        ));
    }
    match fs::read_to_string(path) {
        Ok(content) => Ok((path.to_path_buf(), content)),
        Err(error) => Err(structured_error(
            400,
            json!({
                "code": "file_read_failed",
                "message": format!("failed to read UTF-8 text file: {error}"),
            }),
        )),
    }
}
