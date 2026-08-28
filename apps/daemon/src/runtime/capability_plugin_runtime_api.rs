// Runtime coordination and compensation for Capability Plugin lifecycle mutations.
fn required_capability_record(
    registry: &loom_tool_registry::capability::CapabilityPluginRegistry,
    qualified_id: &str,
) -> std::result::Result<
    loom_tool_registry::capability::CapabilityPluginRecord,
    loom_tool_registry::capability::CapabilityInstallError,
> {
    registry
        .get(qualified_id)?
        .ok_or_else(|| loom_tool_registry::capability::CapabilityInstallError::NotFound(
            qualified_id.to_owned(),
        ))
}

fn activate_committed_record(
    registry: &loom_tool_registry::capability::CapabilityPluginRegistry,
    runtime: &SharedCapabilityRuntime,
    previous: loom_tool_registry::capability::CapabilityPluginRecord,
    updated: loom_tool_registry::capability::CapabilityPluginRecord,
) -> Result<(u16, String)> {
    let Some(digest) = updated.active_digest.as_deref() else {
        compensate_capability_activation(registry, previous, &updated);
        return capability_bad_request(
            "invalid_capability_state",
            "active capability record has no package digest".to_owned(),
        );
    };
    let package = match runtime_package(registry, &updated.qualified_id, digest) {
        Ok(package) => package,
        Err(error) => {
            compensate_capability_activation(registry, previous, &updated);
            return capability_error_response(error);
        }
    };
    if let Err(error) = runtime.activate(package) {
        compensate_capability_activation(registry, previous, &updated);
        return capability_runtime_error_response(error);
    }
    Ok((
        200,
        serde_json::to_string(&json!({ "plugin": updated }))?,
    ))
}

fn compensate_capability_activation(
    registry: &loom_tool_registry::capability::CapabilityPluginRegistry,
    previous: loom_tool_registry::capability::CapabilityPluginRecord,
    updated: &loom_tool_registry::capability::CapabilityPluginRecord,
) {
    if let Err(error) = registry.compensate_runtime_failure(updated.active_digest.as_deref(), previous)
    {
        runtime_log_error(format!(
            "Capability Plugin activation compensation failed for {}: {error}",
            updated.qualified_id
        ));
        let _ = registry.mark_faulted(&updated.qualified_id);
    }
}

fn restore_runtime_record(
    registry: &loom_tool_registry::capability::CapabilityPluginRegistry,
    runtime: &SharedCapabilityRuntime,
    record: &loom_tool_registry::capability::CapabilityPluginRecord,
) {
    if !record.enabled_intent {
        return;
    }
    let Some(digest) = record.active_digest.as_deref() else {
        return;
    };
    let restored = runtime_package(registry, &record.qualified_id, digest)
        .and_then(|package| runtime.activate(package).map_err(|error| {
            loom_tool_registry::capability::CapabilityInstallError::InvalidState(error.to_string())
        }));
    if let Err(error) = restored {
        runtime_log_error(format!(
            "Capability Plugin runtime restoration failed for {}: {error}",
            record.qualified_id
        ));
        let _ = registry.mark_faulted(&record.qualified_id);
    }
}

fn capability_runtime_error_response(
    error: loom_capability_runtime::CapabilityHostError,
) -> Result<(u16, String)> {
    use loom_capability_runtime::CapabilityHostError as Error;
    let (status, code, retryable) = match error {
        Error::NotFound(_) => (404, "capability_command_not_found", false),
        Error::Busy => (503, "capability_busy", true),
        Error::Timeout => (504, "capability_timeout", true),
        Error::InvalidPackage(_) | Error::Protocol(_) => (400, "capability_runtime_rejected", false),
        Error::Unavailable(_) | Error::Io(_) | Error::Process(_) | Error::Json(_) => {
            (503, "capability_runtime_unavailable", true)
        }
    };
    structured_error(
        status,
        json!({
            "code": code,
            "message": "Capability Plugin runtime operation failed",
            "retryable": retryable,
        }),
    )
}
