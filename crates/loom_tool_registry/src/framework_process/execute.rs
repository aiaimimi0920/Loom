use super::*;

/// Execute a pluginized Art through its installed framework package.
pub fn execute_framework_art(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
) -> ToolRegistryResult<Value> {
    let packages_root =
        framework_packages_root().ok_or_else(|| ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: "LOOM_CONTROL_PLANE_ROOT/frameworks".to_owned(),
        })?;
    execute_framework_art_in_root_with_timeout(
        tool,
        framework,
        arguments,
        &packages_root,
        DEFAULT_FRAMEWORK_PROCESS_TIMEOUT,
        None,
    )
}

/// Execute a framework Art with a caller-owned upper timeout bound.
///
/// Surface actions use this entry point so their declared deadline also bounds
/// the managed process tree instead of merely timing out the caller.
pub fn execute_framework_art_with_timeout(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
    timeout: Duration,
) -> ToolRegistryResult<Value> {
    let packages_root =
        framework_packages_root().ok_or_else(|| ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: "LOOM_CONTROL_PLANE_ROOT/frameworks".to_owned(),
        })?;
    execute_framework_art_in_root_with_timeout(
        tool,
        framework,
        arguments,
        &packages_root,
        timeout.min(DEFAULT_FRAMEWORK_PROCESS_TIMEOUT),
        None,
    )
}

/// Execute a framework Art with timeout and caller-owned cancellation.
///
/// Cancellation is propagated to `loom_process`, which terminates the managed
/// process tree before returning.
pub fn execute_framework_art_with_timeout_and_cancellation(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> ToolRegistryResult<Value> {
    let packages_root =
        framework_packages_root().ok_or_else(|| ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: "LOOM_CONTROL_PLANE_ROOT/frameworks".to_owned(),
        })?;
    execute_framework_art_in_root_with_timeout(
        tool,
        framework,
        arguments,
        &packages_root,
        timeout.min(DEFAULT_FRAMEWORK_PROCESS_TIMEOUT),
        Some(cancellation),
    )
}

pub(super) fn execute_framework_art_in_root_with_timeout(
    tool: &ToolDefinition,
    framework: &str,
    arguments: Value,
    packages_root: &Path,
    timeout: Duration,
    cancellation: Option<&AtomicBool>,
) -> ToolRegistryResult<Value> {
    if !crate::framework::is_valid_framework_reference(framework) {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "framework id is not a safe package id".to_owned(),
        });
    }

    let package_dir = resolve_framework_package_dir(packages_root, framework).map_err(|error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            // The resolver distinguishes "no package installed" from "several publishers ship this
            // local id"; carrying its message keeps that distinction visible to the operator.
            path: format!("{} ({error})", packages_root.display()),
        }
    })?;
    let manifest_path = package_dir.join("framework.manifest.json");
    let canonical_packages_root = fs::canonicalize(packages_root).map_err(|_error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: packages_root.display().to_string(),
        }
    })?;
    let package_dir = fs::canonicalize(&package_dir).map_err(|_error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: manifest_path.display().to_string(),
        }
    })?;
    if !package_dir.starts_with(&canonical_packages_root) {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "framework package resolves outside the package root".to_owned(),
        });
    }
    let manifest_path = package_dir.join("framework.manifest.json");
    let manifest_bytes = read_bounded_framework_metadata(&manifest_path).map_err(|_error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: manifest_path.display().to_string(),
        }
    })?;
    let manifest_text = String::from_utf8(manifest_bytes).map_err(|error| {
        ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!(
                "invalid framework.manifest.json UTF-8: {}",
                error.utf8_error()
            ),
        }
    })?;
    let manifest: FrameworkPackageManifest =
        serde_json::from_str(&manifest_text).map_err(|error| {
            ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: format!("invalid framework.manifest.json: {error}"),
            }
        })?;
    let negotiated_protocol =
        loom_protocol::negotiate_framework_protocol(&manifest).map_err(|error| {
            ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: error.to_string(),
            }
        })?;
    if manifest.id != framework && manifest.qualified_id() != framework {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!("manifest identity mismatch: id={}", manifest.id),
        });
    }
    if manifest.entry.kind != "process" || manifest.entry.command.trim().is_empty() {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "manifest entry must be a process with a command".to_owned(),
        });
    }
    enforce_framework_permission_policy(&manifest).map_err(|reason| {
        ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!("permission enforcement unavailable: {reason}"),
        }
    })?;
    let command_path = Path::new(&manifest.entry.command);
    if command_path.is_absolute()
        || command_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "manifest entry command must be relative to the package".to_owned(),
        });
    }
    let unresolved_command_path = package_dir.join(command_path);
    let command_path = fs::canonicalize(&unresolved_command_path).map_err(|_error| {
        ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: unresolved_command_path.display().to_string(),
        }
    })?;
    if !command_path.starts_with(&package_dir) {
        return Err(ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: "manifest entry command resolves outside the framework package".to_owned(),
        });
    }
    if !command_path.is_file() {
        return Err(ToolRegistryError::FrameworkPackageNotFound {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            path: command_path.display().to_string(),
        });
    }

    let art_dir =
        art_directory(tool).ok_or_else(|| ToolRegistryError::FrameworkArtDirectoryNotFound {
            id: tool.id.clone(),
            path: "<metadata.artPackage.dir>".to_owned(),
        })?;
    if !art_dir.is_dir() {
        return Err(ToolRegistryError::FrameworkArtDirectoryNotFound {
            id: tool.id.clone(),
            path: art_dir.display().to_string(),
        });
    }

    let request_id = request_id();
    let cache_dir =
        art_package_path(tool, "cacheDir").unwrap_or_else(|| art_dir.join(".loom-cache"));
    let state_dir = art_package_path(tool, "stateDir");
    let output_dir = art_package_path(tool, "outputDir");
    let temp_root = std::env::temp_dir().join("loom-framework");
    let temp_dir = temp_root.join(&request_id);
    fs::create_dir_all(&cache_dir).map_err(|error| framework_io_error(tool, framework, error))?;
    fs::create_dir_all(&temp_root).map_err(|error| framework_io_error(tool, framework, error))?;
    let temp_dir = TempDirectoryGuard::create(temp_dir)
        .map_err(|error| framework_io_error(tool, framework, error))?;

    let (inputs, params, disabled_params) = split_arguments(tool, &arguments);
    let credential_store = packages_root
        .parent()
        .map(crate::credentials::CredentialStore::new);
    let art_identity = tool.qualified_id();
    let (mut credentials, mcp_server) = if framework == "mcp" {
        let control_plane_root =
            packages_root
                .parent()
                .ok_or_else(|| ToolRegistryError::FrameworkProcessProtocol {
                    id: tool.id.clone(),
                    framework: framework.to_owned(),
                    reason: "framework package root has no control-plane parent".to_owned(),
                })?;
        let (server, grants) =
            resolve_mcp_server(tool, control_plane_root, credential_store.as_ref())?;
        (grants, Some(server))
    } else {
        let grants = credential_store
            .as_ref()
            .map(|store| {
                store.grants_for(
                    framework,
                    &art_identity,
                    &manifest.permission_policy.credentials,
                )
            })
            .transpose()
            .map_err(|error| ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: format!("credential broker failed: {error}"),
            })?
            .unwrap_or_default();
        (grants, None)
    };
    if framework != "mcp" {
        if let (Some(store), bindings) = (credential_store.as_ref(), art_credential_bindings(tool))
        {
            if !bindings.is_empty() {
                let bound = store
                    .grants_for_bindings(framework, &art_identity, &bindings)
                    .map_err(|error| ToolRegistryError::FrameworkProcessProtocol {
                        id: tool.id.clone(),
                        framework: framework.to_owned(),
                        reason: format!("credential binding failed: {error}"),
                    })?;
                for grant in bound {
                    credentials.retain(|existing| existing.name != grant.name);
                    credentials.push(grant);
                }
            }
        }
    }
    let request = FrameworkExecuteRequest {
        protocol_version: negotiated_protocol.to_owned(),
        supported_protocol_versions: vec![FRAMEWORK_PROTOCOL_VERSION.to_owned()],
        framework_id: manifest.id.clone(),
        art_id: tool.id.clone(),
        art_dir: art_dir.clone(),
        inputs,
        params,
        disabled_params,
        context: FrameworkExecutionContext {
            request_id,
            cache_dir: cache_dir.clone(),
            temp_dir: temp_dir.path().to_path_buf(),
            state_dir,
            output_dir: output_dir.clone(),
            host_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            framework_version: Some(manifest.version.clone()),
            art_version: art_package_string(tool, "version"),
            granted_permissions: manifest.permission_policy.clone(),
            credentials,
            mcp_server,
            ..FrameworkExecutionContext::default()
        },
    };
    let payload = serde_json::to_vec(&request).map_err(|error| {
        ToolRegistryError::FrameworkProcessProtocol {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason: format!("cannot serialize request: {error}"),
        }
    })?;

    let mut process = ProcessSpec::new(&command_path);
    process.args = manifest.entry.args.clone();
    let persistent_mcp_host = manifest.id == "mcp";
    if persistent_mcp_host {
        process.args.push("--serve".to_owned());
    }
    process.current_dir = Some(package_dir.clone());
    process.limits.timeout = manifest
        .resources
        .timeout_seconds
        .map(Duration::from_secs)
        .map(|declared| declared.min(timeout))
        .unwrap_or(timeout);
    process.limits.stdout_bytes = manifest
        .resources
        .stdout_mib
        .and_then(|value| usize::try_from(value.saturating_mul(1024 * 1024)).ok())
        .unwrap_or(process.limits.stdout_bytes);
    process.limits.stderr_bytes = manifest
        .resources
        .stderr_mib
        .and_then(|value| usize::try_from(value.saturating_mul(1024 * 1024)).ok())
        .unwrap_or(process.limits.stderr_bytes);
    process.limits.memory_bytes = manifest
        .resources
        .memory_mib
        .and_then(|value| usize::try_from(value.saturating_mul(1024 * 1024)).ok())
        .or(process.limits.memory_bytes);
    process.limits.max_processes = manifest
        .resources
        .max_processes
        .or(process.limits.max_processes);
    let mut stdin_payload = payload;
    stdin_payload.push(b'\n');
    let mut persistent_host = None;
    let (exit_status, stdout, stderr, process_diagnostics) = if persistent_mcp_host {
        let key = persistent_host_key(&command_path, &manifest_text, &process.args);
        let (stdout, host) = request_persistent_mcp_host(
            key,
            &process,
            &stdin_payload,
            cancellation,
            tool,
            framework,
        )
        .map_err(|error| redact_framework_error(error, &request.context.credentials))?;
        persistent_host = Some(host);
        (None, stdout, String::new(), None)
    } else {
        let process_output = match cancellation {
            Some(cancellation) => {
                loom_process::run_with_input_cancellable(&process, &stdin_payload, cancellation)
            }
            None => loom_process::run_with_input(&process, &stdin_payload),
        }
        .map_err(|error| {
            redact_framework_error(
                map_process_error(tool, framework, process.limits.timeout, error),
                &request.context.credentials,
            )
        })?;
        (
            Some(process_output.status),
            process_output.stdout,
            String::from_utf8_lossy(&process_output.stderr).into_owned(),
            Some(process_output.diagnostics),
        )
    };
    let stdout_text = String::from_utf8_lossy(&stdout).trim().to_owned();
    if exit_status.as_ref().is_some_and(|status| !status.success()) {
        return Err(ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: exit_status
                .as_ref()
                .and_then(|status| status.code())
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_owned()),
            message: "framework process exited unsuccessfully".to_owned(),
            detail: redact_framework_text(
                crate::bounded_error_text(&stderr),
                &request.context.credentials,
            ),
        });
    }
    let mut response: FrameworkExecuteResponse =
        serde_json::from_str(&stdout_text).map_err(|error| {
            ToolRegistryError::FrameworkProcessProtocol {
                id: tool.id.clone(),
                framework: framework.to_owned(),
                reason: redact_framework_text(
                    format!(
                        "invalid JSON response: {error}; stdout: {}",
                        crate::bounded_error_text(&stdout_text)
                    ),
                    &request.context.credentials,
                ),
            }
        })?;
    if let Some(host) = persistent_host.take() {
        return_persistent_host(host);
    }
    if let Some(process_diagnostics) = process_diagnostics {
        response.diagnostics.get_or_insert(process_diagnostics);
    }
    let status = response.status.trim().to_ascii_lowercase();
    if !loom_protocol::response_status_is_success(&status) {
        let error = response.error.unwrap_or(FrameworkExecuteError {
            code: "framework_failed".to_owned(),
            message: "framework returned a failure status".to_owned(),
            detail: None,
        });
        return Err(ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: redact_framework_text(error.code, &request.context.credentials),
            message: redact_framework_text(error.message, &request.context.credentials),
            detail: redact_framework_text(
                error.detail.unwrap_or_default(),
                &request.context.credentials,
            ),
        });
    }

    normalize_framework_image_output(
        tool,
        framework,
        &mut response.output,
        &[
            temp_dir.path(),
            cache_dir.as_path(),
            output_dir.as_deref().unwrap_or(temp_dir.path()),
        ],
    )?;

    Ok(response_to_tool_value(tool, response))
}
