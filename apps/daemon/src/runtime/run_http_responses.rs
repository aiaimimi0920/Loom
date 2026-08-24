// Run status and actions, common route errors, and HTTP response serialization.
fn get_run(run_id: &str, run_store: &SharedRunStore) -> Result<(u16, String)> {
    let store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    let run = match store.get_run(run_id) {
        Ok(Some(run)) => run,
        Ok(None) => return run_not_found(run_id),
        Err(error) => return run_store_failed(error),
    };
    Ok((200, serde_json::to_string(&run)?))
}

fn get_run_events(run_id: &str, run_store: &SharedRunStore) -> Result<(u16, String)> {
    let store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    let events = match store.get_events(run_id) {
        Ok(Some(events)) => events,
        Ok(None) => return run_not_found(run_id),
        Err(error) => return run_store_failed(error),
    };
    Ok((
        200,
        serde_json::to_string(&json!({
            "run_id": run_id,
            "events": events,
        }))?,
    ))
}

fn start_tea_run(body: &str, run_store: &SharedRunStore) -> Result<(u16, String)> {
    let Ok(request) = serde_json::from_str::<StartRunRequest>(body) else {
        return bad_request("invalid run request");
    };
    let ticket = request.ticket;
    let run = json!({
        "id": loom_core::RunId::new().to_string(),
        "ticket_id": ticket.id,
        "loom_session_id": loom_core::SessionId::new().to_string(),
        "status": "succeeded",
        "evidence": {
            "summary": format!(
                "loom daemon run completed for {}",
                ticket.title
                    .as_deref()
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or("Tea ticket")
            ),
            "commands": [],
            "artifacts": [
                "loom-daemon:http-run-contract"
            ],
            "risks": ticket
                .description
                .as_deref()
                .filter(|description| !description.trim().is_empty())
                .map(|description| vec![format!("request context length: {} bytes", description.len())])
                .unwrap_or_default()
        }
    });
    let events = match (
        RunEventDraft::new(
            "run_started",
            json!({
                "source": "tea",
                "status": "running",
            }),
        ),
        RunEventDraft::new(
            "run_finished",
            json!({
                "source": "tea",
                "status": "succeeded",
            }),
        ),
    ) {
        (Ok(started), Ok(finished)) => vec![started, finished],
        (Err(error), _) | (_, Err(error)) => return run_store_failed(error),
    };
    let mut store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    if let Err(error) = store.insert_run(run.clone(), events) {
        return run_store_failed(error);
    }
    Ok((200, serde_json::to_string(&run)?))
}

fn run_action(
    path_run_id: &str,
    body: &str,
    status: &str,
    run_store: &SharedRunStore,
) -> Result<(u16, String)> {
    let Ok(request) = serde_json::from_str::<RunActionRequest>(body) else {
        return bad_request("invalid run action request");
    };
    let Some(body_run_id) = request.run.get("id").and_then(Value::as_str) else {
        return structured_error(
            400,
            json!({
                "code": "invalid_run_action_request",
                "message": "run action request requires run.id",
            }),
        );
    };
    if body_run_id != path_run_id {
        return structured_error(
            400,
            json!({
                "code": "run_id_mismatch",
                "message": format!(
                    "path run id `{path_run_id}` does not match body run id `{body_run_id}`"
                ),
                "path_run_id": path_run_id,
                "body_run_id": body_run_id,
            }),
        );
    }

    let mut store = match lock_run_store(run_store) {
        Ok(store) => store,
        Err(error) => return run_store_failed(error),
    };
    let mut run = match store.get_run(path_run_id) {
        Ok(Some(run)) => run,
        Ok(None) => return run_not_found(path_run_id),
        Err(error) => return run_store_failed(error),
    };
    run["status"] = json!(status);
    let event = match RunEventDraft::new(
        "run_action",
        json!({
            "action": status,
            "status": status,
        }),
    ) {
        Ok(event) => event,
        Err(error) => return run_store_failed(error),
    };
    if let Err(error) = store.transition_run(run.clone(), event) {
        return run_store_failed(error);
    }
    Ok((200, serde_json::to_string(&run)?))
}

fn bad_request(message: &'static str) -> Result<(u16, String)> {
    Ok((
        400,
        serde_json::to_string(&json!({
            "status": "failed",
            "error": {
                "code": "invalid_request",
                "message": message,
            }
        }))?,
    ))
}

fn structured_error(status: u16, error: Value) -> Result<(u16, String)> {
    Ok((status, serde_json::to_string(&json!({ "error": error }))?))
}

fn device_auth_error_response(error: DeviceAuthError) -> Result<(u16, String)> {
    structured_error(
        error.status,
        json!({
            "code": error.code,
            "message": error.message,
        }),
    )
}

fn request_worker_failed_response() -> (u16, String) {
    structured_error(
        500,
        json!({
            "code": "request_worker_failed",
            "message": "Loom could not complete the request"
        }),
    )
    .expect("serialize request worker failure response")
}

fn daemon_busy_response() -> (u16, String) {
    structured_error(
        503,
        json!({
            "code": "daemon_busy",
            "message": "Loom daemon request queue is full",
            "retryable": true,
        }),
    )
    .expect("serialize daemon busy response")
}

fn daemon_shutting_down_response() -> (u16, String) {
    structured_error(
        503,
        json!({
            "code": "daemon_shutting_down",
            "message": "Loom daemon is shutting down",
            "retryable": true,
        }),
    )
    .expect("serialize daemon shutdown response")
}

fn invoke_error(
    status: u16,
    request_id: Option<&str>,
    code: &str,
    message: &str,
    fields: Value,
) -> Result<(u16, String)> {
    let mut error = json!({
        "code": code,
        "message": message,
    });
    merge_object_fields(&mut error, fields);
    Ok((
        status,
        serde_json::to_string(&json!({
            "requestId": request_id.unwrap_or_default(),
            "status": "failed",
            "error": error,
        }))?,
    ))
}

fn truncate_diagnostic(diagnostic: String, max_bytes: usize) -> String {
    if diagnostic.len() <= max_bytes {
        return diagnostic;
    }
    if max_bytes <= 3 {
        return diagnostic.chars().take(max_bytes).collect::<String>();
    }
    let mut end = max_bytes - 3;
    while end > 0 && !diagnostic.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &diagnostic[..end])
}

fn run_not_found(run_id: &str) -> Result<(u16, String)> {
    structured_error(
        404,
        json!({
            "code": "run_not_found",
            "message": format!("run `{run_id}` was not found"),
            "run_id": run_id,
        }),
    )
}

fn run_store_failed(error: RunStoreError) -> Result<(u16, String)> {
    eprintln!("loom run store operation failed: {error}");
    structured_error(
        500,
        json!({
            "code": "run_store_failed",
            "message": "Loom run evidence could not be stored"
        }),
    )
}

fn lock_run_store(
    run_store: &SharedRunStore,
) -> std::result::Result<std::sync::MutexGuard<'_, Box<dyn RunEvidenceStore>>, RunStoreError> {
    run_store
        .lock()
        .map_err(|_| RunStoreError::Integrity("run store lock poisoned".to_owned()))
}

fn merge_object_fields(target: &mut Value, fields: Value) {
    let (Some(target), Some(fields)) = (target.as_object_mut(), fields.as_object()) else {
        return;
    };
    for (key, value) in fields {
        target.insert(key.clone(), value.clone());
    }
}

fn run_path_id(path: &str) -> Option<&str> {
    let run_id = path.strip_prefix("/v1/runs/")?;
    (!run_id.is_empty() && !run_id.contains('/')).then_some(run_id)
}

fn execution_diagnostics_path_id(path: &str) -> Option<&str> {
    let run_id = path.strip_prefix("/v1/diagnostics/executions/")?;
    (!run_id.is_empty() && !run_id.contains('/')).then_some(run_id)
}

fn run_events_path_id(path: &str) -> Option<&str> {
    let run_id = path.strip_prefix("/v1/runs/")?.strip_suffix("/events")?;
    (!run_id.is_empty() && !run_id.contains('/')).then_some(run_id)
}

fn run_action_path_id<'a>(path: &'a str, action: &str) -> Option<&'a str> {
    let suffix = match action {
        "stop" => "/stop",
        "retry" => "/retry",
        _ => return None,
    };
    let run_id = path.strip_prefix("/v1/runs/")?.strip_suffix(suffix)?;
    (!run_id.is_empty() && !run_id.contains('/')).then_some(run_id)
}

fn module_statuses() -> Vec<ModuleStatus> {
    vec![
        ModuleStatus {
            name: "core",
            version: loom_core::LOOM_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "durable",
            version: loom_durable::LOOM_DURABLE_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "agent",
            version: loom_agent::LOOM_AGENT_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "workflow",
            version: loom_workflow::LOOM_WORKFLOW_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "memory",
            version: loom_memory::LOOM_MEMORY_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "sandbox",
            version: loom_sandbox::LOOM_SANDBOX_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "gateway",
            version: loom_gateway::LOOM_GATEWAY_VERSION,
            initialized: true,
        },
        ModuleStatus {
            name: "hooks",
            version: loom_hooks::LOOM_HOOKS_VERSION,
            initialized: true,
        },
    ]
}

fn write_response(stream: &mut impl Write, status: u16, body: &str) -> Result<()> {
    write_response_with_headers(stream, status, body, &[])
}

fn write_response_with_headers(
    stream: &mut impl Write,
    status: u16,
    body: &str,
    headers: &[(String, String)],
) -> Result<()> {
    let reason = response_reason(status);
    let content_type = if body.trim_start().starts_with("<!doctype html") {
        "text/html; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\n"
    )
    .context("write daemon response head")?;
    for (name, value) in headers {
        if name
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b':')
            || value.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
        {
            anyhow::bail!("refuse invalid daemon response header");
        }
        write!(stream, "{name}: {value}\r\n").context("write daemon response header")?;
    }
    write!(
        stream,
        "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .context("write daemon response body")
}

fn write_binary_response(
    stream: &mut impl Write,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let reason = response_reason(status);
    // Preview URLs carry a content version token, so a changed image always
    // arrives under a new URL. Force revalidation anyway so an in-place image
    // update is never masked by an aggressive WebView/browser cache.
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .context("write daemon binary response")?;
    stream.write_all(body).context("write daemon binary body")
}

fn response_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        303 => "See Other",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}
