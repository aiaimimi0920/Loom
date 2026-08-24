// Bounded framework process conformance execution and temporary ownership.
fn run_conformance(executable: &Path, framework_id: &str, art_dir: &Path) -> Result<String> {
    if !is_safe_package_reference(framework_id) {
        bail!("framework id is not a safe package id: {framework_id}");
    }
    let _input_executable_guard = open_regular_file(executable, "conformance executable")?;
    let executable = fs::canonicalize(executable)
        .with_context(|| format!("canonicalize conformance executable {}", executable.display()))?;
    let _canonical_executable_guard = open_regular_file(&executable, "conformance executable")?;
    let art_dir = ensure_real_directory(art_dir, "conformance Art directory")?;
    let trust_store = TrustStore::default();
    validate_art_package(&art_dir, true, &trust_store, false)
        .context("validate conformance Art")?;
    let art_manifest: Value = read_json(&art_dir.join("manifest.json"))?;
    let declared_framework = art_manifest
        .pointer("/execution/framework")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("conformance Art has no framework id"))?;
    if declared_framework != framework_id {
        bail!(
            "conformance Art declares framework `{declared_framework}`, expected `{framework_id}`"
        );
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = CONFORMANCE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_dir = std::env::temp_dir().join(format!(
        "loom-plugin-conformance-{}-{nonce}-{sequence}",
        std::process::id(),
    ));
    fs::create_dir_all(&temp_dir)?;
    let _cleanup = RemoveDirOnDrop(temp_dir.clone());
    let request = FrameworkExecuteRequest {
        protocol_version: FRAMEWORK_PROTOCOL_VERSION.to_owned(),
        supported_protocol_versions: vec![FRAMEWORK_PROTOCOL_VERSION.to_owned()],
        framework_id: framework_id.to_owned(),
        art_id: "loom-conformance-art".to_owned(),
        art_dir: art_dir.clone(),
        inputs: json!({ "input": "conformance" }),
        params: json!({}),
        disabled_params: Vec::new(),
        context: FrameworkExecutionContext {
            request_id: "loom-plugin-conformance".to_owned(),
            cache_dir: temp_dir.join("cache"),
            temp_dir: temp_dir.join("temp"),
            host_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            ..FrameworkExecutionContext::default()
        },
    };
    fs::create_dir_all(&request.context.cache_dir)?;
    fs::create_dir_all(&request.context.temp_dir)?;
    let payload = serde_json::to_vec(&request)?;
    let mut process = loom_process::ProcessSpec::new(executable.clone());
    process.args = vec!["--framework-id".to_owned(), framework_id.to_owned()];
    process.current_dir = executable.parent().map(Path::to_path_buf);
    process.limits.timeout = CONFORMANCE_TIMEOUT;
    process.limits.stdout_bytes = MAX_CONFORMANCE_OUTPUT_BYTES;
    process.limits.stderr_bytes = MAX_CONFORMANCE_OUTPUT_BYTES;
    process.limits.max_processes = Some(4);
    let output = loom_process::run_with_input(&process, &payload)
        .with_context(|| format!("run {}", executable.display()))?;
    if !output.status.success() {
        bail!(
            "framework exited with {:?} (stdoutBytes={}, stderrBytes={})",
            output.status.code(),
            output.stdout.len(),
            output.stderr.len()
        );
    }
    let response: FrameworkExecuteResponse =
        serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "parse framework response (stdoutBytes={}, stderrBytes={})",
                output.stdout.len(),
                output.stderr.len()
            )
    })?;
    if !loom_protocol::response_status_is_success(&response.status.to_ascii_lowercase()) {
        bail!("framework returned a failure status");
    }
    Ok(format!(
        "conformance passed: protocol={}, framework={}, stdoutBytes={}, stderrBytes={}",
        FRAMEWORK_PROTOCOL_VERSION,
        framework_id,
        output.stdout.len(),
        output.stderr.len()
    ))
}

struct RemoveDirOnDrop(PathBuf);

impl Drop for RemoveDirOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
fn read_bounded(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    reader
        .by_ref()
        .take((MAX_CONFORMANCE_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)?;
    if output.len() > MAX_CONFORMANCE_OUTPUT_BYTES {
        bail!("process output exceeds {MAX_CONFORMANCE_OUTPUT_BYTES} bytes");
    }
    Ok(output)
}
