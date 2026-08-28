// Resolves core and plugin commands through one registry-backed invoke boundary.
fn invoke_capability(
    body: &str,
    run_store: &SharedRunStore,
    brain_planner: &SharedBrainPlanner,
    registry: &SharedCapabilityDispatchRegistry,
) -> Result<(u16, String)> {
    let Ok(request) = serde_json::from_str::<InvokeCapabilityRequest>(body) else {
        return bad_request("invalid invoke request");
    };
    if request.request_id.trim().is_empty() {
        return bad_request("invalid invoke request: requestId is required");
    }
    if request.caller.trim().is_empty() {
        return invoke_error(
            400,
            Some(&request.request_id),
            "invalid_request",
            "caller is required",
            json!({}),
        );
    }
    let owner = match registry.resolve(&request.capability) {
        Ok(Some(owner)) => owner,
        Ok(None) => {
            return invoke_error(
                404,
                Some(&request.request_id),
                "unknown_capability",
                &format!("unknown capability `{}`", request.capability),
                json!({
                    "capability": request.capability,
                }),
            )
        }
        Err(error) => return capability_runtime_error_response(error),
    };
    match owner {
        CapabilityCommandOwner::Core(CoreCapabilityHandler::BrainPlan) => {
            invoke_brain_plan(request, run_store, brain_planner)
        }
        CapabilityCommandOwner::Core(CoreCapabilityHandler::TeaTicketDecompose) => {
            invoke_tea_ticket_decompose(request, run_store)
        }
        CapabilityCommandOwner::Plugin(plugin_id) => {
            invoke_plugin_command(request, &plugin_id, registry.plugin_runtime())
        }
    }
}

fn invoke_plugin_command(
    request: InvokeCapabilityRequest,
    plugin_id: &str,
    runtime: &SharedCapabilityRuntime,
) -> Result<(u16, String)> {
    if request.resource_refs.len() > 128 {
        return invoke_error(
            400,
            Some(&request.request_id),
            "invalid_input",
            "resourceRefs exceeds the command limit",
            json!({ "capability": request.capability }),
        );
    }
    let timeout = request.timeout_ms.map(Duration::from_millis);
    let request_id = request.request_id.clone();
    let capability = request.capability.clone();
    let output = match runtime.invoke(CapabilityInvocation {
        request_id: request_id.clone(),
        command_id: request.capability,
        input: request.input,
        target: request.target,
        resource_refs: request.resource_refs,
        user_gesture_token: request.user_gesture_token,
        timeout,
    }) {
        Ok(output) => output,
        Err(error) => return capability_runtime_invoke_error(&request_id, &capability, error),
    };
    if output.plugin_id != plugin_id {
        return invoke_error(
            500,
            Some(&request_id),
            "capability_owner_mismatch",
            "capability runtime ownership changed during dispatch",
            json!({ "capability": capability }),
        );
    }
    let status = serde_json::to_value(output.status)?;
    Ok((
        200,
        serde_json::to_string(&json!({
            "requestId": request_id,
            "status": status,
            "output": output.payload,
            "error": output.error,
            "pluginId": output.plugin_id,
            "packageDigest": output.package_digest,
        }))?,
    ))
}

fn cancel_capability_invocation(
    body: &str,
    runtime: &SharedCapabilityRuntime,
) -> Result<(u16, String)> {
    let request = match serde_json::from_str::<CancelCapabilityInvocationRequest>(body) {
        Ok(request) if !request.request_id.trim().is_empty() => request,
        Ok(_) => return bad_request("cancel requestId is required"),
        Err(_) => return bad_request("invalid cancel request"),
    };
    match runtime.cancel_request(&request.request_id) {
        Ok(true) => Ok((
            200,
            serde_json::to_string(&json!({
                "requestId": request.request_id,
                "status": "cancelled",
            }))?,
        )),
        Ok(false) => invoke_error(
            404,
            Some(&request.request_id),
            "capability_request_not_found",
            "capability invocation is not active",
            json!({}),
        ),
        Err(error) => {
            capability_runtime_invoke_error(&request.request_id, "runtime.cancel", error)
        }
    }
}

fn capability_runtime_invoke_error(
    request_id: &str,
    capability: &str,
    error: loom_capability_runtime::CapabilityHostError,
) -> Result<(u16, String)> {
    use loom_capability_runtime::CapabilityHostError as Error;
    let (status, code, retryable) = match &error {
        Error::NotFound(_) => (404, "capability_command_not_found", false),
        Error::Busy => (503, "capability_busy", true),
        Error::Timeout => (504, "capability_timeout", true),
        Error::InvalidPackage(_) | Error::Protocol(_) => {
            (400, "capability_runtime_rejected", false)
        }
        Error::Unavailable(_) | Error::Io(_) | Error::Process(_) | Error::Json(_) => {
            (503, "capability_runtime_unavailable", true)
        }
    };
    invoke_error(
        status,
        Some(request_id),
        code,
        &error.to_string(),
        json!({
            "capability": capability,
            "retryable": retryable,
        }),
    )
}
