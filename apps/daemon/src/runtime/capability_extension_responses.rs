// Extension bridge response mapping and bounded public failure diagnostics.
fn extension_runtime_status_failure(
    request_id: &str,
    output: &loom_capability_runtime::CapabilityInvocationOutput,
) -> Option<ExtensionBridgeTextResult> {
    use loom_protocol::CapabilityRuntimeStatus;
    if output.status == CapabilityRuntimeStatus::Succeeded {
        return None;
    }
    // Ok means the runtime transport completed, not that the command succeeded.
    // Never publish effects or arbitrary runtime error text from a failed command.
    let code = if output.status == CapabilityRuntimeStatus::Cancelled {
        CapabilityErrorCode::Cancelled
    } else {
        output.error.as_ref().map_or(CapabilityErrorCode::RuntimeFault, |error| error.code)
    };
    Some(extension_result_failure(request_id, code, "capability runtime command did not succeed"))
}

fn extension_upload_failure(
    request_id: &str,
    error: ExtensionResourceUploadError,
) -> ExtensionBridgeTextResult {
    let (code, message) = match error {
        ExtensionResourceUploadError::Invalid => (
            CapabilityErrorCode::InvalidInput,
            "extension resource upload is invalid",
        ),
        ExtensionResourceUploadError::PermissionDenied => (
            CapabilityErrorCode::PermissionDenied,
            "extension image upload requires hook.unit.image.read",
        ),
        ExtensionResourceUploadError::Busy => (
            CapabilityErrorCode::Busy,
            "extension resource upload store is busy",
        ),
        ExtensionResourceUploadError::Store => (
            CapabilityErrorCode::RuntimeFault,
            "extension resource upload store is unavailable",
        ),
    };
    extension_result_failure(request_id, code, message)
}

fn extension_resource_failure(
    request_id: &str,
    error: CapabilityResourceError,
) -> ExtensionBridgeTextResult {
    let (code, message) = match error {
        CapabilityResourceError::Invalid => (
            CapabilityErrorCode::InvalidInput,
            "extension resource reference is invalid",
        ),
        CapabilityResourceError::LeaseRejected => (
            CapabilityErrorCode::ResourceNotFound,
            "extension resource lease was rejected",
        ),
        CapabilityResourceError::Busy => (
            CapabilityErrorCode::Busy,
            "extension resource broker is busy",
        ),
        CapabilityResourceError::Io(_) | CapabilityResourceError::Json(_) => (
            CapabilityErrorCode::RuntimeFault,
            "extension resource broker is unavailable",
        ),
    };
    extension_result_failure(request_id, code, message)
}

fn extension_feature_failure(request_id: &str, feature: &str) -> ExtensionBridgeTextResult {
    extension_bridge_failure(
        request_id,
        "extension_feature_not_negotiated",
        format!("extension feature was not negotiated: {feature}"),
        false,
    )
}

fn extension_result_failure(
    request_id: &str,
    code: CapabilityErrorCode,
    message: &str,
) -> ExtensionBridgeTextResult {
    let result = ExtensionResult {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        request_id: request_id.to_owned(),
        status: ExtensionResultStatus::Failed,
        output: Value::Null,
        effects: Vec::new(),
        error: Some(ExtensionError {
            code,
            message: message.to_owned(),
        }),
    };
    extension_bridge_success(
        request_id,
        serde_json::to_value(result).unwrap_or_default(),
        false,
    )
}

fn extension_runtime_failure(
    request_id: &str,
    error: loom_capability_runtime::CapabilityHostError,
) -> ExtensionBridgeTextResult {
    use loom_capability_runtime::CapabilityHostError as Error;
    let (code, retryable) = match error {
        Error::NotFound(_) => ("extension_command_not_found", false),
        Error::InvalidPackage(_) | Error::Protocol(_) => ("extension_request_rejected", false),
        Error::Busy | Error::Timeout => ("extension_runtime_busy", true),
        Error::Unavailable(_) | Error::Io(_) | Error::Process(_) | Error::Json(_) => {
            ("extension_runtime_unavailable", true)
        }
    };
    extension_bridge_failure(
        request_id,
        code,
        "extension runtime request failed",
        retryable,
    )
}

fn extension_bridge_success(
    request_id: &str,
    data: Value,
    subscribe_to_snapshots: bool,
) -> ExtensionBridgeTextResult {
    extension_bridge_response(
        request_id,
        ExtensionBridgeStatus::Succeeded,
        data,
        None,
        subscribe_to_snapshots,
    )
}

fn extension_bridge_failure(
    request_id: &str,
    code: &str,
    message: impl Into<String>,
    retryable: bool,
) -> ExtensionBridgeTextResult {
    extension_bridge_response(
        request_id,
        ExtensionBridgeStatus::Failed,
        Value::Null,
        Some(ExtensionBridgeError {
            code: code.to_owned(),
            message: message.into(),
            retryable,
        }),
        false,
    )
}

fn extension_bridge_response(
    request_id: &str,
    status: ExtensionBridgeStatus,
    data: Value,
    error: Option<ExtensionBridgeError>,
    subscribe_to_snapshots: bool,
) -> ExtensionBridgeTextResult {
    let response = ExtensionBridgeResponse {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        request_id: request_id.to_owned(),
        status,
        data,
        error,
    };
    ExtensionBridgeTextResult {
        response: serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_owned()),
        subscribe_to_snapshots,
    }
}

fn extension_snapshot_event(snapshot: ContributionSnapshot) -> ExtensionSnapshotEvent {
    ExtensionSnapshotEvent {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        method: EXTENSION_EVENT_SNAPSHOT_UPDATED.to_owned(),
        params: ExtensionSnapshotEventParams { snapshot },
    }
}
