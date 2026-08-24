// Python Art discovery, identifier analysis, framework mapping, and external runs.
fn resolve_art_json_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("art.json")
    } else {
        path.to_path_buf()
    }
}

fn display_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(stripped) = raw.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{stripped}");
    }
    raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_owned()
}

fn infer_python_ports_from_code(code: &str) -> (Vec<Value>, Vec<Value>) {
    let mut inputs = Vec::<String>::new();
    collect_python_arg_names(code, "args.get(", &mut inputs);
    collect_python_arg_names(code, "args[", &mut inputs);

    let outputs = collect_python_return_object_keys(code);

    (
        inputs
            .into_iter()
            .map(|name| python_port_json(&name))
            .collect(),
        outputs
            .into_iter()
            .map(|name| python_port_json(&name))
            .collect(),
    )
}

fn collect_python_arg_names(code: &str, marker: &str, names: &mut Vec<String>) {
    let mut rest = code;
    while let Some(index) = rest.find(marker) {
        let after_marker = rest[index + marker.len()..].trim_start();
        let Some(quote) = after_marker
            .chars()
            .next()
            .filter(|quote| *quote == '"' || *quote == '\'')
        else {
            rest = &rest[index + marker.len()..];
            continue;
        };
        let after_quote = &after_marker[quote.len_utf8()..];
        if let Some(end_index) = after_quote.find(quote) {
            let name = &after_quote[..end_index];
            if is_python_identifier_like(name) && !names.iter().any(|existing| existing == name) {
                names.push(name.to_owned());
            }
            rest = &after_quote[end_index + quote.len_utf8()..];
        } else {
            break;
        }
    }
}

fn collect_python_return_object_keys(code: &str) -> Vec<String> {
    let Some(return_index) = code.find("return") else {
        return Vec::new();
    };
    let after_return = &code[return_index..];
    let Some(open_index) = after_return.find('{') else {
        return Vec::new();
    };
    let after_open = &after_return[open_index + 1..];
    let Some(close_index) = after_open.find('}') else {
        return Vec::new();
    };
    let object_body = &after_open[..close_index];
    let mut names = Vec::<String>::new();
    let mut rest = object_body;
    while let Some(quote_index) = rest.find(['"', '\'']) {
        let quote = rest[quote_index..]
            .chars()
            .next()
            .expect("quote char after find");
        let after_quote = &rest[quote_index + quote.len_utf8()..];
        let Some(end_index) = after_quote.find(quote) else {
            break;
        };
        let name = &after_quote[..end_index];
        let after_name = after_quote[end_index + quote.len_utf8()..].trim_start();
        if after_name.starts_with(':')
            && is_python_identifier_like(name)
            && !names.iter().any(|existing| existing == name)
        {
            names.push(name.to_owned());
        }
        rest = after_name;
    }
    names
}

fn is_python_identifier_like(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn python_port_json(name: &str) -> Value {
    let (ui_type, execution_type) = infer_python_port_type(name);
    json!({
        "name": name,
        "label": name,
        "type": ui_type,
        "execution_type": execution_type,
        "executionType": execution_type,
    })
}

fn infer_python_port_type(name: &str) -> (&'static str, &'static str) {
    let lower = name.to_ascii_lowercase();
    if [
        "path",
        "image",
        "file",
        "input",
        "output",
        "source",
        "reference",
        "result",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return ("image", "image_path");
    }
    if ["factor", "ratio", "strength", "alpha", "blend", "scale"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return ("float", "number");
    }
    if ["count", "num", "size", "clusters", "width", "height", "n_"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        return ("int", "number");
    }
    ("string", "string")
}

fn collect_python_arts() -> Vec<Value> {
    let mut seen = HashMap::<String, ()>::new();
    let mut arts = Vec::new();
    for arts_dir in python_arts_dirs() {
        let Ok(entries) = fs::read_dir(&arts_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let art_json_path = path.join("art.json");
            if !art_json_path.is_file() {
                continue;
            }
            let Ok(content) = fs::read_to_string(&art_json_path) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<Value>(&content) else {
                continue;
            };
            let art_id = json
                .get("art_id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            if seen.contains_key(&art_id) {
                continue;
            }
            seen.insert(art_id.clone(), ());
            let label = json
                .get("label")
                .and_then(Value::as_str)
                .or_else(|| json.get("name").and_then(Value::as_str))
                .unwrap_or_else(|| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Python Art")
                });
            let description = json
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let version = json
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("1.0.0");
            arts.push(json!({
                "path": path.to_string_lossy(),
                "art_json_path": art_json_path.to_string_lossy(),
                "art_id": art_id,
                "label": label,
                "description": description,
                "version": version,
                "definition": json,
            }));
        }
    }
    arts.sort_by(|left, right| {
        let left_label = left
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right_label = right
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or_default();
        left_label.cmp(right_label)
    });
    arts
}

fn python_arts_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(configured_dir) = std::env::var("LOOM_PYTHON_ARTS_DIR") {
        let configured_dir = configured_dir.trim();
        if !configured_dir.is_empty() {
            dirs.push(PathBuf::from(configured_dir));
        }
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir.join("python").join("Arts"));
    }
    #[cfg(debug_assertions)]
    {
        if let Ok(current_dir) = std::env::current_dir() {
            dirs.push(current_dir.join("python").join("Arts"));
            dirs.push(
                current_dir
                    .join("Loom")
                    .join("resources")
                    .join("python")
                    .join("Arts"),
            );
        }
        for ancestor in PathBuf::from(env!("CARGO_MANIFEST_DIR")).ancestors() {
            dirs.push(ancestor.join("resources").join("python").join("Arts"));
        }
    }
    dirs
}

fn framework_id_for_tool(tool: &ToolDefinition) -> String {
    match &tool.execution {
        ToolExecution::FrameworkArt { framework } => framework.clone(),
        execution => {
            loom_tool_registry::framework::framework_id_for_execution(execution).to_owned()
        }
    }
}

fn external_tool_run(run_id: &str, tool: &ToolDefinition, arguments: &Value) -> Value {
    let package = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"));
    let argument_keys = arguments
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    json!({
        "id": run_id,
        "capability": "art.execute",
        "status": "running",
        "toolId": &tool.id,
        "qualifiedId": tool.qualified_id(),
        "frameworkId": framework_id_for_tool(tool),
        "package": package.map(|package| json!({
            "version": package.get("version").cloned(),
            "digest": package.get("digest").cloned(),
            "trustStatus": package.get("trustStatus").cloned(),
        })),
        "inputSummary": {
            "keys": argument_keys,
            "jsonBytes": serde_json::to_vec(arguments).map(|bytes| bytes.len()).unwrap_or_default(),
        },
    })
}
